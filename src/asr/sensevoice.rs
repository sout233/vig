//! SenseVoice INT8 recognition with sample-based VAD timing, independent of Dictata code.
use super::recognizer::Recognizer;
use super::{AsrEngine, Caption, ModelSize, Preferences, inference::LoadedModels, vad::Vad};
use crate::model::Project;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn validate_vad(path: &Path) -> Result<(), String> {
    if !path.join("silero_vad.onnx").is_file() {
        return Err("VAD 模型目录缺少 silero_vad.onnx".into());
    }
    Ok(())
}

pub(super) fn generate(
    engine: &AsrEngine,
    mut project: Project,
    track_id: u64,
    preferences: Preferences,
    cancel: &AtomicBool,
    progress: impl Fn(String),
) -> Result<Vec<Caption>, String> {
    let path = preferences
        .model()
        .ok_or("请先下载或导入 SenseVoice 模型")?
        .canonicalize()
        .map_err(|e| format!("SenseVoice 目录无法访问：{e}"))?;
    super::validate_model(&path, false, ModelSize::SenseVoice)?;
    let vad_path = preferences
        .vad
        .as_ref()
        .ok_or("请先下载或导入 VAD 模型")?
        .canonicalize()
        .map_err(|e| format!("VAD 目录无法访问：{e}"))?;
    validate_vad(&vad_path)?;
    let ranges = super::source_ranges(&project, track_id)?;
    project.tracks.retain(|track| track.id == track_id);
    for clip in &mut project.tracks[0].clips {
        clip.group = None;
    }
    let mut loaded = engine
        .loaded
        .lock()
        .map_err(|e| format!("识别引擎异常：{e}"))?;
    if !matches!(loaded.as_ref(), Some(LoadedModels::SenseVoice { path: current, .. }) if current == &path)
    {
        progress("正在加载 SenseVoice INT8（CPU）".into());
        *loaded = None;
        let recognizer = Recognizer::new(&path)?;
        *loaded = Some(LoadedModels::SenseVoice { path, recognizer });
    }
    preferences.save()?;
    let Some(LoadedModels::SenseVoice { recognizer, .. }) = loaded.as_mut() else {
        unreachable!("SenseVoice was loaded above");
    };
    let total: f64 = ranges.iter().map(|(start, end)| end - start).sum();
    let mut completed = 0.;
    let mut captions = Vec::new();
    for (range_start, range_end) in ranges {
        // Retain VAD state across render blocks, reset only at actual timeline gaps.
        let mut vad = Vad::new(&vad_path)?;
        let mut start = range_start;
        while start < range_end {
            if cancel.load(Ordering::Relaxed) {
                return Err("字幕生成已取消".into());
            }
            let end = (start + 30.).min(range_end);
            progress(format!(
                "正在识别 SenseVoice {:.0}% · 时间线 {:.1}–{:.1} 秒",
                completed / total * 100.,
                start,
                end
            ));
            let samples = crate::media::asr_audio::samples(&project, start, end)?;
            for chunk in samples.chunks(512) {
                if cancel.load(Ordering::Relaxed) {
                    return Err("字幕生成已取消".into());
                }
                vad.accept(chunk);
                while let Some((offset, speech)) = vad.pop()? {
                    recognize(
                        recognizer,
                        &speech,
                        offset,
                        range_start,
                        range_end,
                        cancel,
                        &mut captions,
                    )?;
                }
            }
            completed += end - start;
            start = end;
        }
        vad.flush();
        while let Some((offset, speech)) = vad.pop()? {
            recognize(
                recognizer,
                &speech,
                offset,
                range_start,
                range_end,
                cancel,
                &mut captions,
            )?;
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("字幕生成已取消".into());
    }
    if captions.is_empty() {
        return Err("没有识别到可生成字幕的语音".into());
    }
    Ok(captions)
}

fn recognize(
    recognizer: &mut Recognizer,
    samples: &[f32],
    offset: usize,
    range_start: f64,
    range_end: f64,
    cancel: &AtomicBool,
    captions: &mut Vec<Caption>,
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("字幕生成已取消".into());
    }
    let text = recognizer.transcribe(samples)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("字幕生成已取消".into());
    }
    if let Some(caption) = timed_caption(&text, offset, samples.len(), range_start, range_end)? {
        captions.push(caption);
    }
    Ok(())
}

fn timed_caption(
    text: &str,
    offset: usize,
    length: usize,
    origin: f64,
    limit: f64,
) -> Result<Option<Caption>, String> {
    // SenseVoice may include language/emotion/event markers; these are not subtitles.
    let mut content = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<|") {
        content.push_str(&rest[..start]);
        let end = rest[start + 2..]
            .find("|>")
            .ok_or("SenseVoice 返回了未闭合的元数据标记")?;
        rest = &rest[start + 2 + end + 2..];
    }
    content.push_str(rest);
    let text = content.trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }
    let start = origin + offset as f64 / 16000.;
    let end = (start + length as f64 / 16000.).min(limit);
    if start < origin || end <= start || !end.is_finite() {
        return Err("SenseVoice 返回了无效字幕区间".into());
    }
    Ok(Some(Caption { start, end, text }))
}

#[cfg(test)]
mod tests {
    use super::timed_caption;

    #[test]
    fn vad_timing_retains_silence_and_timeline_offset() {
        let first = timed_caption("<|zh|><|NEUTRAL|>你好", 16000, 32000, 5., 20.)
            .unwrap()
            .unwrap();
        let next = timed_caption("世界", 128000, 16000, 5., 20.)
            .unwrap()
            .unwrap();
        assert_eq!(
            (first.start, first.end, first.text.as_str()),
            (6., 8., "你好")
        );
        assert_eq!((next.start, next.end), (13., 14.));
        assert!(
            timed_caption("<|nospeech|>", 0, 16000, 0., 1.)
                .unwrap()
                .is_none()
        );
        assert!(timed_caption("<|broken", 0, 16000, 0., 1.).is_err());
        assert!(timed_caption("你好", 32000, 16000, 0., 1.).is_err());
    }

    #[test]
    fn sensevoice_and_vad_import_require_the_correct_bundle() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            crate::asr::validate_model(root.path(), false, crate::asr::ModelSize::SenseVoice)
                .is_err()
        );
        std::fs::write(root.path().join("model.int8.onnx"), b"test fixture").unwrap();
        std::fs::write(
            root.path().join("tokens.txt"),
            b"<unk> 0\n<|zh|> 1\n<|en|> 2\n<|Speech|> 3\n<|withitn|> 4\n",
        )
        .unwrap();
        crate::asr::validate_model(root.path(), false, crate::asr::ModelSize::SenseVoice).unwrap();
        assert!(super::validate_vad(root.path()).is_err());
        std::fs::write(root.path().join("silero_vad.onnx"), b"test fixture").unwrap();
        super::validate_vad(root.path()).unwrap();
        // Existence is checked at import; invalid graph contents fail at model load.
        assert!(crate::asr::vad::Vad::new(root.path()).is_err());
        assert!(crate::asr::recognizer::Recognizer::new(root.path()).is_err());
    }

    #[test]
    #[ignore = "requires real SenseVoice, VAD and speech audio; downloads nothing"]
    fn real_sensevoice_generates_timed_chinese_subtitles() {
        use crate::{asr, media, model::Project};
        use std::sync::atomic::AtomicBool;
        let model = std::env::var_os("VIG_SENSEVOICE_MODEL").expect("set VIG_SENSEVOICE_MODEL");
        let vad = std::env::var_os("VIG_VAD_MODEL").expect("set VIG_VAD_MODEL");
        let audio = std::env::var_os("VIG_ASR_AUDIO").expect("set VIG_ASR_AUDIO");
        let mut project = Project::default();
        project
            .media
            .push(media::probe(std::path::Path::new(&audio)).unwrap());
        project.insert_media(0, Some(2), Some(5.)).unwrap();
        let preferences = asr::Preferences {
            selected: asr::ModelSize::SenseVoice,
            sensevoice: Some(model.into()),
            vad: Some(vad.into()),
            ..Default::default()
        };
        let captions = asr::generate(
            &asr::AsrEngine::default(),
            project.clone(),
            2,
            preferences,
            &AtomicBool::new(false),
            |message| eprintln!("{message}"),
        )
        .unwrap();
        for caption in &captions {
            eprintln!("{:.3}–{:.3}: {}", caption.start, caption.end, caption.text);
            assert!(caption.start >= 5. && caption.end > caption.start);
            assert!(!caption.text.contains("<|"));
        }
        assert!(captions.iter().any(|c| {
            c.text
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        }));
        asr::insert_captions(&project, &captions)
            .unwrap()
            .0
            .validate()
            .unwrap();
    }
}
