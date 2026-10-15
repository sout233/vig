use super::*;

#[test]
fn generated_content_and_event_transform_invalidate_cached_preview() {
    let mut project = Project::default();
    let id = project.add_subtitle(0.).unwrap();
    let initial = PreviewCache::identity(&project, 0, PreviewResolution::Half).unwrap();
    project.clip_mut(id).unwrap().transform.rotation = 30.;
    let transformed = PreviewCache::identity(&project, 0, PreviewResolution::Half).unwrap();
    assert_ne!(initial, transformed);
    let media = project.clip(id).unwrap().media;
    project.media[media]
        .generator
        .as_mut()
        .unwrap()
        .as_text_mut()
        .unwrap()
        .text = "更新文字".into();
    assert_ne!(
        transformed,
        PreviewCache::identity(&project, 0, PreviewResolution::Half).unwrap()
    );
}
use crate::model::Media;

#[test]
fn only_content_in_the_cached_interval_changes_its_identity() {
    let source = tempfile::NamedTempFile::new().unwrap();
    let mut project = Project::default();
    project.media.push(Media {
        path: source.path().into(),
        name: "视频.mp4".into(),
        duration: 4.,
        width: 320,
        height: 180,
        audio: false,
        frame_rate: None,
        generator: None,
    });
    project.insert_media(0, Some(1), Some(0.)).unwrap();
    let later = project.insert_media(0, Some(1), Some(8.)).unwrap();
    let first_key = PreviewCache::identity(&project, 0, PreviewResolution::Half).unwrap();
    let later_key = PreviewCache::identity(&project, 4, PreviewResolution::Half).unwrap();
    project.name = "另一个项目名称".into();
    project.media[0].name = "改名不影响画面".into();
    assert_eq!(
        first_key,
        PreviewCache::identity(&project, 0, PreviewResolution::Half).unwrap()
    );
    project.clip_mut(later).unwrap().fade_in = 0.5;
    assert_eq!(
        first_key,
        PreviewCache::identity(&project, 0, PreviewResolution::Half).unwrap()
    );
    assert_ne!(
        later_key,
        PreviewCache::identity(&project, 4, PreviewResolution::Half).unwrap()
    );
    let dir = tempfile::tempdir().unwrap();
    let cache = PreviewCache::new(
        dir.path().into(),
        PreviewResolution::Half,
        project.settings().clone(),
    );
    let _busy_renderer = cache.render_lock.lock().unwrap();
    assert!(
        cache.ready(&project, 0).unwrap().is_none(),
        "缓存未完成时不能等待后台渲染锁"
    );
    let video = filter_command(
        &project,
        0.,
        2.,
        (640, 360),
        Some(crate::model::TrackKind::Video),
    )
    .unwrap();
    let video_args: Vec<_> = video.get_args().map(|arg| arg.to_string_lossy()).collect();
    let video_graph = &video_args[video_args
        .iter()
        .position(|arg| arg == "-filter_complex")
        .unwrap()
        + 1];
    assert!(!video_graph.contains("amix") && !video_graph.contains("anullsrc"));
    let audio = filter_command(
        &project,
        0.,
        2.,
        (640, 360),
        Some(crate::model::TrackKind::Audio),
    )
    .unwrap();
    let audio_args: Vec<_> = audio.get_args().map(|arg| arg.to_string_lossy()).collect();
    let audio_graph = &audio_args[audio_args
        .iter()
        .position(|arg| arg == "-filter_complex")
        .unwrap()
        + 1];
    assert!(
        !audio_graph.contains("overlay")
            && !audio_graph.contains("geq")
            && !audio_graph.contains("color=")
    );
}

#[test]
fn buffering_never_advances_the_audio_clock_or_changes_channel_order() {
    let (sender, receiver) = std::sync::mpsc::sync_channel(2);
    let (events, notifications) = std::sync::mpsc::channel();
    let consumed = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let mut stream = AudioStream {
        packets: receiver,
        current: Vec::new().into_iter(),
        consumed: consumed.clone(),
        finished: finished.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        events,
        buffering: false,
        silence_second: false,
    };
    assert_eq!(stream.next(), Some(0.));
    sender.send(vec![0.25, 0.75]).unwrap();
    assert_eq!(stream.next(), Some(0.), "缓冲中的立体声帧必须完整结束");
    assert_eq!(consumed.load(Ordering::Relaxed), 0);
    assert!(matches!(
        notifications.try_recv().unwrap(),
        Event::Buffering(true)
    ));
    assert_eq!(stream.next(), Some(0.25));
    assert_eq!(stream.next(), Some(0.75));
    assert_eq!(consumed.load(Ordering::Relaxed), 2);
    assert!(matches!(
        notifications.try_recv().unwrap(),
        Event::Buffering(false)
    ));
    drop(sender);
    assert_eq!(stream.next(), None);
    assert!(finished.load(Ordering::Relaxed));
}

#[test]
fn preview_cache_reuses_completed_segments_and_ranges_keep_absolute_fades() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    let result = command("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=320x180:r=30:d=4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=4",
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
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut project = Project::default();
    project.media.push(probe(&source).unwrap());
    project.insert_media(0, Some(1), Some(0.)).unwrap();
    for id in [1, 2] {
        project.clip_mut(id).unwrap().fade_out = 1.;
    }
    let cache = PreviewCache::new(
        dir.path().join("cache"),
        PreviewResolution::Half,
        project.settings().clone(),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let first = cache.segment(&project, 0, cancel.clone()).unwrap();
    let modified = std::fs::metadata(&first).unwrap().modified().unwrap();
    assert_eq!(first, cache.segment(&project, 0, cancel.clone()).unwrap());
    assert_eq!(
        modified,
        std::fs::metadata(&first).unwrap().modified().unwrap()
    );
    let second = cache.segment(&project, 1, cancel).unwrap();
    assert!((probe(&second).unwrap().duration - 2.).abs() < 0.1);
    let early = image::load_from_memory(&frame(&second, 0.5).unwrap())
        .unwrap()
        .into_rgb8();
    let fading = image::load_from_memory(&frame(&second, 1.5).unwrap())
        .unwrap()
        .into_rgb8();
    assert!(early.get_pixel(320, 180)[0] > 220);
    assert!(
        (80..180).contains(&fading.get_pixel(320, 180)[0]),
        "区间内渐变应按原时间线位置计算"
    );
    let mut overlapping = project.clone();
    overlapping.insert_media(0, Some(1), Some(1.)).unwrap();
    let mut command = filter_command(
        &overlapping,
        1.,
        3.,
        (640, 360),
        Some(crate::model::TrackKind::Video),
    )
    .unwrap();
    command.args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
    let mut segment = Segment {
        command,
        dimensions: (640, 360),
        frame_rate: Default::default(),
        start: 1.,
        end: 3.,
    };
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(VIDEO_AHEAD_FRAMES);
        let decoder = scope.spawn(move || {
            let cancel = AtomicBool::new(false);
            let result = decode_video(&mut segment, &sender, &cancel);
            drop(sender);
            result
        });
        let mut count = 0;
        for frame in receiver {
            let (pixels, position) = frame.unwrap();
            assert_eq!(pixels.len(), 640 * 360 * 4);
            assert_eq!(position, 1. + count as f64 / 30.);
            count += 1;
        }
        decoder.join().unwrap().unwrap();
        assert_eq!(count, 60, "交叉渐变解码预取不依赖音频设备或呈现时钟");
    });
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(2);
        let producer = scope.spawn(|| {
            let cancel = AtomicBool::new(false);
            let result = audio::load(&project, 0., 2., None, &sender, &cancel);
            drop(sender);
            result
        });
        let packets: Vec<_> = receiver.into_iter().collect();
        producer.join().unwrap().unwrap();
        assert_eq!(packets.len(), 20);
        assert!(
            packets.iter().all(|packet| packet.len() == 9600),
            "实时音频按 100 毫秒送入播放队列"
        );
    });
}
