use super::*;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Mutex,
};
mod audio;
mod sync;
mod video;
use video::decode_video;
#[cfg(test)]
mod resolution_tests;
#[cfg(test)]
mod tests;

/// Cache granularity and read-ahead window, independent of total project length.
pub const SEGMENT_SECONDS: f64 = 2.;
// One second of PCM and 200 ms of video cover decoder startup.
// RGBA memory scales with the selected resolution (Full: ~47.5 MiB for six frames).
const AUDIO_AHEAD_PACKETS: usize = 10;

const VIDEO_AHEAD_FRAMES: usize = 6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewResolution {
    Full,
    #[default]
    Half,
    Quarter,
}

impl PreviewResolution {
    pub fn dimensions(self, settings: &crate::model::VideoSettings) -> (u32, u32) {
        let divisor = match self {
            Self::Full => 1,
            Self::Half => 2,
            Self::Quarter => 4,
        };
        let (width, height) = settings.dimensions();
        // Preview dimensions are rounded to even pixels for YUV 4:2:0 decoding.
        (
            (width / divisor / 2 * 2).max(2),
            (height / divisor / 2 * 2).max(2),
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
        }
    }
}

pub struct PreviewCache {
    root: PathBuf,
    resolution: PreviewResolution,
    settings: crate::model::VideoSettings,
    render_lock: Mutex<()>,
}

impl PreviewCache {
    pub fn resolution(&self) -> PreviewResolution {
        self.resolution
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.resolution.dimensions(&self.settings)
    }

    pub fn settings(&self) -> &crate::model::VideoSettings {
        &self.settings
    }

    pub fn new(
        root: PathBuf,
        resolution: PreviewResolution,
        settings: crate::model::VideoSettings,
    ) -> Self {
        Self {
            root,
            resolution,
            settings,
            render_lock: Mutex::new(()),
        }
    }

    fn identity(
        project: &Project,
        index: u64,
        resolution: PreviewResolution,
    ) -> Result<String, String> {
        let start = index as f64 * SEGMENT_SECONDS;
        let end = (start + SEGMENT_SECONDS).min(project.duration());
        let mut tracks = Vec::new();
        for track in &project.tracks {
            let mut clips = Vec::new();
            for c in track
                .ordered_clips()
                .into_iter()
                .filter(|c| c.start < end && c.end() > start)
            {
                let source = &project.media[c.media];
                if let Some(generator) = &source.generator {
                    clips.push(serde_json::json!({ "clip": c, "generator": generator }));
                    continue;
                }
                let metadata = std::fs::metadata(&source.path).map_err(|e| e.to_string())?;
                let modified = metadata
                    .modified()
                    .map_err(|e| e.to_string())?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos();
                clips.push(serde_json::json!({
                    "path": source.path, "bytes": metadata.len(), "modified": modified,
                    "start": c.start, "in": c.source_in, "out": c.source_out,
                    "speed": c.speed, "volume": c.volume, "muted": c.muted,
                    "fade_in": c.fade_in, "fade_out": c.fade_out,
                    "fade_in_curve": c.fade_in_curve, "fade_out_curve": c.fade_out_curve,
                    "transform": c.transform,
                }));
            }
            if !clips.is_empty() {
                tracks.push(serde_json::json!({"kind": track.kind, "clips": clips}));
            }
        }
        serde_json::to_string(&serde_json::json!({
            "format": "vig-preview-segments-v5", "dimensions": resolution.dimensions(project.settings()), "settings": project.settings(),
            "start": start, "end": end, "tracks": tracks,
        })).map_err(|e| e.to_string())
    }

    pub fn segment(
        &self,
        project: &Project,
        index: u64,
        cancel: Arc<AtomicBool>,
    ) -> Result<PathBuf, String> {
        let identity = Self::identity(project, index, self.resolution)?;
        let mut hash = DefaultHasher::new();
        identity.hash(&mut hash);
        let stem = format!("{:016x}", hash.finish());
        let _guard = self
            .render_lock
            .lock()
            .map_err(|e| format!("预览缓存锁损坏：{e}"))?;
        if cancel.load(Ordering::Relaxed) {
            return Err("操作已取消".into());
        }
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let path = self.root.join(format!("{stem}.mp4"));
        let manifest = self.root.join(format!("{stem}.json"));
        if manifest.is_file() {
            let stored = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
            if stored != identity {
                return Err("预览缓存键冲突".into());
            }
            if path.is_file() {
                return Ok(path);
            }
        } else {
            std::fs::write(&manifest, identity.as_bytes()).map_err(|e| e.to_string())?;
        }
        let start = index as f64 * SEGMENT_SECONDS;
        let end = (start + SEGMENT_SECONDS).min(project.duration());
        let (tx, _rx) = std::sync::mpsc::channel();
        render_range_at_size(
            project,
            &path,
            true,
            self.dimensions(),
            start,
            end,
            cancel,
            &tx,
        )?;
        Ok(path)
    }

    /// A cache miss starts live decoding; playback never waits for the render lock.
    fn ready(&self, project: &Project, index: u64) -> Result<Option<PathBuf>, String> {
        let identity = Self::identity(project, index, self.resolution)?;
        let mut hash = DefaultHasher::new();
        identity.hash(&mut hash);
        let stem = format!("{:016x}", hash.finish());
        let path = self.root.join(format!("{stem}.mp4"));
        let manifest = self.root.join(format!("{stem}.json"));
        if !path.is_file() || !manifest.is_file() {
            return Ok(None);
        }
        if std::fs::read_to_string(manifest).map_err(|e| e.to_string())? != identity {
            return Err("预览缓存键冲突".into());
        }
        Ok(Some(path))
    }
}

struct Segment {
    command: FilterCommand,
    dimensions: (u32, u32),
    frame_rate: crate::model::FrameRate,
    start: f64,
    end: f64,
}

/// The callback copies predecoded PCM only. Buffering is visible and freezes the
/// timeline clock; zero output during buffering never advances past missing audio.
struct AudioStream {
    packets: std::sync::mpsc::Receiver<Vec<f32>>,
    current: std::vec::IntoIter<f32>,
    consumed: Arc<std::sync::atomic::AtomicU64>,
    finished: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    events: Sender<Event>,
    buffering: bool,
    silence_second: bool,
}

impl Iterator for AudioStream {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.cancel.load(Ordering::Relaxed) {
            return None;
        }
        if self.silence_second {
            self.silence_second = false;
            return Some(0.);
        }
        loop {
            if let Some(sample) = self.current.next() {
                if self.buffering {
                    self.buffering = false;
                    let _ = self.events.send(Event::Buffering(false));
                }
                self.consumed.fetch_add(1, Ordering::Relaxed);
                return Some(sample);
            }
            match self.packets.try_recv() {
                Ok(samples) => self.current = samples.into_iter(),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if !self.buffering {
                        self.buffering = true;
                        let _ = self.events.send(Event::Buffering(true));
                    }
                    self.silence_second = true;
                    return Some(0.);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.finished.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        }
    }
}

impl rodio::Source for AudioStream {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        48000
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn send_ahead<T>(
    sender: &std::sync::mpsc::SyncSender<T>,
    mut value: T,
    cancel: &AtomicBool,
) -> Result<(), String> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("操作已取消".into());
        }
        match sender.try_send(value) {
            Ok(()) => return Ok(()),
            Err(std::sync::mpsc::TrySendError::Full(returned)) => {
                value = returned;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                return Err("预览读取端已关闭".into());
            }
        }
    }
}

pub fn playback(
    project: Project,
    cache: Arc<PreviewCache>,
    start: f64,
    cancel: Arc<AtomicBool>,
    tx: Sender<Event>,
) -> Result<(), String> {
    let dimensions = cache.dimensions();
    let frame_rate = project.settings().frame_rate();
    let device = rodio::OutputStreamBuilder::from_default_device()
        .map_err(|e| format!("无法找到默认音频设备：{e}"))?
        .open_stream()
        .map_err(|e| format!("无法打开音频设备：{e}"))?;
    let sink = rodio::Sink::connect_new(device.mixer());
    sink.pause();
    let (audio_tx, audio_rx) = std::sync::mpsc::sync_channel(AUDIO_AHEAD_PACKETS);
    let (video_tx, video_rx) = std::sync::mpsc::sync_channel(2);
    let (frames_tx, frames_rx) = std::sync::mpsc::sync_channel(VIDEO_AHEAD_FRAMES);
    let consumed = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    sink.append(AudioStream {
        packets: audio_rx,
        current: Vec::new().into_iter(),
        consumed: consumed.clone(),
        finished: finished.clone(),
        cancel: cancel.clone(),
        events: tx.clone(),
        buffering: false,
        silence_second: false,
    });
    std::thread::scope(|scope| {
        let decode_cancel = cancel.clone();
        let decoder = scope.spawn(move || -> Result<(), String> {
            loop {
                if decode_cancel.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let mut segment = match video_rx.recv_timeout(Duration::from_millis(40)) {
                    Ok(segment) => segment,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                };
                if let Err(error) = decode_video(&mut segment, &frames_tx, &decode_cancel) {
                    send_ahead(&frames_tx, Err(error.clone()), &decode_cancel)?;
                    return Err(error);
                }
            }
        });
        let producer_cancel = cancel.clone();
        let producer = scope.spawn(move || -> Result<(), String> {
            let first = (start / SEGMENT_SECONDS).floor() as u64;
            let count = (project.duration() / SEGMENT_SECONDS).ceil() as u64;
            for index in first..count {
                if producer_cancel.load(Ordering::Relaxed) {
                    return Err("操作已取消".into());
                }
                let segment_start = index as f64 * SEGMENT_SECONDS;
                let offset = if index == first {
                    start - segment_start
                } else {
                    0.
                };
                let visible_start = segment_start + offset;
                let end = (segment_start + SEGMENT_SECONDS).min(project.duration());
                let cached = cache.ready(&project, index)?;
                let mut video = if let Some(path) = &cached {
                    let mut command = command("ffmpeg");
                    command
                        .args([
                            "-v",
                            "error",
                            "-nostdin",
                            "-ss",
                            &format!("{offset:.9}"),
                            "-i",
                        ])
                        .arg(path)
                        .args([
                            "-an",
                            "-vf",
                            &format!("fps={frame_rate},scale={}:{}", dimensions.0, dimensions.1),
                            "-t",
                            &format!("{:.9}", end - visible_start),
                        ]);
                    FilterCommand::plain(command)
                } else {
                    filter_command(
                        &project,
                        visible_start,
                        end,
                        dimensions,
                        Some(crate::model::TrackKind::Video),
                    )?
                };
                video.args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
                send_ahead(
                    &video_tx,
                    Segment {
                        command: video,
                        dimensions,
                        frame_rate,
                        start: visible_start,
                        end,
                    },
                    &producer_cancel,
                )?;
                audio::load(
                    &project,
                    visible_start,
                    end,
                    cached.as_deref().map(|path| (path, offset)),
                    &audio_tx,
                    &producer_cancel,
                )?;
            }
            Ok(())
        });
        let result: Result<(), String> = (|| {
            let mut started = false;
            let mut scheduler = sync::FrameScheduler::new(frames_rx, frame_rate.frame_duration());
            let mut published_tick = None;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let clock = start + consumed.load(Ordering::Relaxed) as f64 / 96000.;
                let tick = (clock * frame_rate.fps()).floor() as u64;
                if published_tick != Some(tick) {
                    tx.send(Event::PlaybackPosition(clock))
                        .map_err(|e| e.to_string())?;
                    published_tick = Some(tick);
                }
                let Some((pixels, position)) = scheduler.at(clock)? else {
                    if scheduler.finished() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                if !started {
                    sink.play();
                    started = true;
                    tx.send(Event::Buffering(false))
                        .map_err(|e| e.to_string())?;
                }
                if cancel.load(Ordering::Relaxed) {
                    return Ok(());
                }
                tx.send(Event::PlaybackPosition(clock))
                    .map_err(|e| e.to_string())?;
                tx.send(Event::Frame(pixels, position))
                    .map_err(|e| e.to_string())?;
            }
            while started && !sink.empty() && !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        })();
        cancel.store(true, Ordering::Relaxed);
        sink.stop();
        let loaded = producer.join().map_err(|_| "预览加载线程异常")?;
        let decoded = decoder.join().map_err(|_| "预览视频解码线程异常")?;
        result?;
        decoded?;
        loaded
    })
}
