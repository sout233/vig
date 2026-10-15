use super::*;
use crate::editor::ClipDrag;
use crate::model::TrackKind;
use crate::model::TrimEdge;
use gpui_kit::component::menu::ContextMenuExt;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

impl Editor {
    pub(super) fn timeline(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.job == Some(Job::Export);
        let width = (self.project.duration() as f32 * self.zoom + 200.)
            .max(1000.)
            .max(self.timeline_viewport_width);
        let zoom = self.zoom;
        let content_height = 30.
            + self
                .project
                .tracks
                .iter()
                .map(|track| self.track_height(track))
                .sum::<f32>();
        let wheel_owner = cx.entity().downgrade();
        let layout_owner = cx.entity().downgrade();
        let animating = self.timeline_motion.is_some();
        let label_width = self.track_label_width;
        let step = if zoom >= 50. {
            1.
        } else if zoom >= 20. {
            5.
        } else {
            10.
        };
        div()
            .id("timeline-panel")
            .test_support()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .panel_material()
            .child(self.timeline_toolbar(cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        canvas(
                            move |bounds, window, cx| {
                                if animating {
                                    let owner = layout_owner.clone();
                                    // Advance before the next frame builds its layout, avoiding a frame of lag.
                                    window.on_next_frame(move |_, cx| {
                                        owner
                                            .update(cx, |this, cx| {
                                                this.advance_timeline_motion(cx);
                                            })
                                            .expect("时间线所属窗口不存在");
                                    });
                                }
                                let width = (bounds.size.width / px(1.) - label_width).max(0.);
                                let owner = layout_owner.clone();
                                window.defer(cx, move |_, cx| {
                                    owner
                                        .update(cx, |this, cx| {
                                            if this.timeline_viewport_width != width {
                                                this.timeline_viewport_width = width;
                                                cx.notify();
                                            }
                                        })
                                        .expect("时间线所属窗口不存在");
                                });
                                window.insert_hitbox(bounds, HitboxBehavior::Normal)
                            },
                            move |_, hitbox, window, _| {
                                let right_owner = wheel_owner.clone();
                                window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                                    if phase == DispatchPhase::Capture && event.button == MouseButton::Right {
                                        right_owner
                                            .update(cx, |this, _| this.timeline_context = None)
                                            .expect("时间线所属窗口不存在");
                                    }
                                });
                                let owner = wheel_owner.clone();
                                let hitbox = hitbox.clone();
                                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Capture && hitbox.should_handle_scroll(window) {
                                        owner
                                            .update(cx, |this, cx| this.timeline_wheel(event, cx))
                                            .expect("时间线所属窗口不存在");
                                        cx.stop_propagation();
                                    }
                                });
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        div()
                            .id("track-labels")
                            .test_support()
                            .w(px(label_width))
                            .flex_shrink_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.track_labels_scroll)
                            .border_r_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .h(px(content_height))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .h(px(30.))
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .pl_3()
                                            .child(
                                                Button::new("minimize-track-heights")
                                                    .icon(IconName::FoldVertical)
                                                    .compact()
                                                    .ghost()
                                                    .h(px(24.))
                                                    .w(px(28.))
                                                    .accessibility_label("将所有轨道缩到最小")
                                                    .tooltip("将所有轨道缩到最小")
                                                    .disabled(self.project.tracks.is_empty())
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.set_all_track_heights(true, cx)
                                                    })),
                                            )
                                            .child(
                                                Button::new("reset-track-heights")
                                                    .icon(IconName::RotateCcw)
                                                    .compact()
                                                    .ghost()
                                                    .h(px(24.))
                                                    .w(px(28.))
                                                    .accessibility_label("恢复所有轨道默认高度")
                                                    .tooltip("恢复所有轨道默认高度")
                                                    .disabled(self.project.tracks.is_empty())
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.set_all_track_heights(false, cx)
                                                    })),
                                            ),
                                    )
                                    .children(
                                        self.project
                                            .tracks
                                            .iter()
                                            .enumerate()
                                            .map(|(index, track)| self.track_header(index, track, cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("timeline-scroll")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .overflow_scroll()
                            .track_scroll(&self.timeline_scroll)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                    if this.busy || this.job == Some(Job::Export) {
                                        return;
                                    }
                                    this.begin_marquee(event.position, event.modifiers.control, window, cx);
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Middle,
                                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                    this.timeline_motion = None;
                                    this.focus.focus(window, cx);
                                    this.timeline_drag = Some(TimelineDrag::Pan {
                                        pointer: event.position.x,
                                        offset: this.timeline_scroll.offset(),
                                    });
                                }),
                            )
                            .child(
                                div()
                                    .w(px(width))
                                    .h(px(content_height))
                                    .flex_shrink_0()
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .id("ruler")
                                            .test_support()
                                            .flex_shrink_0()
                                            .h(px(30.))
                                            .w_full()
                                            .relative()
                                            .cursor_pointer()
                                            .border_b_1()
                                            .border_color(rgb(BORDER))
                                            .children((0..=(width / zoom / step) as usize).map(|i| {
                                                div()
                                                    .absolute()
                                                    .left(px(i as f32 * step * zoom))
                                                    .top(px(4.))
                                                    .pl_1()
                                                    .border_l_1()
                                                    .border_color(rgb(BORDER))
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(format!(
                                                        "{:02}:{:02}",
                                                        (i as f32 * step) as u32 / 60,
                                                        (i as f32 * step) as u32 % 60
                                                    ))
                                            }))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    if locked {
                                                        return;
                                                    }
                                                    this.begin_scrub(event.position, window, cx);
                                                }),
                                            ),
                                    )
                                    .children(
                                        self.project
                                            .tracks
                                            .iter()
                                            .map(|track| self.track(track, width, cx)),
                                    )
                                    .child(
                                        div()
                                            .id("playhead")
                                            .test_support()
                                            .absolute()
                                            .left(px(self.position as f32 * zoom - 6.))
                                            .top_0()
                                            .bottom_0()
                                            .w(px(12.))
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px(5.))
                                                    .top_0()
                                                    .bottom_0()
                                                    .w(px(1.))
                                                    .bg(rgb(ACCENT)),
                                            )
                                            .child(div().w(px(12.)).h(px(10.)).rounded_b_sm().bg(rgb(ACCENT)))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    if !locked {
                                                        this.begin_scrub(event.position, window, cx);
                                                    }
                                                }),
                                            ),
                                    )
                                    .when_some(self.marquee.as_ref(), |element, marquee| {
                                        let bounds = marquee.bounds();
                                        element.child(
                                            div()
                                                .id("selection-marquee")
                                                .test_support()
                                                .absolute()
                                                .left(bounds.left())
                                                .top(bounds.top())
                                                .w(bounds.size.width)
                                                .h(bounds.size.height)
                                                .border_1()
                                                .border_color(cx.theme().primary.opacity(0.6))
                                                .bg(cx.theme().selection.opacity(0.35)),
                                        )
                                    }),
                            ),
                    )
                    .child(self.track_drag_guide(cx))
                    .child(
                        div()
                            .id("track-label-divider")
                            .test_support()
                            .absolute()
                            .left(px(label_width - 3.))
                            .top_0()
                            .bottom_0()
                            .w(px(6.))
                            .cursor(CursorStyle::ResizeLeftRight)
                            .hover(|d| d.bg(rgba(0xffffff22)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.timeline_motion = None;
                                    this.timeline_drag = None;
                                    this.track_label_resize = Some((event.position.x, this.track_label_width));
                                    this.focus.focus(window, cx);
                                    cx.notify();
                                }),
                            ),
                    ),
            )
    }

    fn timeline_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .justify_between()
            .items_center()
            .px_5()
            .py_3()
            .panel_material()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .mr_3()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("时间线"),
                    )
                    .children(self.timeline_creation_buttons(cx))
                    .children(self.timeline_edit_buttons(cx)),
            )
            .child(self.timeline_navigation_buttons(cx))
            .into_any_element()
    }

    fn track_header(
        &self,
        index: usize,
        track: &crate::model::Track,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let locked = self.busy || self.job == Some(Job::Export);
        let id = track.id;
        let video = track.kind == TrackKind::Video;
        let number = self.project.tracks[..=index]
            .iter()
            .filter(|t| t.kind == track.kind)
            .count();
        div()
            .id(("track-header", id))
            .test_support()
            .relative()
            .flex_shrink_0()
            .h(px(self.track_height(track)))
            .p_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(if self.selected_track == Some(id) {
                VIDEO
            } else {
                SURFACE
            }))
            .when(self.track_drag == Some(id), |d| d.opacity(0.55))
            .child(
                div()
                    .id(("track-handle", id))
                    .test_support()
                    .cursor_pointer()
                    .text_color(rgb(if video { ACCENT } else { AUDIO_TEXT }))
                    .child(format!(
                        "{}{}  {}",
                        if video { "V" } else { "A" },
                        number,
                        if video { "视频" } else { "音频" }
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if !locked {
                                this.selected_track = Some(id);
                                this.timeline_motion = None;
                                this.focus.focus(window, cx);
                                this.track_drag = Some(id);
                                this.track_drop_slot = None;
                                this.move_timeline_pointer(event.position, cx);
                                cx.notify();
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new(("track-up", id))
                            .icon(IconName::ChevronUp)
                            .tooltip("上移轨道")
                            .ghost()
                            .disabled(locked || index == 0)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.reorder_track(id, index - 1, w, cx)
                            })),
                    )
                    .child(
                        Button::new(("track-down", id))
                            .icon(IconName::ChevronDown)
                            .tooltip("下移轨道")
                            .ghost()
                            .disabled(locked || index + 1 == self.project.tracks.len())
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.reorder_track(id, index + 1, w, cx)
                            })),
                    )
                    .child(
                        Button::new(("track-delete", id))
                            .icon(IconName::Trash)
                            .tooltip("删除轨道及其中片段（可撤销）")
                            .ghost()
                            .disabled(locked)
                            .on_click(
                                cx.listener(move |this, _, w, cx| this.delete_track(id, w, cx)),
                            ),
                    ),
            )
            .child(self.track_resize_handle(track, "header-resize", cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    this.selected_track = Some(id);
                    cx.notify();
                }),
            )
            .context_menu(self.track_menu(id, cx))
    }

    fn track(
        &self,
        track: &crate::model::Track,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let video = track.kind == TrackKind::Video;
        let clips = track.ordered_clips();
        let track_id = track.id;
        let zoom = self.zoom;
        let locked = self.busy || self.job == Some(Job::Export);
        let height = self.track_height(track);
        div()
            .id(format!("track-{track_id}"))
            .test_support()
            .relative()
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(BORDER))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(rgb(VIDEO)))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                cx.stop_propagation();
                let time = f64::from(
                    (window.mouse_position().x
                        - this.timeline_scroll.bounds().left()
                        - this.timeline_scroll.offset().x)
                        / px(this.zoom),
                )
                .max(0.);
                this.import_media(
                    Some(paths.paths().to_vec()),
                    true,
                    Some((track_id, time)),
                    window,
                    cx,
                );
            }))
            .drag_over::<MediaDrag>(|style, _, _, _| style.bg(rgb(VIDEO)))
            .on_drop(cx.listener(move |this, drag: &MediaDrag, window, cx| {
                cx.stop_propagation();
                if locked {
                    return;
                }
                let kind = if this.project.media[drag.index].width > 0 {
                    TrackKind::Video
                } else {
                    TrackKind::Audio
                };
                let target = this
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .unwrap();
                if target.kind != kind {
                    this.error = Some("视频只能放入视频轨，音频只能放入音频轨".into());
                    cx.notify();
                    return;
                }
                let bounds = this.timeline_scroll.bounds();
                let time = f64::from(
                    (window.mouse_position().x - bounds.left() - this.timeline_scroll.offset().x)
                        / px(this.zoom),
                )
                .max(0.);
                let mut next = this.project.clone();
                match next.insert_media(drag.index, Some(track_id), Some(time)) {
                    Ok(id) => {
                        let match_video = next.first_video_match(&this.project);
                        this.checkpoint();
                        this.project = next;
                        this.selected = Some(id);
                        this.selected_track = Some(track_id);
                        this.changed(window, cx);
                        this.offer_first_video_settings(match_video, window, cx);
                    }
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.selected_track = Some(track_id);
                    cx.notify();
                }),
            )
            .children(clips.iter().map(|c| {
                let id = c.id;
                let start = c.start;
                let selected = self.is_clip_selected(id);
                div()
                    .id(("clip", id))
                    .test_support()
                    .absolute()
                    .left(px(c.start as f32 * zoom))
                    .top(px(8.))
                    .h(px(height - 16.))
                    .w(px((c.duration() as f32 * zoom).max(2.)))
                    .rounded_md()
                    .border_2()
                    .border_color(rgb(if selected {
                        ACCENT
                    } else if video {
                        VIDEO_BORDER
                    } else {
                        AUDIO_BORDER
                    }))
                    .bg(rgb(if video { VIDEO } else { AUDIO }))
                    .clip_material(video)
                    .when_some(
                        self.project.media[c.media].generator.as_ref(),
                        |d, generator| {
                            if let crate::model::Generator::Solid { color } = generator {
                                let color =
                                    u32::from_str_radix(color, 16).expect("已验证的素材颜色无效");
                                d.child(div().h(px(height * 0.4)).w_full().bg(rgb(color)))
                            } else {
                                d
                            }
                        },
                    )
                    .overflow_hidden()
                    .cursor_pointer()
                    .when(!video, |d| d.child(self.audio_waveform(c)))
                    .when(
                        video && self.project.media[c.media].generator.is_none(),
                        |d| {
                            d.child(
                                div()
                                    .flex()
                                    .w_full()
                                    .h(px(height * 0.4))
                                    .overflow_hidden()
                                    .children(c.preview_times().into_iter().map(|time| {
                                        let key = (self.project.media[c.media].path.clone(), time);
                                        div()
                                            .id(format!("preview-{id}-{time}"))
                                            .test_support()
                                            .w(relative(1. / 3.))
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .overflow_hidden()
                                            .bg(rgb(MONITOR))
                                            .when_some(
                                                self.filmstrip.get(&key).cloned(),
                                                |d, image| {
                                                    d.child(
                                                        div()
                                                            .id(format!(
                                                                "preview-frame-{id}-{time}"
                                                            ))
                                                            .test_support()
                                                            .w(px(height * 0.4 * 16. / 9.))
                                                            .max_w_full()
                                                            .h_full()
                                                            .flex_shrink_0()
                                                            .child(
                                                                img(image)
                                                                    .size_full()
                                                                    .object_fit(ObjectFit::Contain),
                                                            ),
                                                    )
                                                },
                                            )
                                    })),
                            )
                        },
                    )
                    .child(
                        div()
                            .px_2()
                            .pt_1()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.project.media[c.media].display_name()),
                    )
                    .child(self.fade_controls(c, locked, cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                            if locked {
                                return;
                            }
                            cx.stop_propagation();
                            let members = this.click_clip(id, event.modifiers.control, w, cx);
                            let copy = if event.modifiers.control {
                                match this.project.copy_selection(&members, id) {
                                    Ok(payload) => Some(payload),
                                    Err(error) => {
                                        this.error = Some(error);
                                        cx.notify();
                                        return;
                                    }
                                }
                            } else {
                                None
                            };
                            this.drag = Some(ClipDrag {
                                id,
                                pointer: event.position.x / px(1.),
                                start,
                                original: this.project.clone(),
                                invalid: None,
                                edge: None,
                                fade_edge: None,
                                copy,
                                copied_id: None,
                                members,
                            });
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, w, cx| {
                            this.close_color_editor(cx);
                            this.number_edit = None;
                            if this.timeline_context.is_none() {
                                this.timeline_context = Some(TimelineContext::Clip(id));
                            }
                            if !this.is_clip_selected(id) {
                                this.select(id, w, cx);
                            }
                            this.selected = Some(id);
                            this.selected_track = Some(track_id);
                            this.refresh_fields(w, cx);
                            cx.notify();
                        }),
                    )
            }))
            .children(clips.iter().flat_map(|clip| {
                [TrimEdge::In, TrimEdge::Out]
                    .map(|edge| self.trim_control(clip, edge, height, locked, cx))
            }))
            .child(self.track_resize_handle(track, "lane-resize", cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    this.selected_track = Some(track_id);
                    cx.notify();
                }),
            )
            .context_menu(self.track_menu(track_id, cx))
    }
}
mod clip_menu;
mod fade_menu;
mod fades;
mod interaction;
#[cfg(all(test, feature = "ui-verify"))]
mod selection_tests;
mod toolbar;
mod track_menu;
mod trimming;
mod waveform;
