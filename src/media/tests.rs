use super::*;
use std::sync::mpsc;

fn generate(path: &Path, video: bool) {
    let mut cmd = command("ffmpeg");
    cmd.args(["-v", "error", "-y"]);
    if video {
        cmd.args(["-f", "lavfi", "-i", "color=c=red:s=320x180:r=30:d=3"]);
    }
    cmd.args([
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=3",
    ]);
    if video {
        cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"]);
    }
    let output = cmd.arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fade_curve_preview_pixels_match_the_model_for_every_curve() {
    use crate::model::{FadeCurve, Generator};
    let mut project = Project::default();
    let id = project
        .add_generator(
            Generator::Solid {
                color: "FFFFFF".into(),
            },
            0.,
            Some(1),
        )
        .unwrap();
    project.clip_mut(id).unwrap().fade_in = 1.;
    project.clip_mut(id).unwrap().fade_out = 1.;
    for (curve, label) in FadeCurve::ALL {
        let clip = project.clip_mut(id).unwrap();
        clip.fade_in_curve = curve;
        clip.fade_out_curve = curve;
        for time in [0.25, 2.25] {
            let expected = project.clip(id).unwrap().brightness(time) * 255.;
            let frame =
                image::load_from_memory(&subtitles::snapshot(&project, time, (640, 360)).unwrap())
                    .unwrap()
                    .into_rgb8();
            let actual = f64::from(frame.get_pixel(320, 180)[0]);
            assert!(
                (actual - expected).abs() < 8.,
                "{label} at {time}: actual {actual}, expected {expected}"
            );
        }
    }
}

#[test]
fn video_transform_controls_geometry_alpha_and_transparent_text_overlay() {
    use crate::model::{TrackKind, Transform};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("transform.mp4");
    generate(&source, true);
    let mut project = Project::default();
    project.media.push(probe(&source).unwrap());
    let video = project.append(0);
    project.clip_mut(video).unwrap().transform =
        Transform::from_values([25., 0., 50., 90., 50.]).unwrap();
    let transformed =
        image::load_from_memory(&subtitles::snapshot(&project, 1., (640, 360)).unwrap())
            .unwrap()
            .into_rgb8();
    let center = transformed.get_pixel(480, 180);
    assert!(
        (100..150).contains(&center[0]) && center[1] < 20 && center[2] < 20,
        "半透明旋转视频中心应位于右侧：{center:?}"
    );
    assert!(
        transformed.get_pixel(320, 180)[0] < 10,
        "平移后原中心应为空"
    );
    assert!(
        transformed.get_pixel(480, 30)[0] > 100,
        "旋转应使短边成为横向边"
    );
    assert!(transformed.get_pixel(480, 10)[0] < 10);
    project.clip_mut(video).unwrap().transform = Transform::default();
    let text = project.add_subtitle(0.).unwrap();
    let top = project.add_track(TrackKind::Video);
    project.move_clip(text, top, 0.).unwrap();
    project.reorder_track(top, 0);
    let overlaid = image::load_from_memory(&subtitles::snapshot(&project, 1., (640, 360)).unwrap())
        .unwrap()
        .into_rgb8();
    assert!(
        overlaid.get_pixel(10, 10)[0] > 220,
        "文本透明背景不能遮挡下面的视频"
    );
    assert!(
        overlaid
            .pixels()
            .filter(|pixel| pixel[1] > 180 && pixel[2] > 180)
            .count()
            > 100
    );
    project.clip_mut(text).unwrap().transform.opacity = 0.;
    let invisible =
        image::load_from_memory(&subtitles::snapshot(&project, 1., (640, 360)).unwrap())
            .unwrap()
            .into_rgb8();
    assert!(
        invisible
            .pixels()
            .all(|pixel| pixel[1] < 20 && pixel[2] < 20),
        "不透明度为零应隐藏文字和描边"
    );
}

#[test]
fn solid_color_fade_transform_opacity_and_export_match_preview() {
    use crate::model::{Generator, TrackKind};
    let mut project = Project::default();
    let blue = project
        .add_generator(
            Generator::Solid {
                color: "0000FF".into(),
            },
            0.,
            Some(1),
        )
        .unwrap();
    let top = project.add_track(TrackKind::Video);
    project.reorder_track(top, 0);
    let red = project
        .add_generator(
            Generator::Solid {
                color: "FF0000".into(),
            },
            0.,
            Some(top),
        )
        .unwrap();
    project.clip_mut(red).unwrap().transform.opacity = 0.5;
    let image = image::load_from_memory(&subtitles::snapshot(&project, 1., (640, 360)).unwrap())
        .unwrap()
        .into_rgb8();
    let center = image.get_pixel(320, 180);
    assert!(
        (110..145).contains(&center[0]) && (110..145).contains(&center[2]) && center[1] < 20,
        "纯色素材应透明叠加：{center:?}"
    );
    project.clip_mut(red).unwrap().fade_in = 1.;
    let fading = image::load_from_memory(&subtitles::snapshot(&project, 0.25, (640, 360)).unwrap())
        .unwrap()
        .into_rgb8();
    let expected = project.clip(red).unwrap().brightness(0.25) * 0.5 * 255.;
    assert!((f64::from(fading.get_pixel(320, 180)[0]) - expected).abs() < 8.);
    let clip = project.clip_mut(red).unwrap();
    clip.fade_in = 0.;
    clip.transform.scale = 0.5;
    clip.transform.rotation = 45.;
    let transformed =
        image::load_from_memory(&subtitles::snapshot(&project, 1., (640, 360)).unwrap())
            .unwrap()
            .into_rgb8();
    assert!(transformed.get_pixel(10, 10)[2] > 220);
    assert!((110..145).contains(&transformed.get_pixel(320, 180)[0]));
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("solid.mp4");
    let (tx, _) = mpsc::channel();
    render_range(
        &project,
        &output,
        true,
        0.5,
        1.5,
        Arc::new(AtomicBool::new(false)),
        &tx,
    )
    .unwrap();
    let exported = image::load_from_memory(&frame(&output, 0.5).unwrap())
        .unwrap()
        .into_rgb8();
    assert!((100..150).contains(&exported.get_pixel(320, 180)[0]));
    project.delete(blue, false);
    assert!(
        project
            .media
            .iter()
            .all(|source| source.generator.is_some())
    );
}

#[test]
fn real_export_trim_speed_gaps_audio_and_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("中文素材.mp4");
    let music = dir.path().join("配乐.wav");
    generate(&source, true);
    generate(&music, false);
    let mut project = Project::default();
    project.media.push(probe(&source).unwrap());
    project.media.push(probe(&music).unwrap());
    let id = project.append(0);
    let music_track = project.add_track(crate::model::TrackKind::Audio);
    project.insert_media(1, Some(music_track), None).unwrap();
    let clip = project.clip_mut(id).unwrap();
    clip.source_in = 0.5;
    clip.source_out = 2.5;
    clip.speed = 2.;
    clip.start = 1.;
    clip.volume = 0.5;
    clip.fade_in = 0.2;
    clip.fade_out = 0.2;
    let mut sound = clip.clone();
    sound.id = 2;
    *project.clip_mut(2).unwrap() = sound;
    project.validate().unwrap();
    let json = serde_json::to_vec(&project).unwrap();
    let loaded: Project = serde_json::from_slice(&json).unwrap();
    loaded.validate().unwrap();
    let output = dir.path().join("输出.mp4");
    let (tx, _rx) = mpsc::channel();
    render(
        &loaded,
        &output,
        true,
        Arc::new(AtomicBool::new(false)),
        &tx,
    )
    .unwrap();
    let metadata = probe(&output).unwrap();
    use rodio::Source;
    let mut decoder = rodio::Decoder::try_from(std::fs::File::open(&output).unwrap()).unwrap();
    decoder.try_seek(Duration::from_secs_f64(1.5)).unwrap();
    assert!(
        decoder.take(4800).any(|s| s.abs() > 0.001),
        "MP4 音频需要能够解码和定位"
    );
    assert!((metadata.duration - 3.).abs() < 0.1);
    assert!(metadata.audio);
    assert_eq!(metadata.width, 960);
    let black = image::load_from_memory(&frame(&output, 0.5).unwrap())
        .unwrap()
        .into_rgb8();
    let red = image::load_from_memory(&frame(&output, 1.5).unwrap())
        .unwrap()
        .into_rgb8();
    let after = image::load_from_memory(&frame(&output, 2.5).unwrap())
        .unwrap()
        .into_rgb8();
    assert!(black.get_pixel(320, 180)[0] < 10);
    assert!(red.get_pixel(320, 180)[0] > 200);
    assert!(red.get_pixel(320, 180)[1] < 20);
    assert!(after.get_pixel(320, 180)[0] < 10);
    let audio = command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-vn", "-f", "f32le", "-ac", "1", "pipe:1"])
        .output()
        .unwrap();
    assert!(audio.status.success());
    let samples = audio
        .stdout
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert!(
        samples.iter().any(|s| s.abs() > 0.02),
        "导出音频必须包含真实波形"
    );
}

#[test]
fn cancelled_export_preserves_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    generate(&source, true);
    let mut p = Project::default();
    p.media.push(probe(&source).unwrap());
    p.append(0);
    let output = dir.path().join("output.mp4");
    std::fs::write(&output, b"existing output").unwrap();
    let (tx, _) = mpsc::channel();
    assert!(render(&p, &output, true, Arc::new(AtomicBool::new(true)), &tx).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"existing output");
}

#[test]
fn audio_only_export_and_mute() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("sound.wav");
    generate(&source, false);
    let mut p = Project::default();
    p.media.push(probe(&source).unwrap());
    let id = p.append(0);
    p.clip_mut(id).unwrap().muted = true;
    let output = dir.path().join("output.mp4");
    let (tx, _) = mpsc::channel();
    render(&p, &output, true, Arc::new(AtomicBool::new(false)), &tx).unwrap();
    let audio = command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(output)
        .args(["-vn", "-f", "f32le", "-ac", "1", "pipe:1"])
        .output()
        .unwrap();
    assert!(audio.status.success());
    assert!(
        audio
            .stdout
            .chunks_exact(4)
            .all(|c| f32::from_le_bytes(c.try_into().unwrap()).abs() < 0.001)
    );
}

#[test]
fn allows_overlapping_projects_but_rejects_invalid_parameters() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("sound.wav");
    generate(&source, false);
    let mut p = Project::default();
    p.media.push(probe(&source).unwrap());
    p.append(0);
    p.append(0);
    p.tracks[1].clips[1].start = 1.;
    p.validate().unwrap();
    p.tracks[1].clips[1].start = 3.;
    p.tracks[1].clips[0].speed = f64::NAN;
    assert!(p.validate().is_err());
}

#[test]
fn multiple_video_tracks_follow_stacking_order_and_gaps() {
    use crate::model::TrackKind;
    let dir = tempfile::tempdir().unwrap();
    let red = dir.path().join("red.mp4");
    let blue = dir.path().join("blue.mp4");
    generate(&red, true);
    let status = command("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x180:r=30:d=3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&blue)
        .status()
        .unwrap();
    assert!(status.success());
    let mut p = Project::default();
    p.media.push(probe(&red).unwrap());
    p.media.push(probe(&blue).unwrap());
    let top = p.append(0);
    p.clip_mut(top).unwrap().source_out = 1.;
    let lower = p.add_track(TrackKind::Video);
    p.append_to(1, Some(lower));
    let (tx, _) = mpsc::channel();
    let output = dir.path().join("layers.mp4");
    render(&p, &output, true, Arc::new(AtomicBool::new(false)), &tx).unwrap();
    let pixel = |time| {
        image::load_from_memory(&frame(&output, time).unwrap())
            .unwrap()
            .into_rgb8()
            .get_pixel(320, 180)
            .0
    };
    assert!(pixel(0.5)[0] > 200, "上方视频轨应覆盖下方轨道");
    assert!(pixel(1.5)[2] > 200, "上方轨道空隙应显示下方视频");
    p.reorder_track(lower, 0);
    render(&p, &output, true, Arc::new(AtomicBool::new(false)), &tx).unwrap();
    assert!(pixel(0.5)[2] > 200, "调整顺序必须改变导出叠层");
}

#[test]
fn muting_separated_audio_keeps_video_and_removes_all_sound() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    generate(&source, true);
    let mut p = Project::default();
    p.media.push(probe(&source).unwrap());
    p.append(0);
    p.clip_mut(2).unwrap().muted = true;
    p.validate().unwrap();
    let output = dir.path().join("muted.mp4");
    let (tx, _) = mpsc::channel();
    render(&p, &output, true, Arc::new(AtomicBool::new(false)), &tx).unwrap();
    let video = image::load_from_memory(&frame(&output, 1.).unwrap())
        .unwrap()
        .into_rgb8();
    assert!(
        video.get_pixel(320, 180)[0] > 200,
        "静音音频块不能影响视频块"
    );
    let audio = command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-vn", "-f", "f32le", "-ac", "1", "pipe:1"])
        .output()
        .unwrap();
    assert!(audio.status.success());
    assert!(
        audio
            .stdout
            .chunks_exact(4)
            .all(|c| f32::from_le_bytes(c.try_into().unwrap()).abs() < 0.001),
        "视频块不能再次输出嵌入音频"
    );
}

#[test]
fn exported_video_has_independent_fade_in_and_out() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("red.mp4");
    generate(&source, true);
    let mut p = Project::default();
    p.media.push(probe(&source).unwrap());
    p.append(0);
    let c = p.clip_mut(1).unwrap();
    c.fade_in = 0.5;
    c.fade_out = 1.5;
    let output = dir.path().join("fades.mp4");
    let (tx, _) = mpsc::channel();
    render(&p, &output, true, Arc::new(AtomicBool::new(false)), &tx).unwrap();
    let red = |time| {
        image::load_from_memory(&frame(&output, time).unwrap())
            .unwrap()
            .into_rgb8()
            .get_pixel(320, 180)[0]
    };
    assert!((80..170).contains(&red(0.25)), "淡入区间应逐渐变亮");
    assert!(red(1.) > 200, "淡入结束后应恢复原亮度");
    assert!(red(2.7) < 80, "独立淡出区间应逐渐变暗");
    assert_eq!(p.clip(2).unwrap().fade_in, 0., "视频淡入不应改变独立音频块");
}
