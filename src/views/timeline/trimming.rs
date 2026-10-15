use super::*;

impl Editor {
    pub(super) fn trim_control(
        &self,
        clip: &crate::model::Clip,
        edge: TrimEdge,
        height: f32,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = clip.id;
        let start = clip.start;
        let width = (clip.duration() as f32 * self.zoom).max(2.);
        let handle_width = (width / 2.).min(8.);
        let left = start as f32 * self.zoom
            + if edge == TrimEdge::In {
                0.
            } else {
                width - handle_width
            };
        // Paint above all events so overlapping events cannot hide an earlier tail.
        // Leave the top corners to the manual fade controls.
        div()
            .id((
                if edge == TrimEdge::In {
                    "trim-in"
                } else {
                    "trim-out"
                },
                id,
            ))
            .test_support()
            .absolute()
            .left(px(left))
            .top(px(18.))
            .h(px(height - 26.))
            .w(px(handle_width))
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|d| d.bg(rgba(0xffffff44)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if locked {
                        return;
                    }
                    if event.modifiers.control {
                        this.click_clip(id, true, window, cx);
                        return;
                    }
                    this.select(id, window, cx);
                    this.drag = Some(ClipDrag {
                        id,
                        pointer: event.position.x / px(1.),
                        start,
                        original: this.project.clone(),
                        invalid: None,
                        edge: Some(edge),
                        fade_edge: None,
                        copy: None,
                        copied_id: None,
                        members: this.project.linked_ids(id),
                    });
                }),
            )
            .into_any_element()
    }
}

#[cfg(all(test, feature = "ui-verify"))]
mod tests {
    use crate::{editor::Editor, model::Generator, theme};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };

    #[gpui_kit::test]
    fn both_crossfade_boundaries_are_draggable(cx: &mut TestAppContext) {
        let (handle, editor, outgoing, incoming) = cx.update(|cx| {
            gpui_kit::init(cx);
            theme::apply(cx);
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
            let (outgoing, incoming) = editor.update(cx, |this, cx| {
                this.snap = false;
                let outgoing = this
                    .project
                    .add_generator(
                        Generator::Solid {
                            color: "FFFFFF".into(),
                        },
                        0.,
                        Some(1),
                    )
                    .unwrap();
                let incoming = this
                    .project
                    .add_generator(
                        Generator::Solid {
                            color: "000000".into(),
                        },
                        2.,
                        Some(1),
                    )
                    .unwrap();
                cx.notify();
                (outgoing, incoming)
            });
            (handle, editor, outgoing, incoming)
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let tail = window.find(("trim-out", outgoing)).bounds().center();
            window.drag(tail, tail - point(px(30.), px(0.)), cx);
            let project = &editor.read(cx).project;
            assert!(
                (project.clip(outgoing).unwrap().end() - 2.5).abs() < 0.001,
                "重叠区域右边界必须裁剪前一片段的尾部，而不是移动上层片段"
            );
            assert_eq!(project.clip(incoming).unwrap().start, 2.);
            assert!((project.clip(incoming).unwrap().fade_in - 0.5).abs() < 0.001);
            window.render_frame(cx);
            let head = window.find(("trim-in", incoming)).bounds().center();
            window.drag(head, head + point(px(15.), px(0.)), cx);
            let project = &editor.read(cx).project;
            assert!(
                (project.clip(incoming).unwrap().start - 2.25).abs() < 0.001,
                "重叠区域左边界必须裁剪后一片段的头部"
            );
            assert!((project.clip(outgoing).unwrap().end() - 2.5).abs() < 0.001);
            assert!((project.clip(incoming).unwrap().fade_in - 0.25).abs() < 0.001);
            assert!(editor.read(cx).drag.is_none());
        })
        .unwrap();
    }
}
