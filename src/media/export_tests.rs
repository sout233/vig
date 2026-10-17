use super::*;

#[test]
fn configured_render_preserves_project_and_encodes_each_format() {
    use super::render_settings::{RenderFormat, RenderSettings, render_configured};
    use crate::model::{Generator, VideoSettings};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::default();
    project
        .add_generator(
            Generator::Solid {
                color: "336699".into(),
            },
            0.,
            None,
        )
        .unwrap();
    project
        .tracks
        .iter_mut()
        .flat_map(|track| &mut track.clips)
        .for_each(|clip| clip.source_out = 0.4);
    let before = serde_json::to_string(&project).unwrap();
    for format in RenderFormat::ALL {
        let settings = RenderSettings {
            video: VideoSettings::new(256, 144, "24000/1001".parse().unwrap()).unwrap(),
            format,
            quality: 1,
        };
        let path = dir
            .path()
            .join(format!("{format:?}.{}", format.extension()));
        let (tx, _) = std::sync::mpsc::channel();
        render_configured(
            &project,
            &path,
            &settings,
            Arc::new(AtomicBool::new(false)),
            &tx,
        )
        .unwrap();
        let metadata = probe(&path).unwrap();
        assert_eq!((metadata.width, metadata.height), (256, 144));
        assert!((metadata.frame_rate.unwrap().fps() - 24000. / 1001.).abs() < 0.001);
        assert!(metadata.audio);
        let output = command("ffprobe")
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let streams = data["streams"].as_array().unwrap();
        let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
        let audio = streams.iter().find(|s| s["codec_type"] == "audio").unwrap();
        assert_eq!(
            video["codec_name"],
            match format {
                RenderFormat::Hevc => "hevc",
                RenderFormat::WebM => "vp9",
                _ => "h264",
            }
        );
        assert_eq!(
            audio["codec_name"],
            if format == RenderFormat::WebM {
                "opus"
            } else {
                "aac"
            }
        );
        let decoded = command("ffmpeg")
            .args(["-v", "error", "-xerror", "-i"])
            .arg(path)
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
    }
    assert_eq!(serde_json::to_string(&project).unwrap(), before);
    let path = dir.path().join("cancelled.webm");
    std::fs::write(&path, b"existing output").unwrap();
    let settings = RenderSettings {
        video: project.settings().clone(),
        format: RenderFormat::WebM,
        quality: 1,
    };
    let (tx, _) = std::sync::mpsc::channel();
    assert!(
        render_configured(
            &project,
            &path,
            &settings,
            Arc::new(AtomicBool::new(true)),
            &tx
        )
        .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"existing output");
}

#[test]
fn overlapping_events_export_blended_video_and_faded_audio() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::default();
    for (color, frequency) in [("red", 440), ("blue", 880)] {
        let source = dir.path().join(format!("{color}.mp4"));
        let output = command("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!("color=c={color}:s=320x180:r=30:d=2"),
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency={frequency}:sample_rate=48000:duration=2"),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        project.media.push(probe(&source).unwrap());
    }
    project.insert_media(0, Some(1), Some(0.)).unwrap();
    project.insert_media(1, Some(1), Some(1.)).unwrap();
    project.move_clip(4, 2, 1.).unwrap();
    let destination = dir.path().join("crossfade.mp4");
    let (tx, _) = std::sync::mpsc::channel();
    render(
        &project,
        &destination,
        true,
        Arc::new(AtomicBool::new(false)),
        &tx,
    )
    .unwrap();
    let pixel = |time| {
        let decoded = image::load_from_memory(&frame(&destination, time).unwrap())
            .unwrap()
            .into_rgb8();
        *decoded.get_pixel(320, 180)
    };
    assert!(pixel(0.5)[0] > 220);
    let middle = pixel(1.5);
    assert!(
        (80..180).contains(&middle[0]) && (80..180).contains(&middle[2]),
        "重叠中点应混合红蓝而非黑场或直接切换：{middle:?}"
    );
    assert!(pixel(2.5)[2] > 220);
    let audio = command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&destination)
        .args(["-vn", "-f", "f32le", "-ac", "1", "-ar", "48000", "pipe:1"])
        .output()
        .unwrap();
    assert!(
        audio.status.success(),
        "{}",
        String::from_utf8_lossy(&audio.stderr)
    );
    let samples: Vec<_> = audio
        .stdout
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()) as f64)
        .collect();
    let amplitude = |start: f64, frequency: f64| {
        let begin = (start * 48000.) as usize;
        let count = 4800;
        let (mut sine, mut cosine) = (0., 0.);
        for (i, sample) in samples[begin..begin + count].iter().enumerate() {
            let phase = std::f64::consts::TAU * frequency * (begin + i) as f64 / 48000.;
            sine += sample * phase.sin();
            cosine += sample * phase.cos();
        }
        2. * sine.hypot(cosine) / count as f64
    };
    let red_ratio = amplitude(1.45, 440.) / amplitude(0.45, 440.);
    let blue_ratio = amplitude(1.45, 880.) / amplitude(2.45, 880.);
    assert!(
        (0.35..0.65).contains(&red_ratio),
        "原音频应渐出：{red_ratio}"
    );
    assert!(
        (0.35..0.65).contains(&blue_ratio),
        "新音频应渐入：{blue_ratio}"
    );
}

#[test]
fn full_hd_export_contains_video_and_audio() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    let output = command("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x180:r=30:d=0.6",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut project = Project::default();
    project.media.push(probe(&source).unwrap());
    project.append(0);
    let destination = dir.path().join("final.mp4");
    let (tx, _) = std::sync::mpsc::channel();
    render(
        &project,
        &destination,
        false,
        Arc::new(AtomicBool::new(false)),
        &tx,
    )
    .unwrap();
    let metadata = probe(&destination).unwrap();
    assert_eq!((metadata.width, metadata.height), (1920, 1080));
    assert!(metadata.audio);
    assert!((metadata.duration - 0.6).abs() < 0.1);
    let decoded = command("ffmpeg")
        .args(["-v", "error", "-xerror", "-i"])
        .arg(destination)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
}
