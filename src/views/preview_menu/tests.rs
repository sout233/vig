use crate::{editor::Editor, media::preview::PreviewResolution, model::VideoSettings};
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};
use std::time::Duration;

#[gpui_kit::test]
async fn preview_menu_exposes_current_properties_copy_and_project_settings(
    cx: &mut TestAppContext,
) {
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
        editor.update(cx, |editor, cx| {
            editor
                .project
                .set_settings(VideoSettings::new(1280, 720, "30000/1001".parse().unwrap()).unwrap())
                .unwrap();
            editor.set_preview_resolution(PreviewResolution::Half, cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("preview-resolution").is_some());
        window.click("preview-menu", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("preview-properties").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        for id in [
            "preview-project-size",
            "preview-render-size",
            "preview-frame-rate",
            "preview-aspect-ratio",
            "preview-duration",
        ] {
            assert!(window.find(id).bounds().size.width > px(0.));
        }
        window.click("close-preview-properties", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("preview-menu", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").click(1usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| text.contains("工程分辨率：1280 × 720"))
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        let copied = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copied.contains("预览分辨率：640 × 360 (1/2)"));
        assert!(copied.contains("帧率：29.970 FPS (30000/1001)"));
        assert!(copied.contains("画面比例：16:9"));
        window.render_frame(cx);
        window.click("preview-menu", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").click(2usize, cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("project-width").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        assert_eq!(editor.read(cx).project.settings().dimensions(), (1280, 720));
    })
    .unwrap();
}
