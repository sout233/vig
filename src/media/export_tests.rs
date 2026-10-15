use super::*;

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
