use crate::model::{Clip, Generator, Media, Project, TextGenerator, TrackKind};

#[derive(Clone, Debug, PartialEq)]
pub struct Caption {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Build one transaction so failures never leave a partial subtitle track.
pub fn insert_captions(project: &Project, captions: &[Caption]) -> Result<(Project, u64), String> {
    if captions.is_empty() {
        return Err("没有识别到可生成字幕的语音".into());
    }
    let mut next = project.clone();
    let track = next.add_track(TrackKind::Video);
    next.reorder_track(track, 0);
    let font = crate::platform_paths::subtitle_font()?;
    let (width, height) = next.settings().dimensions();
    for caption in captions {
        if !caption.start.is_finite()
            || !caption.end.is_finite()
            || caption.start < 0.
            || caption.end <= caption.start
        {
            return Err("字幕时间戳无效".into());
        }
        let generator = TextGenerator {
            text: caption.text.clone(),
            font_file: font.clone(),
            font_size: 48.,
            color: "FFFFFF".into(),
        };
        generator.validate()?;
        let media = next.media.len();
        next.media.push(Media {
            path: Default::default(),
            name: "文本".into(),
            duration: caption.end - caption.start,
            width,
            height,
            audio: false,
            frame_rate: None,
            generator: Some(Generator::Text(generator)),
        });
        let id = next.next_id;
        next.next_id += 1;
        next.tracks[0].clips.push(Clip {
            id,
            media,
            start: caption.start,
            source_in: 0.,
            source_out: caption.end - caption.start,
            speed: 1.,
            volume: 1.,
            muted: false,
            fade_in: 0.,
            fade_out: 0.,
            fade_in_curve: Default::default(),
            fade_out_curve: Default::default(),
            group: None,
            transform: Default::default(),
        });
    }
    next.validate()?;
    Ok((next, track))
}

#[derive(serde::Deserialize)]
pub(super) struct Word {
    pub text: String,
    pub start_time: f64,
    pub end_time: f64,
}

pub(super) fn segment(words: Vec<Word>, offset: f64) -> Result<Vec<Caption>, String> {
    let mut captions: Vec<Caption> = Vec::new();
    let mut current: Option<Caption> = None;
    let mut previous = 0.;
    for word in words {
        if !word.start_time.is_finite()
            || !word.end_time.is_finite()
            || word.start_time < previous
            || word.end_time < word.start_time
        {
            return Err("对齐模型返回了无效或倒序的时间戳".into());
        }
        previous = word.start_time;
        if word.text.trim().is_empty() {
            continue;
        }
        // Keep captions readable: break at pauses, five seconds, or roughly one line.
        if current.as_ref().is_some_and(|c| {
            word.start_time + offset - c.end > 0.6
                || c.end - c.start >= 5.
                || c.text.chars().count() >= 24
        }) {
            captions.push(current.take().unwrap());
        }
        let caption = current.get_or_insert_with(|| Caption {
            start: word.start_time + offset,
            end: word.end_time + offset,
            text: String::new(),
        });
        if caption
            .text
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric())
            && word.text.starts_with(|c: char| c.is_ascii_alphanumeric())
        {
            caption.text.push(' ');
        }
        caption.text.push_str(word.text.trim());
        caption.end = word.end_time + offset;
        if word.text.ends_with(['。', '！', '？', '.', '!', '?']) && caption.end > caption.start
        {
            captions.push(current.take().unwrap());
        }
    }
    if let Some(caption) = current {
        captions.push(caption);
    }
    if captions.iter().any(|c| c.end <= c.start) {
        return Err("对齐结果包含没有有效时长的字幕".into());
    }
    Ok(captions)
}
