use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelSize {
    #[default]
    Small,
    Large,
    SenseVoice,
}

impl ModelSize {
    pub fn repository(self) -> &'static str {
        match self {
            Self::Small => "Qwen/Qwen3-ASR-0.6B",
            Self::Large => "Qwen/Qwen3-ASR-1.7B",
            Self::SenseVoice => "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Preferences {
    pub small: Option<PathBuf>,
    pub large: Option<PathBuf>,
    pub aligner: Option<PathBuf>,
    #[serde(default)]
    pub sensevoice: Option<PathBuf>,
    #[serde(default)]
    pub vad: Option<PathBuf>,
    pub selected: ModelSize,
    #[serde(default)]
    pub source: super::download::DownloadSource,
}

impl Preferences {
    pub fn model(&self) -> Option<&Path> {
        match self.selected {
            ModelSize::Small => self.small.as_deref(),
            ModelSize::Large => self.large.as_deref(),
            ModelSize::SenseVoice => self.sensevoice.as_deref(),
        }
    }

    fn path() -> Result<PathBuf, String> {
        Ok(crate::platform_paths::data_directory()?.join("asr.json"))
    }

    pub fn load() -> Result<Self, String> {
        match std::fs::read_to_string(Self::path()?) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("模型设置损坏：{e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("读取模型设置失败：{e}")),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        use std::io::Write;
        let path = Self::path()?;
        let parent = path.parent().ok_or("模型设置目录无效")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.persist(path)
            .map_err(|e| format!("保存模型设置失败：{e}"))?;
        Ok(())
    }
}

pub fn validate_model(path: &Path, aligner: bool, size: ModelSize) -> Result<(), String> {
    if !aligner && size == ModelSize::SenseVoice {
        for name in ["model.int8.onnx", "tokens.txt"] {
            if !path.join(name).is_file() {
                return Err(format!("SenseVoice 模型目录缺少 {name}"));
            }
        }
        return super::recognizer::validate_tokens(path);
    }
    let text = std::fs::read_to_string(path.join("config.json"))
        .map_err(|e| format!("无法读取 config.json：{e}"))?;
    let config: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("模型配置无效：{e}"))?;
    let thinker = &config["thinker_config"];
    let kind = thinker["model_type"]
        .as_str()
        .ok_or("模型缺少 thinker_config.model_type，请下载原始 Qwen3 模型")?;
    if kind.contains("forced_aligner") != aligner {
        return Err("识别模型与对齐模型选反了，请重新选择目录".into());
    }
    if !aligner {
        let layers = thinker["audio_config"]["encoder_layers"]
            .as_u64()
            .ok_or("无法确定识别模型大小")?;
        let expected = match size {
            ModelSize::Small => 18,
            ModelSize::Large => 24,
            ModelSize::SenseVoice => unreachable!("SenseVoice is validated separately"),
        };
        if layers != expected {
            return Err("目录中的模型与所选的 0.6B / 1.7B 不一致".into());
        }
    }
    if !path.join("tokenizer_config.json").is_file() {
        return Err("缺少 tokenizer_config.json".into());
    }
    if !path.join("tokenizer.json").is_file()
        && !(path.join("vocab.json").is_file() && path.join("merges.txt").is_file())
    {
        return Err("缺少 tokenizer.json 或 vocab.json + merges.txt".into());
    }
    if path.join("model.safetensors").is_file() {
        return Ok(());
    }
    let index = std::fs::read_to_string(path.join("model.safetensors.index.json"))
        .map_err(|e| format!("缺少模型权重或权重索引：{e}"))?;
    let index: serde_json::Value = serde_json::from_str(&index).map_err(|e| e.to_string())?;
    let shards = index["weight_map"]
        .as_object()
        .filter(|map| !map.is_empty())
        .ok_or("权重索引为空")?;
    for shard in shards.values() {
        let name = shard.as_str().ok_or("权重索引中的文件名无效")?;
        if Path::new(name)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("权重索引必须引用模型目录内的文件".into());
        }
        if !path.join(name).is_file() {
            return Err(format!("缺少权重分片：{name}"));
        }
    }
    Ok(())
}
