use super::{
    Caption, ModelSize, Preferences,
    captions::{Word, segment},
    validate_model,
};
use crate::model::{Project, TrackKind};
use qwen3_asr::{
    AudioInput, LoadOptions, Qwen3Asr, TranscribeOptions, forced_aligner::Qwen3ForcedAligner,
};
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
pub struct AsrEngine {
    pub(super) loaded: Mutex<Option<LoadedModels>>,
}

pub(super) enum LoadedModels {
    Qwen {
        paths: (PathBuf, PathBuf),
        asr: Box<Qwen3Asr>,
        aligner: Box<Qwen3ForcedAligner>,
    },
    SenseVoice {
        path: PathBuf,
        recognizer: super::recognizer::Recognizer,
    },
}

pub fn source_ranges(project: &Project, track_id: u64) -> Result<Vec<(f64, f64)>, String> {
    let track = project
        .tracks
        .iter()
        .find(|track| track.id == track_id && track.kind == TrackKind::Audio)
        .ok_or("请选择音频轨道")?;
    let mut ranges: Vec<(f64, f64)> = Vec::new();
    for clip in track
        .ordered_clips()
        .into_iter()
        .filter(|clip| !clip.muted && clip.volume > 0.)
    {
        if let Some(last) = ranges.last_mut().filter(|range| clip.start <= range.1) {
            last.1 = last.1.max(clip.end());
        } else {
            ranges.push((clip.start, clip.end()));
        }
    }
    if ranges.is_empty() {
        return Err("所选轨道没有可识别的音频片段（空轨道或片段已静音）".into());
    }
    Ok(ranges)
}

pub fn generate(
    engine: &AsrEngine,
    mut project: Project,
    track_id: u64,
    preferences: Preferences,
    cancel: &AtomicBool,
    progress: impl Fn(String),
) -> Result<Vec<Caption>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("字幕生成已取消".into());
    }
    if preferences.selected == ModelSize::SenseVoice {
        return super::sensevoice::generate(
            engine,
            project,
            track_id,
            preferences,
            cancel,
            progress,
        );
    }
    let model = preferences
        .model()
        .ok_or("请先导入识别模型")?
        .canonicalize()
        .map_err(|e| format!("识别模型目录无法访问：{e}"))?;
    let aligner = preferences
        .aligner
        .as_ref()
        .ok_or("请先导入对齐模型")?
        .canonicalize()
        .map_err(|e| format!("对齐模型目录无法访问：{e}"))?;
    validate_model(&model, false, preferences.selected)?;
    validate_model(&aligner, true, ModelSize::Small)?;
    let ranges = source_ranges(&project, track_id)?;
    project.tracks.retain(|track| track.id == track_id);
    // Only audio remains in this private render snapshot; remove dangling AV groups.
    for clip in &mut project.tracks[0].clips {
        clip.group = None;
    }
    let paths = (model, aligner);
    let mut loaded = engine
        .loaded
        .lock()
        .map_err(|e| format!("识别引擎异常：{e}"))?;
    if !matches!(loaded.as_ref(), Some(LoadedModels::Qwen { paths: current, .. }) if current == &paths)
    {
        progress("正在加载识别和对齐模型（CPU）".into());
        // Release the previous pair before loading another model size.
        *loaded = None;
        let device = candle_core::Device::Cpu;
        let options = LoadOptions::default();
        let asr = Qwen3Asr::from_pretrained(
            paths.0.to_str().ok_or("模型目录不是有效 UTF-8 路径")?,
            &device,
            &options,
        )
        .map_err(|e| format!("加载识别模型失败：{e:#}"))?;
        if cancel.load(Ordering::Relaxed) {
            return Err("字幕生成已取消".into());
        }
        let aligner = Qwen3ForcedAligner::from_pretrained(
            paths.1.to_str().ok_or("对齐目录不是有效 UTF-8 路径")?,
            &device,
            &options,
        )
        .map_err(|e| format!("加载对齐模型失败：{e:#}"))?;
        *loaded = Some(LoadedModels::Qwen {
            paths,
            asr: Box::new(asr),
            aligner: Box::new(aligner),
        });
    }
    preferences.save()?;
    let Some(LoadedModels::Qwen { asr, aligner, .. }) = loaded.as_ref() else {
        unreachable!("Qwen models were loaded above");
    };
    let total = ranges.iter().map(|(start, end)| end - start).sum::<f64>();
    let mut completed = 0.;
    let mut words = Vec::new();
    for (range_start, range_end) in ranges {
        let mut start = range_start;
        while start < range_end {
            if cancel.load(Ordering::Relaxed) {
                return Err("字幕生成已取消".into());
            }
            // 30-second cores bound CPU memory; one second of shared context preserves boundary words.
            let end = (start + 30.).min(range_end);
            let decode_start = (start - 1.).max(range_start);
            let decode_end = (end + 1.).min(range_end);
            progress(format!(
                "正在识别 {:.0}% · 时间线 {:.1}–{:.1} 秒",
                completed / total * 100.,
                start,
                end
            ));
            let samples = crate::media::asr_audio::samples(&project, decode_start, decode_end)?;
            let mut results = asr
                .transcribe_with_forced_aligner(
                    aligner,
                    vec![AudioInput::Waveform {
                        samples: &samples,
                        sample_rate: 16000,
                    }],
                    TranscribeOptions {
                        return_timestamps: true,
                        max_batch_size: 1,
                        ..Default::default()
                    },
                )
                .map_err(|e| format!("语音识别或时间对齐失败：{e:#}"))?;
            let result = results.pop().ok_or("识别引擎没有返回结果")?;
            if let Some(timestamps) = result.timestamps {
                #[derive(serde::Deserialize)]
                struct Timestamps {
                    items: Vec<Word>,
                }
                let timestamps: Timestamps = serde_json::from_value(timestamps)
                    .map_err(|e| format!("字幕时间戳格式错误：{e}"))?;
                for mut word in timestamps.items {
                    if !word.start_time.is_finite()
                        || !word.end_time.is_finite()
                        || word.start_time < 0.
                        || word.end_time < word.start_time
                    {
                        return Err("对齐模型返回了无效的词语时间戳".into());
                    }
                    let middle = (word.start_time + word.end_time) / 2. + decode_start;
                    if middle >= start && middle < end {
                        word.start_time += decode_start;
                        word.end_time += decode_start;
                        words.push(word);
                    }
                }
            } else if !result.text.trim().is_empty() {
                return Err("识别到文字，但对齐模型没有返回时间戳".into());
            }
            completed += end - start;
            start = end;
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("字幕生成已取消".into());
    }
    if words.is_empty() {
        return Err("没有识别到可生成字幕的语音".into());
    }
    segment(words, 0.)
}
