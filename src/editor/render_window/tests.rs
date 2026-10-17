use crate::{editor::Editor, model::Generator};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size};

#[gpui_kit::test]
fn render_window_has_independent_settings_and_validates_input(cx: &mut TestAppContext) {
    let (main, editor) = cx.update(|cx| {
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
    cx.update_window(main, |_, window, cx| {
        editor.update(cx, |editor, cx| {
            editor
                .project
                .add_generator(
                    Generator::Solid {
                        color: "FFFFFF".into(),
                    },
                    0.,
                    None,
                )
                .unwrap();
            editor.export(window, cx);
        });
    })
    .unwrap();
    let render = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != main)
            .unwrap()
    });
    cx.update_window(render, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("render-width").value(), Some("1920"));
        assert_eq!(window.find("render-height").value(), Some("1080"));
        assert_eq!(window.find("render-fps").value(), Some("30"));
        window.click("render-width", cx);
        window.press("ctrl-a", cx);
        for c in "721".chars() {
            window.press(&c.to_string(), cx);
        }
        window.click("start-render", cx);
        window.render_frame(cx);
        assert!(window.try_find("render-error").is_some());
        assert_eq!(
            editor.read(cx).project.settings().dimensions(),
            (1920, 1080)
        );
        assert!(editor.read(cx).job.is_none());
        window.within("render-format").click("input", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find("render-quality").is_some());
    })
    .unwrap();
}
