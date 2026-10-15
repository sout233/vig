use crate::model::{Media, Project};
pub mod asr_audio;
pub mod waveform;
use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

pub fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

pub fn probe(path: &Path) -> Result<Media, String> {
    let output = command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("无法运行 ffprobe：{e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let streams = data["streams"].as_array().ok_or("没有媒体流")?;
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1);
    let audio = streams.iter().any(|s| s["codec_type"] == "audio");
    let duration = data["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("无法读取素材时长；请导入视频或音频文件")?;
    if !duration.is_finite() || duration <= 0. || (video.is_none() && !audio) {
        return Err("素材没有可编辑的视频或音频流".into());
    }
    Ok(Media {
        path: path.to_owned(),
        name: path
            .file_name()
            .ok_or("文件名无效")?
            .to_string_lossy()
            .into_owned(),
        duration,
        width: video.and_then(|s| s["width"].as_u64()).unwrap_or(0) as u32,
        height: video.and_then(|s| s["height"].as_u64()).unwrap_or(0) as u32,
        audio,
        frame_rate: video
            .map(|stream| {
                stream["avg_frame_rate"]
                    .as_str()
                    .ok_or("无法读取视频帧率")?
                    .parse::<crate::model::FrameRate>()
            })
            .transpose()?,
        generator: None,
    })
}

pub fn frame(path: &Path, time: f64) -> Result<Vec<u8>, String> {
    let output = command("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{time:.6}"), "-i"])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:(ow-iw)/2:(oh-ih)/2",
            "-f",
            "image2pipe",
            "-c:v",
            "png",
            "pipe:1",
        ])
        .output()
        .map_err(|e| format!("无法运行 ffmpeg：{e}"))?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err(format!(
            "预览解码失败：{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

pub enum Event {
    Progress(f64),
    Frame(Vec<u8>, f64),
    PlaybackPosition(f64),
    Finished(Result<(), String>),
    Buffering(bool),
}
pub mod preview;
pub mod subtitles;
use subtitles::FilterCommand;
#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod tests;

pub fn render(
    project: &Project,
    output: &Path,
    preview: bool,
    cancel: Arc<AtomicBool>,
    tx: &Sender<Event>,
) -> Result<(), String> {
    render_range(project, output, preview, 0., project.duration(), cancel, tx)
}

fn filter_command(
    project: &Project,
    start: f64,
    end: f64,
    dimensions: (u32, u32),
    kind: Option<crate::model::TrackKind>,
) -> Result<FilterCommand, String> {
    project.validate()?;
    let duration = end - start;
    if !start.is_finite() || !end.is_finite() || start < 0. || end > project.duration() {
        return Err("预览或导出时间区间无效".into());
    }
    if duration <= 0. {
        return Err("时间线为空".into());
    }
    let (w, h) = dimensions;
    let fps = project.settings().frame_rate();
    let mut cmd = FilterCommand::plain(command("ffmpeg"));
    cmd.args([
        "-v",
        "error",
        "-nostdin",
        "-y",
        "-filter_complex_threads",
        "1",
    ]);
    // Upper tracks cover lower tracks; render from bottom to top.
    let clips = project
        .tracks
        .iter()
        .rev()
        .flat_map(|t| t.ordered_clips().into_iter().map(move |c| (t, c)))
        .filter(|(track, c)| {
            kind.is_none_or(|kind| track.kind == kind) && c.start < end && c.end() > start
        })
        .collect::<Vec<_>>();
    let mut inputs = Vec::new();
    let mut input_index = 0;
    for (_, clip) in &clips {
        if project.media[clip.media].generator.is_some() {
            inputs.push(None);
            continue;
        }
        inputs.push(Some(input_index));
        input_index += 1;
        cmd.args([
            "-ss",
            &format!(
                "{:.9}",
                clip.source_in + (start - clip.start).max(0.) * clip.speed
            ),
            "-t",
            &format!(
                "{:.9}",
                (clip.end().min(end) - clip.start.max(start)) * clip.speed
            ),
        ])
        .arg("-i")
        .arg(&project.media[clip.media].path);
    }
    let video = kind != Some(crate::model::TrackKind::Audio);
    let audio = kind != Some(crate::model::TrackKind::Video);
    let mut graph = String::new();
    if video {
        graph.push_str(&format!(
            "color=c=black:s={w}x{h}:r={fps}:d={duration:.6}[base];"
        ));
    }
    if audio {
        graph.push_str(&format!(
            "anullsrc=r=48000:cl=stereo,atrim=duration={duration:.6}[silence];"
        ));
    }
    let mut last = "base".to_owned();
    let mut audio_labels = vec!["[silence]".to_owned()];
    for (i, (track, c)) in clips.iter().enumerate() {
        let m = &project.media[c.media];
        let visible_start = c.start.max(start);
        let visible_end = c.end().min(end);
        let source_duration = (visible_end - visible_start) * c.speed;
        if track.kind == crate::model::TrackKind::Video {
            let alpha = format!(
                "({})*{:.9}",
                track.video_alpha_expression(c, &format!("(T+{visible_start:.9})")),
                c.transform.opacity
            );
            if let Some(text) = m
                .generator
                .as_ref()
                .and_then(crate::model::Generator::as_text)
            {
                graph.push_str(&format!(
                    "color=c=black@0:s={w}x{h}:r={fps}:d={:.9},format=rgba",
                    visible_end - visible_start
                ));
                subtitles::append_text(text, h, &alpha.replace('T', "t"), &mut graph, &mut cmd)?;
            } else if let Some(crate::model::Generator::Solid { color }) = &m.generator {
                graph.push_str(&format!(
                    "color=c=0x{color}:s={w}x{h}:r={fps}:d={:.9}",
                    visible_end - visible_start
                ));
            } else {
                let input = inputs[i].unwrap();
                graph.push_str(&format!("[{input}:v]trim=start=0:end={source_duration:.9},setpts=(PTS-STARTPTS)/{:.9},scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps}",c.speed));
            }
            let fading = c.fade_in > 0. && visible_start < c.start + c.fade_in
                || c.fade_out > 0. && visible_end > c.end() - c.fade_out;
            let overlapping = track.clips.iter().any(|other| {
                other.id != c.id && other.start < visible_end && other.end() > visible_start
            });
            if !matches!(&m.generator, Some(crate::model::Generator::Text(_)))
                && (fading || overlapping || c.transform.opacity != 1.)
            {
                // Opacity is uniform across a frame: evaluate four mask pixels,
                // then expand the mask instead of evaluating the expression at every video pixel.
                graph.push_str(&format!(",format=yuv420p[body{i}];color=c=white:s=2x2:r={fps}:d={:.9},format=gray,geq=lum='255*({alpha})',scale={w}:{h}:flags=neighbor[alpha{i}];[body{i}][alpha{i}]alphamerge", visible_end-visible_start));
            }
            let transform = &c.transform;
            if transform.scale != 1. || transform.rotation != 0. {
                graph.push_str(",format=rgba");
            }
            if transform.scale != 1. {
                graph.push_str(&format!(
                    ",scale=w='max(1,round(iw*{:.9}))':h='max(1,round(ih*{:.9}))'",
                    transform.scale, transform.scale
                ));
            }
            if transform.rotation != 0. {
                let angle = transform.rotation.to_radians();
                graph.push_str(&format!(
                    ",rotate={angle:.9}:ow='rotw({angle:.9})':oh='roth({angle:.9})':c=none"
                ));
            }
            graph.push_str(&format!(",setpts=PTS+{:.9}/TB[v{i}];[{last}][v{i}]overlay=x='(W-w)/2+W*{:.9}/100':y='(H-h)/2+H*{:.9}/100':eof_action=pass:enable='gte(t,{:.9})*lt(t,{:.9})'[o{i}];",visible_start-start,transform.x,transform.y,visible_start-start,visible_end-start));
            last = format!("o{i}");
        }
        if track.kind == crate::model::TrackKind::Audio && m.audio && !c.muted {
            let input = inputs[i].unwrap();
            graph.push_str(&format!("[{input}:a]atrim=start=0:end={source_duration:.9},asetpts=PTS-STARTPTS,aresample=48000,aformat=channel_layouts=stereo"));
            let mut speed = c.speed;
            while speed > 2. {
                graph.push_str(",atempo=2");
                speed /= 2.;
            }
            while speed < 0.5 {
                graph.push_str(",atempo=0.5");
                speed *= 2.;
            }
            graph.push_str(&format!(",atempo={speed:.6},volume={:.6}", c.volume));
            let time = format!("(t+{visible_start:.9})");
            let fade = c.fade_expression(&time);
            graph.push_str(&format!(
                ",volume='{fade}':eval=frame,adelay={:.0}:all=1[a{i}];",
                (visible_start - start) * 1000.
            ));
            audio_labels.push(format!("[a{i}]"));
        }
    }
    if audio {
        graph.push_str(&format!(
            "{}amix=inputs={}:normalize=0:duration=first[aout]",
            audio_labels.join(""),
            audio_labels.len()
        ));
    } else {
        graph.pop();
    }
    cmd.args(["-filter_complex", &graph]);
    if video {
        cmd.args(["-map", &format!("[{last}]")]);
    }
    if audio {
        cmd.args(["-map", "[aout]"]);
    }
    cmd.args(["-t", &format!("{duration:.9}")]);
    Ok(cmd)
}

pub fn render_range(
    project: &Project,
    output: &Path,
    preview: bool,
    start: f64,
    end: f64,
    cancel: Arc<AtomicBool>,
    tx: &Sender<Event>,
) -> Result<(), String> {
    let dimensions = if preview {
        preview::PreviewResolution::Half.dimensions(project.settings())
    } else {
        project.settings().dimensions()
    };
    render_range_at_size(project, output, preview, dimensions, start, end, cancel, tx)
}

fn render_range_at_size(
    project: &Project,
    output: &Path,
    preview: bool,
    dimensions: (u32, u32),
    start: f64,
    end: f64,
    cancel: Arc<AtomicBool>,
    tx: &Sender<Event>,
) -> Result<(), String> {
    if project.media.iter().any(|m| m.path == output) {
        return Err("导出路径不能覆盖源素材".into());
    }
    let duration = end - start;
    let mut cmd = filter_command(project, start, end, dimensions, None)?;
    let temp = tempfile::Builder::new()
        .suffix(".mp4")
        .tempfile_in(output.parent().ok_or("导出目录无效")?)
        .map_err(|e| e.to_string())?;
    cmd.args([
        "-c:v",
        "libx264",
        "-preset",
        if preview { "ultrafast" } else { "medium" },
        "-crf",
        if preview { "26" } else { "20" },
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-movflags",
        "+faststart",
        "-progress",
        "pipe:1",
    ])
    .arg(temp.path())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("无法运行 ffmpeg：{e}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let progress = tx.clone();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let line = line.map_err(|e| e.to_string())?;
            if let Some(t) = line
                .strip_prefix("out_time_us=")
                .and_then(|s| s.parse::<f64>().ok())
            {
                let _ = progress.send(Event::Progress((t / 1e6 / duration).clamp(0., 1.)));
            }
        }
        Ok::<_, String>(())
    });
    let error_reader = std::thread::spawn(move || {
        let mut error = String::new();
        BufReader::new(stderr)
            .read_to_string(&mut error)
            .map(|_| error)
            .map_err(|e| e.to_string())
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            child.kill().map_err(|e| e.to_string())?;
            break child.wait().map_err(|e| e.to_string())?;
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    reader.join().map_err(|_| "进度读取线程异常")??;
    let error = error_reader.join().map_err(|_| "错误读取线程异常")??;
    if cancel.load(Ordering::Relaxed) {
        return Err("操作已取消".into());
    }
    if !status.success() {
        return Err(format!("FFmpeg 导出失败：{error}"));
    }
    temp.persist(output)
        .map_err(|e| format!("保存输出文件失败：{e}"))?;
    Ok(())
}
