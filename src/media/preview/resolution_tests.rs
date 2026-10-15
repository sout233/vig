use super::*;

#[test]
fn preview_resolution_controls_snapshots_cache_and_raw_frame_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::default();
    project
        .add_generator(
            crate::model::Generator::Solid {
                color: "FFFFFF".into(),
            },
            0.,
            Some(1),
        )
        .unwrap();
    let mut cached_paths = Vec::new();
    for resolution in [
        PreviewResolution::Full,
        PreviewResolution::Half,
        PreviewResolution::Quarter,
    ] {
        let dimensions = resolution.dimensions(project.settings());
        let bytes = subtitles::snapshot(&project, 0.5, dimensions).unwrap();
        let snapshot = image::load_from_memory(&bytes).unwrap();
        assert_eq!((snapshot.width(), snapshot.height()), dimensions);

        let cache = PreviewCache::new(
            dir.path().join("cache"),
            resolution,
            project.settings().clone(),
        );
        let path = cache
            .segment(&project, 0, Arc::new(AtomicBool::new(false)))
            .unwrap();
        let metadata = probe(&path).unwrap();
        assert_eq!((metadata.width, metadata.height), dimensions);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(cache.ready(&project, 0).unwrap(), Some(path.clone()));
        assert_eq!(
            cache
                .segment(&project, 0, Arc::new(AtomicBool::new(false)))
                .unwrap(),
            path
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        assert!(
            !cached_paths.contains(&path),
            "不同分辨率不能共用一个缓存文件"
        );
        cached_paths.push(path);

        let mut command = filter_command(
            &project,
            0.,
            1. / 30.,
            dimensions,
            Some(crate::model::TrackKind::Video),
        )
        .unwrap();
        command.args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
        let mut segment = Segment {
            command,
            dimensions,
            frame_rate: project.settings().frame_rate(),
            start: 0.,
            end: 1. / 30.,
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        decode_video(&mut segment, &sender, &AtomicBool::new(false)).unwrap();
        let (pixels, time) = receiver.recv().unwrap().unwrap();
        assert_eq!(
            pixels.len(),
            dimensions.0 as usize * dimensions.1 as usize * 4
        );
        assert_eq!(time, 0.);
    }
    // Changing quality and returning must retain the earlier completed segment.
    let restored = PreviewCache::new(
        dir.path().join("cache"),
        PreviewResolution::Full,
        project.settings().clone(),
    );
    assert_eq!(
        restored.ready(&project, 0).unwrap(),
        Some(cached_paths[0].clone())
    );
}

#[test]
fn custom_project_resolution_and_fractional_fps_control_export_cache_and_decode() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::default();
    project
        .add_generator(
            crate::model::Generator::Solid {
                color: "FF0000".into(),
            },
            0.,
            Some(1),
        )
        .unwrap();
    let rate: crate::model::FrameRate = "30000/1001".parse().unwrap();
    project
        .set_settings(crate::model::VideoSettings::new(360, 640, rate).unwrap())
        .unwrap();
    let output = dir.path().join("portrait.mp4");
    let (tx, _) = std::sync::mpsc::channel();
    render(
        &project,
        &output,
        false,
        Arc::new(AtomicBool::new(false)),
        &tx,
    )
    .unwrap();
    let metadata = probe(&output).unwrap();
    assert_eq!((metadata.width, metadata.height), (360, 640));
    assert_eq!(
        metadata.frame_rate,
        Some(rate),
        "导出必须保留非整数帧率的精确分数"
    );
    let cache = PreviewCache::new(
        dir.path().join("cache"),
        PreviewResolution::Half,
        project.settings().clone(),
    );
    let first = cache
        .segment(&project, 0, Arc::new(AtomicBool::new(false)))
        .unwrap();
    let metadata = probe(&first).unwrap();
    assert_eq!((metadata.width, metadata.height), (180, 320));
    assert_eq!(metadata.frame_rate, Some(rate));
    project
        .set_settings(crate::model::VideoSettings::new(360, 640, "60".parse().unwrap()).unwrap())
        .unwrap();
    let faster = PreviewCache::new(
        dir.path().join("cache"),
        PreviewResolution::Half,
        project.settings().clone(),
    );
    assert!(
        faster.ready(&project, 0).unwrap().is_none(),
        "只改帧率也必须改变预览缓存键"
    );
    let dimensions = faster.dimensions();
    let mut command = filter_command(
        &project,
        0.,
        0.1,
        dimensions,
        Some(crate::model::TrackKind::Video),
    )
    .unwrap();
    command.args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
    let mut segment = Segment {
        command,
        dimensions,
        frame_rate: project.settings().frame_rate(),
        start: 0.,
        end: 0.1,
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel(6);
    decode_video(&mut segment, &sender, &AtomicBool::new(false)).unwrap();
    drop(sender);
    let frames: Vec<_> = receiver.into_iter().map(Result::unwrap).collect();
    assert_eq!(frames.len(), 6, "60 FPS 的 100 ms 应输出六帧");
    for (index, (pixels, time)) in frames.iter().enumerate() {
        assert_eq!(pixels.len(), 180 * 320 * 4);
        assert!((time - index as f64 / 60.).abs() < 1e-9);
    }
}
