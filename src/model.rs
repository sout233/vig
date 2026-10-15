use serde::{Deserialize, Serialize};
use std::path::PathBuf;
mod fade_curve;
#[cfg(test)]
mod fades;
mod linking;
mod placement;
mod transitions;
mod trimming;
pub use fade_curve::FadeCurve;
mod generators;
mod subtitles;
pub use generators::Generator;
mod clipboard;
mod selection;
mod transform;
pub use clipboard::ClipClipboard;
pub use subtitles::TextGenerator;
pub use transform::Transform;
pub use trimming::TrimEdge;
mod video_settings;
pub use video_settings::{FrameRate, VideoSettings};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Media {
    pub path: PathBuf,
    pub name: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub audio: bool,
    #[serde(default)]
    pub frame_rate: Option<FrameRate>,
    #[serde(default)]
    pub generator: Option<Generator>,
}

impl Media {
    pub fn display_name(&self) -> String {
        if let Some(text) = self.generator.as_ref().and_then(Generator::as_text) {
            return text.text.lines().next().unwrap_or("").to_owned();
        }
        std::path::Path::new(&self.name)
            .with_extension("")
            .to_string_lossy()
            .into_owned()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: u64,
    pub media: usize,
    pub start: f64,
    pub source_in: f64,
    pub source_out: f64,
    pub speed: f64,
    pub volume: f64,
    pub muted: bool,
    pub fade_in: f64,
    pub fade_out: f64,
    #[serde(default)]
    pub fade_in_curve: FadeCurve,
    #[serde(default)]
    pub fade_out_curve: FadeCurve,
    #[serde(default)]
    pub group: Option<u64>,
    #[serde(default)]
    pub transform: Transform,
}

impl Clip {
    pub fn fit_fades(&mut self) {
        let total = self.fade_in + self.fade_out;
        if total > self.duration() {
            let ratio = self.duration() / total;
            self.fade_in *= ratio;
            self.fade_out *= ratio;
        }
    }

    pub fn brightness(&self, offset: f64) -> f64 {
        let incoming = if self.fade_in > 0. {
            self.fade_in_curve.value(offset / self.fade_in)
        } else {
            1.
        };
        let outgoing = if self.fade_out > 0. {
            1. - self
                .fade_out_curve
                .value((offset - self.duration() + self.fade_out) / self.fade_out)
        } else {
            1.
        };
        incoming.min(outgoing).clamp(0., 1.)
    }

    pub fn preview_times(&self) -> [u64; 3] {
        [0., 0.5, 0.95].map(|fraction| {
            ((self.source_in + (self.source_out - self.source_in) * fraction) * 1_000_000.).floor()
                as u64
        })
    }

    pub fn duration(&self) -> f64 {
        (self.source_out - self.source_in) / self.speed
    }

    pub fn end(&self) -> f64 {
        self.start + self.duration()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub id: u64,
    pub kind: TrackKind,
    pub clips: Vec<Clip>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
}

impl TrackKind {
    pub fn height(self) -> f32 {
        match self {
            Self::Video => 100.,
            Self::Audio => 72.,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub media: Vec<Media>,
    pub tracks: Vec<Track>,
    pub next_id: u64,
    pub next_track_id: u64,
    #[serde(default)]
    settings: VideoSettings,
    #[serde(default)]
    first_video_match_offered: bool,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: 9,
            name: "未命名项目".into(),
            media: vec![],
            tracks: vec![
                Track {
                    id: 1,
                    kind: TrackKind::Video,
                    clips: vec![],
                },
                Track {
                    id: 2,
                    kind: TrackKind::Audio,
                    clips: vec![],
                },
            ],
            next_id: 1,
            next_track_id: 3,
            settings: VideoSettings::default(),
            first_video_match_offered: false,
        }
    }
}

impl Project {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let old_version = value["version"].as_u64().ok_or("项目版本无效")?;
        let legacy_subtitles = value
            .as_object_mut()
            .ok_or("项目内容无效")?
            .remove("subtitles");
        if old_version == 7 {
            for media in value["media"].as_array_mut().ok_or("项目素材列表无效")? {
                if !media["generator"].is_null() {
                    let text = media["generator"].take();
                    media["generator"] = serde_json::json!({ "kind": "Text", "data": text });
                }
            }
        }
        if (1..=3).contains(&old_version) {
            let convert = |clip: &mut serde_json::Value| {
                if let Some(fade) = clip.as_object_mut().and_then(|c| c.remove("fade")) {
                    clip["fade_in"] = fade.clone();
                    clip["fade_out"] = fade;
                }
            };
            for key in ["video", "audio"] {
                if let Some(clips) = value.get_mut(key).and_then(|v| v.as_array_mut()) {
                    clips.iter_mut().for_each(&convert);
                }
            }
            if let Some(tracks) = value.get_mut("tracks").and_then(|v| v.as_array_mut()) {
                for track in tracks {
                    if let Some(clips) = track.get_mut("clips").and_then(|v| v.as_array_mut()) {
                        clips.iter_mut().for_each(&convert);
                    }
                }
            }
        }
        let mut project: Self = if old_version == 1 {
            #[derive(Deserialize)]
            struct Legacy {
                name: String,
                media: Vec<Media>,
                video: Vec<Clip>,
                audio: Vec<Clip>,
                next_id: u64,
            }
            let old: Legacy = serde_json::from_value(value).map_err(|e| e.to_string())?;
            Self {
                name: old.name,
                media: old.media,
                next_id: old.next_id,
                tracks: vec![
                    Track {
                        id: 1,
                        kind: TrackKind::Video,
                        clips: old.video,
                    },
                    Track {
                        id: 2,
                        kind: TrackKind::Audio,
                        clips: old.audio,
                    },
                ],
                ..Self::default()
            }
        } else {
            serde_json::from_value(value).map_err(|e| e.to_string())?
        };
        if old_version == 1 || old_version == 2 {
            let videos = project.video().cloned().collect::<Vec<_>>();
            for clip in videos {
                if project
                    .media
                    .get(clip.media)
                    .ok_or("片段素材引用无效")?
                    .audio
                {
                    project.separate_audio(clip);
                }
            }
        }
        if (1..=4).contains(&old_version) {
            project.sync_overlap_fades(&Self::default());
        }
        if (1..=7).contains(&old_version) {
            if let Some(subtitles) = legacy_subtitles {
                project.migrate_subtitles(subtitles)?;
            }
        }
        if (1..=8).contains(&old_version) {
            project.version = 9;
        }
        project.settings.validate()?;
        Ok(project)
    }

    pub fn video(&self) -> impl Iterator<Item = &Clip> {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video)
            .flat_map(|t| &t.clips)
    }

    pub fn add_track(&mut self, kind: TrackKind) -> u64 {
        let id = self.next_track_id;
        self.next_track_id += 1;
        self.tracks.push(Track {
            id,
            kind,
            clips: vec![],
        });
        id
    }

    pub fn reorder_track(&mut self, id: u64, index: usize) {
        let source = self
            .tracks
            .iter()
            .position(|t| t.id == id)
            .expect("轨道不存在");
        let track = self.tracks.remove(source);
        self.tracks.insert(index.min(self.tracks.len()), track);
    }

    pub fn clip_track(&self, id: u64) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.clips.iter().any(|c| c.id == id))
    }

    pub fn move_clip(&mut self, id: u64, track_id: u64, start: f64) -> Result<(), String> {
        let source = self.clip_track(id).ok_or("片段不存在")?;
        let target = self
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .ok_or("轨道不存在")?;
        if source.kind != target.kind {
            return Err("视频只能放入视频轨，音频只能放入音频轨".into());
        }
        let mut clip = self.clip(id).unwrap().clone();
        clip.start = start;
        if !start.is_finite() || start < 0. {
            return Err("目标位置无效".into());
        }
        let previous = self.clone();
        if self.clip_track(id).unwrap().id == track_id {
            self.clip_mut(id).unwrap().start = start;
            self.sync_overlap_fades(&previous);
            return Ok(());
        }
        self.tracks
            .iter_mut()
            .for_each(|t| t.clips.retain(|c| c.id != id));
        self.tracks
            .iter_mut()
            .find(|t| t.id == track_id)
            .unwrap()
            .clips
            .push(clip);
        self.sync_overlap_fades(&previous);
        Ok(())
    }

    pub fn duration(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(Clip::end)
            .fold(0., f64::max)
    }

    pub fn clip(&self, id: u64) -> Option<&Clip> {
        self.tracks
            .iter()
            .flat_map(|t| &t.clips)
            .find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: u64) -> Option<&mut Clip> {
        self.tracks
            .iter_mut()
            .flat_map(|t| &mut t.clips)
            .find(|c| c.id == id)
    }

    #[cfg(test)]
    pub fn append(&mut self, media: usize) -> u64 {
        self.insert_media(media, None, None).unwrap()
    }

    pub fn append_to(&mut self, media: usize, preferred: Option<u64>) -> u64 {
        let kind = if self.media[media].width > 0 {
            TrackKind::Video
        } else {
            TrackKind::Audio
        };
        let track_id = self
            .tracks
            .iter()
            .find(|t| Some(t.id) == preferred && t.kind == kind)
            .or_else(|| self.tracks.iter().find(|t| t.kind == kind))
            .map(|t| t.id)
            .unwrap_or_else(|| self.add_track(kind));
        let m = &self.media[media];
        let track = &mut self
            .tracks
            .iter_mut()
            .find(|t| t.id == track_id)
            .unwrap()
            .clips;
        let start = track.iter().map(Clip::end).fold(0., f64::max);
        let id = self.next_id;
        self.next_id += 1;
        track.push(Clip {
            id,
            media,
            start,
            source_in: 0.,
            source_out: m.duration,
            speed: 1.,
            volume: 1.,
            muted: false,
            fade_in: 0.,
            fade_out: 0.,
            fade_in_curve: FadeCurve::default(),
            fade_out_curve: FadeCurve::default(),
            group: None,
            transform: Transform::default(),
        });
        id
    }

    pub fn split(&mut self, id: u64, time: f64) -> Result<(), String> {
        let previous = self.clone();
        let track = &mut self
            .tracks
            .iter_mut()
            .find(|t| t.clips.iter().any(|c| c.id == id))
            .ok_or("请选择片段")?
            .clips;
        let index = track.iter().position(|c| c.id == id).ok_or("请选择片段")?;
        let clip = &mut track[index];
        if time <= clip.start + 0.001 || time >= clip.end() - 0.001 {
            return Err("播放头必须位于所选片段内部".into());
        }
        let source = clip.source_in + (time - clip.start) * clip.speed;
        let mut right = clip.clone();
        right.group = None;
        right.id = self.next_id;
        self.next_id += 1;
        right.start = time;
        right.source_in = source;
        clip.source_out = source;
        clip.fit_fades();
        right.fit_fades();
        track.insert(index + 1, right);
        self.sync_overlap_fades(&previous);
        Ok(())
    }

    #[cfg(test)]
    pub fn delete(&mut self, id: u64, ripple: bool) {
        let previous = self.clone();
        for track in self.tracks.iter_mut().map(|t| &mut t.clips) {
            if let Some(index) = track.iter().position(|c| c.id == id) {
                let clip = track.remove(index);
                if ripple {
                    for later in track {
                        if later.start >= clip.end() - 0.001 {
                            later.start -= clip.duration();
                        }
                    }
                }
            }
        }
        self.clean_links();
        self.sync_overlap_fades(&previous);
    }

    pub fn validate(&self) -> Result<(), String> {
        self.settings.validate()?;
        if self.version != 9 {
            return Err("不支持的项目版本".into());
        }
        for m in &self.media {
            if let Some(frame_rate) = m.frame_rate {
                frame_rate.validate()?;
            }
            if let Some(text) = &m.generator {
                text.validate()?;
                if m.width == 0 || m.audio {
                    return Err("生成式素材必须是无音频的视频素材".into());
                }
            } else if !m.path.is_file() {
                return Err(format!("素材不存在：{}", m.path.display()));
            }
            if !m.duration.is_finite() || m.duration <= 0. {
                return Err("素材时长无效".into());
            }
        }
        let mut ids = std::collections::HashSet::new();
        let mut track_ids = std::collections::HashSet::new();
        let mut groups = std::collections::HashMap::<u64, Vec<TrackKind>>::new();
        for track in &self.tracks {
            if track.id == 0 || track.id >= self.next_track_id || !track_ids.insert(track.id) {
                return Err("轨道标识无效".into());
            }
            let video = track.kind == TrackKind::Video;
            for c in &track.clips {
                if let Some(group) = c.group {
                    if group == 0 || group >= self.next_id {
                        return Err("绑定标识无效".into());
                    }
                    groups.entry(group).or_default().push(track.kind);
                }
                let m = self.media.get(c.media).ok_or("片段素材引用无效")?;
                c.transform.validate()?;
                if !ids.insert(c.id) || c.id >= self.next_id {
                    return Err("片段标识无效".into());
                }
                if [
                    c.start,
                    c.source_in,
                    c.source_out,
                    c.speed,
                    c.volume,
                    c.fade_in,
                    c.fade_out,
                ]
                .iter()
                .any(|v| !v.is_finite())
                    || c.start < 0.
                    || c.source_in < 0.
                    || (m.generator.is_none() && c.source_out > m.duration + 0.001)
                    || c.source_out <= c.source_in
                    || !(0.25..=4.).contains(&c.speed)
                    || !(0. ..=2.).contains(&c.volume)
                    || c.fade_in < 0.
                    || c.fade_out < 0.
                    || c.fade_in > c.duration()
                    || c.fade_out > c.duration()
                    || c.fade_in + c.fade_out > c.duration() + 0.000001
                    || (video && m.width == 0)
                    || (!video && !m.audio)
                {
                    return Err("片段参数无效".into());
                }
            }
        }
        if groups
            .values()
            .any(|kinds| kinds.len() != 2 || kinds[0] == kinds[1])
        {
            return Err("绑定组必须包含一个视频块和一个音频块".into());
        }
        Ok(())
    }
}

pub fn timecode(seconds: f64, frame_rate: FrameRate) -> String {
    let seconds = seconds.max(0.);
    let whole = seconds.floor() as u64;
    let frame = (seconds.fract() * frame_rate.fps()).floor() as u64;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        whole / 3600,
        whole / 60 % 60,
        whole % 60,
        frame
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Project {
        let mut p = Project::default();
        p.media.push(Media {
            path: "x.mp4".into(),
            name: "x".into(),
            duration: 10.,
            width: 1920,
            height: 1080,
            audio: false,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        p
    }

    #[test]
    fn split_preserves_source_and_duration() {
        let mut p = fixture();
        p.tracks[0].clips[0].speed = 2.;
        p.split(1, 2.).unwrap();
        assert_eq!(p.tracks[0].clips[0].source_out, 4.);
        assert_eq!(p.tracks[0].clips[1].source_in, 4.);
        assert_eq!(p.duration(), 5.);
    }

    #[test]
    fn ripple_closes_gap() {
        let mut p = fixture();
        p.append(0);
        p.delete(1, true);
        assert_eq!(p.tracks[0].clips[0].start, 0.);
    }

    #[test]
    fn split_at_boundary_does_not_mutate() {
        let mut p = fixture();
        assert!(p.split(1, 10.).is_err());
        assert_eq!(p.tracks[0].clips.len(), 1);
    }

    #[test]
    fn preview_frames_follow_source_trim_not_timeline_or_speed() {
        let mut p = fixture();
        let c = &mut p.tracks[0].clips[0];
        c.source_in = 2.;
        c.source_out = 6.;
        c.start = 20.;
        c.speed = 2.;
        assert_eq!(c.preview_times(), [2_000_000, 4_000_000, 5_800_000]);
        c.source_out = 4.;
        assert_eq!(c.preview_times(), [2_000_000, 3_000_000, 3_900_000]);
    }

    #[test]
    fn move_between_tracks_checks_type_and_allows_overlap() {
        let mut p = fixture();
        let second = p.add_track(TrackKind::Video);
        p.move_clip(1, second, 4.).unwrap();
        assert_eq!(p.clip_track(1).unwrap().id, second);
        assert!(p.tracks[0].clips.is_empty());
        let original = serde_json::to_string(&p).unwrap();
        assert!(p.move_clip(1, 2, 0.).is_err());
        assert_eq!(original, serde_json::to_string(&p).unwrap());
        let other = p.append_to(0, Some(second));
        p.move_clip(other, second, 5.).unwrap();
        p.reorder_track(second, 0);
        assert_eq!(p.tracks[0].id, second);
        assert_eq!(p.clip_track(1).unwrap().id, second);
        p.delete(1, true);
        assert_eq!(p.clip(other).unwrap().start, 5.);
    }

    #[test]
    fn empty_tracks_and_versioned_projects_roundtrip() {
        let mut p = Project::default();
        p.tracks.clear();
        p.validate().unwrap();
        p.add_track(TrackKind::Audio);
        p.add_track(TrackKind::Video);
        p.reorder_track(4, 0);
        let json = serde_json::to_string(&p).unwrap();
        let loaded = Project::from_json(&json).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.tracks[0].kind, TrackKind::Video);
        let old = r#"{"version":1,"name":"旧项目","media":[],"video":[],"audio":[],"next_id":1}"#;
        let converted = Project::from_json(old).unwrap();
        converted.validate().unwrap();
        assert_eq!(converted.version, 9);
        assert_eq!(converted.tracks.len(), 2);
        assert_eq!(converted.name, "旧项目");
    }
}
