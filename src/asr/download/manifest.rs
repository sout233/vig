use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct Repository {
    sha: String,
    siblings: Vec<File>,
}

#[derive(Deserialize)]
pub(super) struct File {
    #[serde(rename = "rfilename")]
    pub name: String,
    pub size: u64,
    #[serde(rename = "blobId")]
    pub blob: String,
    pub lfs: Option<Lfs>,
}

#[derive(Deserialize)]
pub(super) struct Lfs {
    pub sha256: String,
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|c| c.is_ascii_hexdigit())
}

impl Repository {
    #[cfg(test)]
    pub(super) fn files(self) -> Result<(String, Vec<File>), String> {
        self.selected_files(None)
    }

    pub(super) fn selected_files(
        self,
        required: Option<&[&str]>,
    ) -> Result<(String, Vec<File>), String> {
        if !hex(&self.sha, 40) {
            return Err("模型清单缺少有效的版本标识".into());
        }
        let files: Vec<_> = self
            .siblings
            .into_iter()
            .filter(|file| {
                if let Some(required) = required {
                    return required.contains(&file.name.as_str());
                }
                matches!(
                    file.name.as_str(),
                    "config.json"
                        | "tokenizer_config.json"
                        | "tokenizer.json"
                        | "vocab.json"
                        | "merges.txt"
                        | "model.safetensors.index.json"
                ) || file.name.ends_with(".safetensors")
            })
            .collect();
        for file in &files {
            let path = std::path::Path::new(&file.name);
            if path.components().count() != 1
                || !matches!(
                    path.components().next(),
                    Some(std::path::Component::Normal(_))
                )
                || file.name.contains(['/', '\\', ':'])
                || file.name.ends_with(['.', ' '])
            {
                return Err(format!("模型清单中的文件名无效：{}", file.name));
            }
            let valid = match &file.lfs {
                Some(lfs) => hex(&lfs.sha256, 64),
                None => hex(&file.blob, 40),
            };
            if !valid {
                return Err(format!("模型清单缺少文件校验值：{}", file.name));
            }
        }
        let has = |name: &str| files.iter().any(|file| file.name == name);
        if let Some(required) = required {
            for name in required {
                if !has(name) {
                    return Err(format!("模型清单缺少 {name}"));
                }
            }
        } else if !has("config.json")
            || !has("tokenizer_config.json")
            || !(has("tokenizer.json") || has("vocab.json") && has("merges.txt"))
            || !files.iter().any(|file| file.name.ends_with(".safetensors"))
        {
            return Err("模型清单缺少配置、分词器或权重文件".into());
        }
        Ok((self.sha, files))
    }
}
