use super::*;
use crate::model::FadeCurve;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};

impl Editor {
    pub(super) fn fade_menu(
        &self,
        targets: Vec<(u64, TrimEdge)>,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + use<> {
        let owner = cx.entity().downgrade();
        let choices: Vec<_> = targets
            .into_iter()
            .map(|(id, edge)| {
                let clip = self.project.clip(id).unwrap();
                let curve = if edge == TrimEdge::In {
                    clip.fade_in_curve
                } else {
                    clip.fade_out_curve
                };
                (
                    id,
                    edge,
                    curve,
                    self.project.media[clip.media].display_name(),
                )
            })
            .collect();
        move |mut menu, _, _| {
            for (index, (id, edge, current, name)) in choices.iter().enumerate() {
                if index > 0 {
                    menu = menu.separator();
                }
                let title = if *edge == TrimEdge::In {
                    "淡入曲线"
                } else {
                    "淡出曲线"
                };
                menu = menu.label(if choices.len() > 2 {
                    format!("{title} · {name}")
                } else {
                    title.into()
                });
                for (curve, label) in FadeCurve::ALL {
                    let owner = owner.clone();
                    let (id, edge) = (*id, *edge);
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(curve == *current)
                            .disabled(locked)
                            .on_click(move |_, window, cx| {
                                let owner = owner.clone();
                                window.defer(cx, move |window, cx| {
                                    owner
                                        .update(cx, |this, cx| {
                                            let clip = this.project.clip(id).unwrap();
                                            let current = if edge == TrimEdge::In {
                                                clip.fade_in_curve
                                            } else {
                                                clip.fade_out_curve
                                            };
                                            if current == curve {
                                                return;
                                            }
                                            this.close_color_editor(cx);
                                            this.checkpoint();
                                            let clip = this.project.clip_mut(id).unwrap();
                                            if edge == TrimEdge::In {
                                                clip.fade_in_curve = curve;
                                            } else {
                                                clip.fade_out_curve = curve;
                                            }
                                            this.selected = Some(id);
                                            this.changed(window, cx);
                                        })
                                        .expect("淡入淡出菜单所属编辑器不存在");
                                });
                            }),
                    );
                }
            }
            menu
        }
    }
}

#[cfg(all(test, feature = "ui-verify"))]
mod tests {
    use crate::{
        editor::Editor,
        model::{FadeCurve, Generator},
        theme,
    };
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };

    #[gpui_kit::test]
    fn fade_region_menu_changes_each_curve_independently(cx: &mut TestAppContext) {
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
                let clip = this.project.clip_mut(id).unwrap();
                clip.fade_in = 1.;
                clip.fade_out = 1.;
                cx.notify();
                id
            });
            (handle, editor, id)
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.right_click(format!("fade-region-{id}-{id}-0"), cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.click(2usize, cx); // Heading, smooth, linear.
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            let clip = editor.read(cx).project.clip(id).unwrap();
            assert_eq!(clip.fade_in_curve, FadeCurve::Linear);
            assert_eq!(clip.fade_out_curve, FadeCurve::Smooth);
            window.render_frame(cx);
            window.right_click(format!("fade-region-{id}-{id}-1"), cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.click(4usize, cx); // EaseOut.
        })
        .unwrap();
        cx.update(|cx| {
            let clip = editor.read(cx).project.clip(id).unwrap();
            assert_eq!(clip.fade_in_curve, FadeCurve::Linear);
            assert_eq!(clip.fade_out_curve, FadeCurve::EaseOut);
            assert_eq!(editor.read(cx).undo.len(), 2);
        });
    }
}
