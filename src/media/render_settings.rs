use super::Event;
use crate::model::{Project, VideoSettings};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool, mpsc::Sender},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderFormat {
    Mp4,
    Hevc,
    Mkv,
    WebM,
}

impl RenderFormat {
    pub const ALL: [Self; 4] = [Self::Mp4, Self::Hevc, Self::Mkv, Self::WebM];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4 · H.264 / AAC",
            Self::Hevc => "MP4 · H.265 / AAC",
            Self::Mkv => "MKV · H.264 / AAC",
            Self::WebM => "WebM · VP9 / Opus",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 | Self::Hevc => "mp4",
            Self::Mkv => "mkv",
            Self::WebM => "webm",
        }
    }

    pub(super) fn encode(self, cmd: &mut super::FilterCommand, quality: usize) {
        let crf = match self {
            Self::WebM => ["24", "31", "38"][quality],
            Self::Hevc => ["20", "24", "28"][quality],
            _ => ["17", "20", "25"][quality],
        };
        let video = match self {
            Self::Hevc => "libx265",
            Self::WebM => "libvpx-vp9",
            _ => "libx264",
        };
        cmd.args(["-c:v", video, "-crf", crf, "-pix_fmt", "yuv420p"]);
        if self == Self::WebM {
            cmd.args([
                "-b:v",
                "0",
                "-deadline",
                "good",
                "-cpu-used",
                "2",
                "-c:a",
                "libopus",
            ]);
        } else {
            cmd.args(["-preset", "medium", "-c:a", "aac"]);
        }
        if matches!(self, Self::Mp4 | Self::Hevc) {
            cmd.args(["-movflags", "+faststart"]);
        }
        if self == Self::Hevc {
            cmd.args(["-tag:v", "hvc1"]);
        }
        cmd.args(["-b:a", "192k", "-ar", "48000"]);
    }
}

#[derive(Clone)]
pub struct RenderSettings {
    pub video: VideoSettings,
    pub format: RenderFormat,
    /// 0: high quality, 1: balanced, 2: smaller file.
    pub quality: usize,
}

pub fn render_configured(
    project: &Project,
    output: &Path,
    settings: &RenderSettings,
    cancel: Arc<AtomicBool>,
    tx: &Sender<Event>,
) -> Result<(), String> {
    if settings.quality > 2 {
        return Err("渲染质量选项无效".into());
    }
    let mut snapshot = project.clone();
    snapshot.set_settings(settings.video.clone())?;
    super::render_encoded(
        &snapshot,
        output,
        false,
        settings.video.dimensions(),
        0.,
        snapshot.duration(),
        Some(settings),
        cancel,
        tx,
    )
}
