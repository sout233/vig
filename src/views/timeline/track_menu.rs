use super::*;
use gpui_kit::component::{
    ActiveTheme,
    menu::{PopupMenu, PopupMenuItem},
};

#[derive(Clone, Copy)]
enum TrackCommand {
    AddVideo,
    AddAudio,
    MoveUp,
    MoveDown,
    Clear,
    Delete,
    ResetHeight,
}

#[cfg(all(test, feature = "ui-verify"))]
mod tests {
    use crate::{editor::Editor, model::Generator, theme};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };

    #[gpui_kit::test]
    fn right_click_clip_opens_only_the_clip_menu(cx: &mut TestAppContext) {
        let (handle, editor, id) = cx.update(|cx| {
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
            let id = editor.update(cx, |this, cx| {
                let id = this
                    .project
                    .add_generator(
                        Generator::Solid {
                            color: "FFFFFF".into(),
                        },
                        0.,
                        Some(1),
                    )
                    .unwrap();
                cx.notify();
                id
            });
            (handle, editor, id)
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.right_click(("clip", id), cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("popup-menu").visible(), "右键应只打开一个菜单");
            window.press("escape", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("popup-menu").is_none());
            window.right_click(("track-handle", 1u64), cx);
            window.right_click(("clip", id), cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("popup-menu").visible(), "右键应只打开一个菜单");
            window.press("escape", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.right_click(("clip", id), cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("popup-menu").visible(), "右键应只打开一个菜单");
            window.within("popup-menu").click(0usize, cx); // Unlink is disabled for a generated event.
        })
        .unwrap();
        cx.update(|cx| assert_eq!(editor.read(cx).selected, Some(id)));
    }
}

impl Editor {
    pub(super) fn track_drag_guide(&self, cx: &mut Context<Self>) -> AnyElement {
        let destination = self
            .track_drag
            .zip(self.track_drop_slot)
            .and_then(|(id, slot)| {
                let source = self
                    .project
                    .tracks
                    .iter()
                    .position(|track| track.id == id)
                    .unwrap();
                if slot == source || slot == source + 1 {
                    return None;
                }
                Some(
                    30. + self.timeline_scroll.offset().y / px(1.)
                        + self.project.tracks[..slot]
                            .iter()
                            .map(|track| self.track_height(track))
                            .sum::<f32>(),
                )
            });
        let color = cx.theme().primary;
        canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                if let Some(y) = destination {
                    if y < 0. || y > bounds.size.height / px(1.) {
                        return;
                    }
                    let mut path = PathBuilder::stroke(px(3.));
                    let top = bounds.top() + px(y);
                    path.move_to(point(bounds.left(), top));
                    path.line_to(point(bounds.right(), top));
                    path.move_to(point(bounds.left() + px(4.), top - px(5.)));
                    path.line_to(point(bounds.left() + px(4.), top + px(5.)));
                    window.paint_path(path.build().expect("无法绘制轨道插入位置"), color);
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    pub(super) fn track_resize_handle(
        &self,
        track: &crate::model::Track,
        part: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = track.id;
        let height = self.track_height(track);
        let locked = self.busy || self.job == Some(Job::Export);
        let highlight = cx.theme().primary.opacity(0.13);
        div()
            .id((part, id))
            .test_support()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(6.))
            .when(!locked, |d| {
                d.cursor(CursorStyle::ResizeUpDown)
                    .hover(|d| d.bg(highlight))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if locked {
                        return;
                    }
                    this.timeline_motion = None;
                    this.selected_track = Some(id);
                    this.track_resize = Some(TrackResize {
                        id,
                        pointer: event.position.y,
                        height,
                    });
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn track_menu(
        &self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + use<> {
        let owner = cx.entity().downgrade();
        let index = self
            .project
            .tracks
            .iter()
            .position(|track| track.id == id)
            .unwrap();
        let locked = self.busy || self.job == Some(Job::Export);
        let empty = self.project.tracks[index].clips.is_empty();
        let items = [
            ("在下方添加视频轨", TrackCommand::AddVideo, locked),
            ("在下方添加音频轨", TrackCommand::AddAudio, locked),
            ("上移轨道", TrackCommand::MoveUp, locked || index == 0),
            (
                "下移轨道",
                TrackCommand::MoveDown,
                locked || index + 1 == self.project.tracks.len(),
            ),
            ("恢复默认高度", TrackCommand::ResetHeight, locked),
            ("清空轨道片段", TrackCommand::Clear, locked || empty),
            ("删除轨道", TrackCommand::Delete, locked),
        ];
        move |mut menu, window, cx| {
            let current = cx.entity().downgrade();
            let previous = owner
                .update(cx, |this, _| this.timeline_menu.replace(current))
                .expect("轨道菜单所属编辑器不存在");
            if let Some(previous) = previous.and_then(|menu| menu.upgrade()) {
                // ContextMenu observes dismissal; replacing a menu must close its old surface.
                previous.update(cx, |_, cx| cx.emit(DismissEvent));
            }
            let route = owner
                .update(cx, |this, cx| {
                    type Builder =
                        Box<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;
                    match this.timeline_context.clone() {
                        Some(TimelineContext::Clip(id)) => {
                            Some(Box::new(this.clip_menu(id, cx)) as Builder)
                        }
                        Some(TimelineContext::Fades(targets)) => {
                            Some(Box::new(this.fade_menu(targets, locked, cx)) as Builder)
                        }
                        None => None,
                    }
                })
                .expect("轨道菜单所属编辑器不存在");
            if let Some(route) = route {
                return route(menu, window, cx);
            }
            for (label, command, disabled) in items {
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new(label).disabled(disabled).on_click(
                    move |_, window, cx| {
                        let owner = owner.clone();
                        window.defer(cx, move |window, cx| {
                            owner
                                .update(cx, |this, cx| {
                                    this.selected_track = Some(id);
                                    match command {
                                        TrackCommand::AddVideo => {
                                            this.add_track(TrackKind::Video, window, cx)
                                        }
                                        TrackCommand::AddAudio => {
                                            this.add_track(TrackKind::Audio, window, cx)
                                        }
                                        TrackCommand::MoveUp => {
                                            this.reorder_track(id, index - 1, window, cx)
                                        }
                                        TrackCommand::MoveDown => {
                                            this.reorder_track(id, index + 1, window, cx)
                                        }
                                        TrackCommand::Clear => this.clear_track(id, window, cx),
                                        TrackCommand::Delete => this.delete_track(id, window, cx),
                                        TrackCommand::ResetHeight => {
                                            this.track_heights.remove(&id);
                                            cx.notify();
                                        }
                                    }
                                })
                                .expect("轨道菜单所属编辑器不存在");
                        });
                    },
                ));
            }
            menu
        }
    }
}
