use super::{
    Caption, ModelSize,
    captions::{Word, segment},
    insert_captions, source_ranges, validate_model,
};
use crate::model::{Media, Project, TrackKind};

#[test]
fn subtitle_ranges_follow_the_timeline_and_insert_as_one_regular_video_track() {
    let mut project = Project::default();
    let captions = vec![
        Caption {
            start: 8.25,
            end: 10.5,
            text: "你好，世界".into(),
        },
        Caption {
            start: 11.,
            end: 15.,
            text: "第二行字幕".into(),
        },
    ];
    let before = serde_json::to_string(&project).unwrap();
    let (next, track) = insert_captions(&project, &captions).unwrap();
    assert_eq!(serde_json::to_string(&project).unwrap(), before);
    assert_eq!(next.tracks[0].id, track);
    assert_eq!(next.tracks[0].kind, TrackKind::Video);
    assert_eq!(next.tracks[0].clips.len(), 2);
    assert_eq!(next.tracks[0].clips[0].start, 8.25);
    assert_eq!(next.tracks[0].clips[0].duration(), 2.25);
    assert_eq!(
        next.media[0]
            .generator
            .as_ref()
            .unwrap()
            .as_text()
            .unwrap()
            .text,
        "你好，世界"
    );
    let loaded = Project::from_json(&serde_json::to_string(&next).unwrap()).unwrap();
    loaded.validate().unwrap();
    project = next;
    project
        .trim_clip(
            project.tracks[0].clips[0].id,
            crate::model::TrimEdge::Out,
            9.75,
        )
        .unwrap();
    assert!(
        insert_captions(
            &project,
            &[Caption {
                start: 1.,
                end: 0.,
                text: "无效".into()
            }]
        )
        .is_err()
    );
}

#[test]
fn source_ranges_merge_overlaps_keep_gaps_and_ignore_muted_events() {
    let mut project = Project::default();
    project.media.push(Media {
        path: "audio.wav".into(),
        name: "audio".into(),
        duration: 10.,
        width: 0,
        height: 0,
        audio: true,
        frame_rate: None,
        generator: None,
    });
    let first = project.insert_media(0, Some(2), Some(5.)).unwrap();
    project.clip_mut(first).unwrap().speed = 2.;
    project.insert_media(0, Some(2), Some(8.)).unwrap();
    let muted = project.insert_media(0, Some(2), Some(20.)).unwrap();
    project.clip_mut(muted).unwrap().muted = true;
    project.insert_media(0, Some(2), Some(35.)).unwrap();
    assert_eq!(
        source_ranges(&project, 2).unwrap(),
        vec![(5., 18.), (35., 45.)]
    );
    assert!(source_ranges(&project, 1).is_err());
}

#[test]
fn aligned_words_form_readable_captions_with_absolute_timestamps() {
    let words = [
        (0.2, 0.5, "你好"),
        (0.5, 0.8, "世界。"),
        (2., 2.3, "Hello"),
        (2.3, 2.8, "world!"),
    ]
    .into_iter()
    .map(|(start_time, end_time, text)| Word {
        text: text.into(),
        start_time,
        end_time,
    })
    .collect();
    let captions = segment(words, 37.).unwrap();
    assert_eq!(
        captions,
        vec![
            Caption {
                start: 37.2,
                end: 37.8,
                text: "你好世界。".into()
            },
            Caption {
                start: 39.,
                end: 39.8,
                text: "Hello world!".into()
            }
        ]
    );
    assert!(
        segment(
            vec![Word {
                text: "无效".into(),
                start_time: f64::NAN,
                end_time: 1.
            }],
            0.
        )
        .is_err()
    );
}

#[test]
fn importing_models_rejects_missing_shards_and_swapped_model_roles() {
    let directory = tempfile::tempdir().unwrap();
    let write =
        |name, text: &str| std::fs::write(directory.path().join(name), text.as_bytes()).unwrap();
    write(
        "config.json",
        r#"{"thinker_config":{"model_type":"qwen3_asr","audio_config":{"encoder_layers":18}}}"#,
    );
    write("tokenizer_config.json", "{}");
    write("tokenizer.json", "{}");
    write(
        "model.safetensors.index.json",
        r#"{"weight_map":{"x":"model-00001-of-00002.safetensors","y":"model-00002-of-00002.safetensors"}}"#,
    );
    write("model-00001-of-00002.safetensors", "test");
    assert!(
        validate_model(directory.path(), false, ModelSize::Small)
            .unwrap_err()
            .contains("00002")
    );
    write("model-00002-of-00002.safetensors", "test");
    validate_model(directory.path(), false, ModelSize::Small).unwrap();
    assert!(validate_model(directory.path(), true, ModelSize::Small).is_err());
    assert!(validate_model(directory.path(), false, ModelSize::Large).is_err());
}

#[test]
#[ignore = "需要通过 VIG_ASR_MODEL 和 VIG_ASR_ALIGNER 指定真实模型目录"]
fn real_models_generate_chinese_subtitles() {
    let model = std::env::var_os("VIG_ASR_MODEL").expect("请指定真实识别模型");
    let aligner = std::env::var_os("VIG_ASR_ALIGNER").expect("请指定真实对齐模型");
    let audio = std::env::var_os("VIG_ASR_AUDIO").expect("请指定带语音的真实音频文件");
    let mut project = Project::default();
    project
        .media
        .push(crate::media::probe(std::path::Path::new(&audio)).unwrap());
    // Source can be a video; explicitly retain only its audio event.
    let id = project
        .insert_media(
            0,
            Some(if project.media[0].width > 0 { 1 } else { 2 }),
            Some(5.),
        )
        .unwrap();
    let _ = id;
    let captions = super::generate(
        &super::AsrEngine::default(),
        project.clone(),
        2,
        super::Preferences {
            small: Some(model.into()),
            large: None,
            aligner: Some(aligner.into()),
            selected: ModelSize::Small,
            source: Default::default(),
            sensevoice: None,
            vad: None,
        },
        &std::sync::atomic::AtomicBool::new(false),
        |message| eprintln!("{message}"),
    )
    .unwrap();
    assert!(!captions.is_empty());
    assert!(
        captions
            .iter()
            .all(|c| c.start >= 5. && c.end > c.start && !c.text.is_empty())
    );
    insert_captions(&project, &captions)
        .unwrap()
        .0
        .validate()
        .unwrap();
}
