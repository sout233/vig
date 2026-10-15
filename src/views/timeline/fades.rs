use super::*;

impl Editor {
    pub(super) fn fade_controls(
        &self,
        c: &crate::model::Clip,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = c.id;
        let start = c.start;
        let track = self.project.clip_track(id).unwrap();
        let (crossfade_in, crossfade_out) = track.crossfade_ranges(c);
        // Show the obscured event's actual outgoing curve inside the upper event.
        let width = c.duration() as f32 * self.zoom;
        let mut regions = Vec::new();
        if c.fade_in > 0. {
            regions.push((
                id,
                TrimEdge::In,
                0.,
                c.fade_in as f32 * self.zoom,
                c.fade_in_curve,
            ));
        }
        if c.fade_out > 0. {
            regions.push((
                id,
                TrimEdge::Out,
                width - c.fade_out as f32 * self.zoom,
                width,
                c.fade_out_curve,
            ));
        }
        regions.extend(
            track
                .ordered_clips()
                .into_iter()
                .filter(|other| {
                    (other.start < c.start || other.start == c.start && other.id < c.id)
                        && other.end() > c.start
                        && other.fade_out > 0.
                        && other.end() - other.fade_out < c.end()
                })
                .map(|other| {
                    (
                        other.id,
                        TrimEdge::Out,
                        (other.end() - other.fade_out - c.start) as f32 * self.zoom,
                        (other.end() - c.start) as f32 * self.zoom,
                        other.fade_out_curve,
                    )
                }),
        );
        let mut spans: Vec<_> = regions
            .iter()
            .map(|(_, _, begin, end, _)| (*begin, *end))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut backgrounds: Vec<(f32, f32)> = Vec::new();
        for (begin, end) in spans {
            if let Some(last) = backgrounds.last_mut().filter(|last| begin <= last.1) {
                last.1 = last.1.max(end);
            } else {
                backgrounds.push((begin, end));
            }
        }
        let height = self.track_height(track) - 16.;
        div()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h(px(10.))
            .children(backgrounds.into_iter().map(|(begin, end)| {
                div()
                    .absolute()
                    .left(px(begin))
                    .top_0()
                    .w(px(end - begin))
                    .h(px(height))
                    .bg(rgba(0xffffff0d))
            }))
            .children(regions.iter().map(|(target, edge, begin, end, _)| {
                let targets: Vec<_> = regions
                    .iter()
                    .filter(|(_, _, other_begin, other_end, _)| {
                        *other_begin < *end && *other_end > *begin
                    })
                    .map(|(id, edge, _, _, _)| (*id, *edge))
                    .collect();
                div()
                    .id(format!(
                        "fade-region-{id}-{target}-{}",
                        usize::from(*edge == TrimEdge::Out)
                    ))
                    .test_support()
                    .absolute()
                    .left(px(*begin))
                    .top_0()
                    .w(px(end - begin))
                    .h(px(height))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, _, _| {
                            this.timeline_context = Some(TimelineContext::Fades(targets.clone()));
                        }),
                    )
            }))
            .child(
                canvas(
                    |bounds, _, _| bounds,
                    move |bounds, _, window, _| {
                        let mut path = PathBuilder::stroke(px(1.));
                        for (_, edge, begin, end, curve) in &regions {
                            for step in 0..=32 {
                                let t = f64::from(step) / 32.;
                                let value = curve.value(t);
                                let gain = if *edge == TrimEdge::In {
                                    value
                                } else {
                                    1. - value
                                };
                                let position = point(
                                    bounds.left() + px(begin + (end - begin) * t as f32),
                                    bounds.bottom() - bounds.size.height * gain as f32,
                                );
                                if step == 0 {
                                    path.move_to(position);
                                } else {
                                    path.line_to(position);
                                }
                            }
                            let boundary = if *edge == TrimEdge::In { *end } else { *begin };
                            path.move_to(point(bounds.left() + px(boundary), bounds.top()));
                            path.line_to(point(bounds.left() + px(boundary), bounds.bottom()));
                            // The obscured preceding event still needs its visible tail boundary.
                            if *edge == TrimEdge::Out {
                                path.move_to(point(bounds.left() + px(*end), bounds.top()));
                                path.line_to(point(bounds.left() + px(*end), bounds.bottom()));
                            }
                        }
                        if !regions.is_empty() {
                            window.paint_path(
                                path.build().expect("无法绘制淡入淡出曲线"),
                                rgb(ACCENT),
                            );
                        }
                    },
                )
                .absolute()
                .w_full()
                .h(px(height)),
            )
            .children(
                [TrimEdge::In, TrimEdge::Out]
                    .into_iter()
                    .filter(|edge| match edge {
                        TrimEdge::In => crossfade_in == 0.,
                        TrimEdge::Out => crossfade_out == 0.,
                    })
                    .map(|edge| {
                        let offset = (if edge == TrimEdge::In {
                            c.fade_in as f32 * self.zoom
                        } else {
                            c.fade_out as f32 * self.zoom
                        } - 5.)
                            .clamp(0., (c.duration() as f32 * self.zoom - 14.).max(0.));
                        div()
                            .id((
                                if edge == TrimEdge::In {
                                    "fade-in"
                                } else {
                                    "fade-out"
                                },
                                id,
                            ))
                            .test_support()
                            .absolute()
                            .top_0()
                            .size(px(10.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(ACCENT))
                            .bg(rgb(SURFACE))
                            .cursor(CursorStyle::ResizeLeftRight)
                            .when(edge == TrimEdge::In, |d| d.left(px(offset)))
                            .when(edge == TrimEdge::Out, |d| d.right(px(offset)))
                            .hover(|d| d.bg(rgb(ACCENT)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                                    cx.stop_propagation();
                                    if locked {
                                        return;
                                    }
                                    if event.modifiers.control {
                                        this.click_clip(id, true, w, cx);
                                        return;
                                    }
                                    this.close_color_editor(cx);
                                    this.select(id, w, cx);
                                    this.selected_track = this.project.clip_track(id).map(|t| t.id);
                                    this.refresh_fields(w, cx);
                                    this.focus.focus(w, cx);
                                    this.stop();
                                    this.drag = Some(ClipDrag {
                                        id,
                                        pointer: event.position.x / px(1.),
                                        start,
                                        original: this.project.clone(),
                                        invalid: None,
                                        edge: None,
                                        fade_edge: Some(edge),
                                        copy: None,
                                        copied_id: None,
                                        members: this.project.linked_ids(id),
                                    });
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |this, _, _, _| {
                                    this.timeline_context =
                                        Some(TimelineContext::Fades(vec![(id, edge)]));
                                }),
                            )
                    }),
            )
            .into_any_element()
    }
}
