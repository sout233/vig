use crate::{
    editor::Editor,
    model::{Generator, TrackKind},
    theme,
};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext, Bounds, InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, TestAppContext, Window, WindowBounds, WindowOptions, point, px,
    size,
};

fn control_gesture(window: &mut Window, position: Point<Pixels>, end: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
    if position != end {
        window.dispatch_event(
            MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    }
    window.dispatch_event(
        MouseUpEvent {
            position: end,
            button: MouseButton::Left,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
fn control_selection_marquee_batch_drag_copy_delete_and_undo(cx: &mut TestAppContext) {
    let (handle, editor, first, second, third) = cx.update(|cx| {
        gpui_kit::init(cx);
        theme::apply(cx);
        crate::editor::bind_keys(cx);
        let (handle, editor) = gpui_kit::open_window(
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
        .unwrap();
        let (first, second, third) = editor.update(cx, |editor, cx| {
            editor.snap = false;
            let first = editor
                .project
                .add_generator(
                    Generator::Solid {
                        color: "FFFFFF".into(),
                    },
                    1.,
                    Some(1),
                )
                .unwrap();
            let second = editor
                .project
                .add_generator(
                    Generator::Solid {
                        color: "FFFFFF".into(),
                    },
                    5.,
                    Some(1),
                )
                .unwrap();
            let track = editor.project.add_track(TrackKind::Video);
            let third = editor
                .project
                .add_generator(
                    Generator::Solid {
                        color: "FFFFFF".into(),
                    },
                    2.,
                    Some(track),
                )
                .unwrap();
            cx.notify();
            (first, second, third)
        });
        (handle, editor, first, second, third)
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("clip", first), cx);
        let center = window.find(("clip", second)).bounds().center();
        window.hover(("clip", second), cx);
        control_gesture(window, center, center, cx);
        assert_eq!(editor.read(cx).selection.len(), 2);
        assert_eq!(editor.read(cx).project.next_id, 4, "Ctrl 单击不能复制片段");
        window.hover(("clip", first), cx);
        let center = window.find(("clip", first)).bounds().center();
        control_gesture(window, center, center, cx);
        assert_eq!(
            editor
                .read(cx)
                .selection
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![second]
        );

        let first_bounds = window.find(("clip", first)).bounds();
        let second_bounds = window.find(("clip", second)).bounds();
        let from = point(
            first_bounds.left() - px(10.),
            first_bounds.bottom() + px(4.),
        );
        let to = point(
            second_bounds.right() + px(10.),
            second_bounds.top() - px(4.),
        );
        window.drag(to, from, cx);
        assert_eq!(
            editor
                .read(cx)
                .selection
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        assert!(!editor.read(cx).selection.contains(&third));
        assert!(editor.read(cx).marquee.is_none());
        let drag = window.find(("clip", second)).bounds().center();
        window.drag(drag, drag + point(px(60.), px(0.)), cx);
        assert_eq!(editor.read(cx).project.clip(first).unwrap().start, 2.);
        assert_eq!(editor.read(cx).project.clip(second).unwrap().start, 6.);
        assert_eq!(editor.read(cx).project.clip(third).unwrap().start, 2.);
        window.press("ctrl-c", cx);
        editor.update(cx, |editor, _| editor.position = 12.);
        window.press("ctrl-v", cx);
        assert_eq!(editor.read(cx).selection.len(), 2);
        assert_eq!(
            editor
                .read(cx)
                .project
                .tracks
                .iter()
                .map(|track| track.clips.len())
                .sum::<usize>(),
            5
        );
        window.press("delete", cx);
        assert_eq!(
            editor
                .read(cx)
                .project
                .tracks
                .iter()
                .map(|track| track.clips.len())
                .sum::<usize>(),
            3
        );
        window.click("undo", cx);
        assert_eq!(
            editor
                .read(cx)
                .project
                .tracks
                .iter()
                .map(|track| track.clips.len())
                .sum::<usize>(),
            5
        );
        window.click(("clip", first), cx);
        window.hover(("clip", second), cx);
        let center = window.find(("clip", second)).bounds().center();
        control_gesture(window, center, center, cx);
        control_gesture(window, center, center + point(px(60.), px(0.)), cx);
        assert_eq!(
            editor
                .read(cx)
                .project
                .tracks
                .iter()
                .map(|track| track.clips.len())
                .sum::<usize>(),
            7
        );
        assert_eq!(editor.read(cx).selection.len(), 2);
        assert_eq!(editor.read(cx).project.clip(first).unwrap().start, 2.);
        assert_eq!(editor.read(cx).project.clip(second).unwrap().start, 6.);
        let bounds = window.find(("clip", first)).bounds();
        control_gesture(
            window,
            point(bounds.left() - px(10.), bounds.top() - px(4.)),
            point(bounds.left() + px(10.), bounds.bottom() - px(1.)),
            cx,
        );
        assert_eq!(
            editor.read(cx).selection.len(),
            3,
            "Ctrl 框选应保留原有选择"
        );
        window.click_at(("clip", first), point(px(15.), px(40.)), cx);
        assert_eq!(
            editor
                .read(cx)
                .selection
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![first],
            "普通单击应回到单选"
        );
        window.click_at("track-2", point(px(30.), px(20.)), cx);
        assert!(editor.read(cx).selection.is_empty(), "单击空白处应清空选择");
    })
    .unwrap();
}
