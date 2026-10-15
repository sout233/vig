use crate::editor::Editor;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};

#[gpui_kit::test]
async fn tools_menu_opens_subtitle_form_and_exposes_import_guide(cx: &mut TestAppContext) {
    let (handle, editor) = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        crate::theme::apply(cx);
        crate::editor::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1380.), px(900.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Editor::new(window, cx)),
        )
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tools-menu", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
    cx.executor().run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("auto-subtitles-form").bounds().size.width > px(0.));
        assert!(window.find("asr-track").value().unwrap().contains("A1"));
        assert!(window.find("asr-download-source").value().is_some());
        let selected = window.find("asr-model").value().unwrap().to_owned();
        window.within("asr-model").click("input", cx);
        let steps = if selected.contains("SenseVoice") {
            2
        } else if selected.contains("1.7B") {
            1
        } else {
            0
        };
        for _ in 0..steps {
            window.press("up", cx);
        }
        window.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(1), |window, _| {
        window.find("asr-model").expanded() == Some(false)
            && window.find("asr-model").value().unwrap().contains("0.6B")
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("asr-model")
                .value()
                .unwrap()
                .contains("Qwen3-ASR")
        );
        window.click("copy-asr-download-link", cx);
        let link = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(link.ends_with("/tree/main") && link.contains("/Qwen/Qwen3-ASR-"));
        window.click("copy-aligner-download-link", cx);
        assert!(
            cx.read_from_clipboard()
                .unwrap()
                .text()
                .unwrap()
                .ends_with("/Qwen/Qwen3-ForcedAligner-0.6B/tree/main")
        );
        window.within("asr-download-source").click("input", cx);
        if window
            .find("asr-download-source")
            .value()
            .unwrap()
            .contains("hf-mirror.com")
        {
            window.press("up", cx);
        } else {
            window.press("down", cx);
        }
        window.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(1), |window, _| {
        window.find("asr-download-source").expanded() == Some(false)
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("asr-model").click("input", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(1), |window, _| {
        window
            .find("asr-model")
            .value()
            .is_some_and(|value| value.contains("SenseVoice"))
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("asr-sensevoice-timing").is_some());
        let endpoint = if window
            .find("asr-download-source")
            .value()
            .unwrap()
            .contains("hf-mirror.com")
        {
            "https://hf-mirror.com"
        } else {
            "https://huggingface.co"
        };
        window.click("copy-asr-download-link", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            format!(
                "{endpoint}/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/tree/main"
            )
        );
        window.click("copy-aligner-download-link", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            format!("{endpoint}/istupakov/silero-vad-onnx/tree/main")
        );
        window.click("generate-auto-subtitles", cx);
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(1), |window, _| {
        window.try_find("asr-error").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("asr-error").bounds().size.width > px(0.));
        assert!(
            !editor.read(cx).busy,
            "没有可识别的源或模型时，不得启动后台任务"
        );
        window.click("asr-import-help", cx);
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(1), |window, _| {
        window.try_find("asr-import-guide").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("download-asr-model").bounds().size.width > px(0.));
        assert!(window.find("download-aligner-model").bounds().size.width > px(0.));
        assert!(
            window.find("generate-auto-subtitles").bounds().bottom() < px(900.),
            "教程展开后，生成按钮应留在可见底栏"
        );
        window.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("auto-subtitles-form").is_none());
    })
    .unwrap();
    assert_eq!(cx.update(|cx| editor.read(cx).project.tracks.len()), 2);
}
