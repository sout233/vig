use super::*;

impl Editor {
    pub(in crate::views) fn finish_pointer_drag(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Middle {
            self.timeline_drag = None;
            return;
        }
        if event.button != MouseButton::Left {
            return;
        }
        if self.marquee.is_some() {
            self.move_marquee(event.position, cx);
            self.marquee = None;
            self.refresh_fields(window, cx);
            cx.notify();
            return;
        }
        if self.track_label_resize.is_some() {
            self.resize_track_labels(event.position.x, cx);
            self.track_label_resize = None;
            return;
        }
        if self.track_resize.is_some() {
            self.resize_track(event.position.y, cx);
            self.track_resize = None;
            return;
        }
        if self.number_drag.is_some() {
            self.move_number(event.modifiers.shift, window, cx);
            self.finish_number(window, cx);
            return;
        }
        self.move_timeline_pointer(event.position, cx);
        self.timeline_drag = None;
        self.finish_drag(window, cx);
        if let Some(id) = self.track_drag.take() {
            if let Some(slot) = self.track_drop_slot.take() {
                let source = self
                    .project
                    .tracks
                    .iter()
                    .position(|track| track.id == id)
                    .unwrap();
                self.reorder_track(id, slot - usize::from(slot > source), window, cx);
            }
            cx.notify();
        }
    }

    pub(in crate::views) fn finish_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if let Some(error) = drag.invalid {
            self.project = drag.original;
            self.selected = Some(drag.id);
            self.selection = drag.members.into_iter().collect();
            self.selected_track = self.project.clip_track(drag.id).map(|track| track.id);
            self.refresh_fields(window, cx);
            self.error = Some(error);
            cx.notify();
            return;
        }
        if drag.copy.is_some() && drag.copied_id.is_none() {
            return;
        }
        if drag.copy.is_none()
            && self.project.clip_track(drag.id).unwrap().id
                == drag.original.clip_track(drag.id).unwrap().id
            && (self.project.clip(drag.id).unwrap().start - drag.start).abs() < 0.001
            && (self.project.clip(drag.id).unwrap().source_in
                - drag.original.clip(drag.id).unwrap().source_in)
                .abs()
                < 0.001
            && (self.project.clip(drag.id).unwrap().source_out
                - drag.original.clip(drag.id).unwrap().source_out)
                .abs()
                < 0.001
            && (self.project.clip(drag.id).unwrap().fade_in
                - drag.original.clip(drag.id).unwrap().fade_in)
                .abs()
                < 0.001
            && (self.project.clip(drag.id).unwrap().fade_out
                - drag.original.clip(drag.id).unwrap().fade_out)
                .abs()
                < 0.001
        {
            if drag.edge.is_none() && drag.fade_edge.is_none() {
                self.select(drag.id, window, cx);
            }
            return;
        }
        let next = std::mem::replace(&mut self.project, drag.original);
        self.checkpoint();
        self.project = next;
        self.selected = Some(drag.copied_id.unwrap_or(drag.id));
        self.selected_track = self
            .project
            .clip_track(self.selected.unwrap())
            .map(|t| t.id);
        self.changed(window, cx);
    }

    pub(in crate::views) fn track_at(&self, y: Pixels) -> Option<usize> {
        let bounds = self.timeline_scroll.bounds();
        if y < bounds.top() || y >= bounds.bottom() {
            return None;
        }
        let mut top = bounds.top() + self.timeline_scroll.offset().y + px(30.);
        for (index, track) in self.project.tracks.iter().enumerate() {
            let bottom = top + px(self.track_height(track));
            if y >= top && y < bottom {
                return Some(index);
            }
            top = bottom;
        }
        None
    }

    pub(super) fn begin_scrub(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marquee = None;
        self.timeline_motion = None;
        self.focus.focus(window, cx);
        self.timeline_drag = Some(TimelineDrag::Scrub {
            origin: self.timeline_scroll.bounds().left() + self.timeline_scroll.offset().x,
        });
        self.move_timeline_pointer(position, cx);
    }

    pub(in crate::views) fn move_timeline_pointer(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.marquee.is_some() {
            self.move_marquee(position, cx);
            return;
        }
        if self.track_drag.is_some() {
            let bounds = self.timeline_scroll.bounds();
            let slot = if position.y >= bounds.top() && position.y < bounds.bottom() {
                let heights: Vec<_> = self
                    .project
                    .tracks
                    .iter()
                    .map(|track| self.track_height(track))
                    .collect();
                let pointer =
                    (position.y - bounds.top() - self.timeline_scroll.offset().y) / px(1.) - 30.;
                Some(crate::track_order::insertion_slot(&heights, pointer))
            } else {
                None
            };
            if self.track_drop_slot != slot {
                self.track_drop_slot = slot;
                cx.notify();
            }
            return;
        }
        if let Some(mut drag) = self.drag.take() {
            if let Some(edge) = drag.fade_edge {
                let original = drag.original.clip(drag.id).unwrap();
                let delta = f64::from((position.x / px(1.) - drag.pointer) / self.zoom);
                let mut next = drag.original.clone();
                let clip = next.clip_mut(drag.id).unwrap();
                if edge == TrimEdge::In {
                    clip.fade_in =
                        (original.fade_in + delta).clamp(0., clip.duration() - clip.fade_out);
                } else {
                    clip.fade_out =
                        (original.fade_out - delta).clamp(0., clip.duration() - clip.fade_in);
                }
                self.project = next;
                self.drag = Some(drag);
                self.seek(self.position, cx);
                return;
            }
            if let Some(edge) = drag.edge {
                let original = drag.original.clip(drag.id).unwrap();
                let mut time = (if edge == TrimEdge::In {
                    original.start
                } else {
                    original.end()
                }) + f64::from((position.x / px(1.) - drag.pointer) / self.zoom);
                if self.snap {
                    let track = drag.original.clip_track(drag.id).unwrap();
                    if let Some(value) = track
                        .clips
                        .iter()
                        .filter(|c| c.id != drag.id)
                        .flat_map(|c| [c.start, c.end()])
                        .chain([0., self.position])
                        .filter(|p| (p - time).abs() < 8. / f64::from(self.zoom))
                        .min_by(|a, b| (a - time).abs().total_cmp(&(b - time).abs()))
                    {
                        time = value;
                    }
                }
                let mut next = drag.original.clone();
                drag.invalid = next.drag_trim_edge(drag.id, edge, time).err();
                if drag.invalid.is_none() {
                    self.project = next;
                }
                self.drag = Some(drag);
                cx.notify();
                return;
            }
            if let Some(index) = self.track_at(position.y) {
                if (position.x / px(1.) - drag.pointer).abs() < 3.
                    && drag.copy.is_some()
                    && drag.copied_id.is_none()
                    && drag.original.tracks[index].id
                        == drag.original.clip_track(drag.id).unwrap().id
                {
                    self.drag = Some(drag);
                    return;
                }
                let track = &drag.original.tracks[index];
                let earliest = drag
                    .original
                    .tracks
                    .iter()
                    .flat_map(|track| &track.clips)
                    .filter(|clip| drag.members.binary_search(&clip.id).is_ok())
                    .map(|clip| clip.start)
                    .fold(f64::INFINITY, f64::min);
                let mut start = (drag.start
                    + f64::from((position.x / px(1.) - drag.pointer) / self.zoom))
                .max(drag.start - earliest);
                if self.snap {
                    let duration = drag.original.clip(drag.id).unwrap().duration();
                    if let Some(value) = track
                        .clips
                        .iter()
                        .filter(|c| !drag.members.contains(&c.id))
                        .flat_map(|c| [c.start, c.end(), c.start - duration, c.end() - duration])
                        .chain([0., self.position])
                        .filter(|p| *p >= 0. && (p - start).abs() < 8. / f64::from(self.zoom))
                        .min_by(|a, b| (a - start).abs().total_cmp(&(b - start).abs()))
                    {
                        start = value;
                    }
                }
                let mut next = drag.original.clone();
                if drag.copy.is_some()
                    && drag.copied_id.is_none()
                    && start == drag.start
                    && track.id == drag.original.clip_track(drag.id).unwrap().id
                {
                    self.drag = Some(drag);
                    return;
                }
                start = start.max(drag.start - earliest);
                if let Some(payload) = &drag.copy {
                    let result = next.paste_clips(payload, Some(track.id), start);
                    match result {
                        Ok(id) => {
                            drag.copied_id = Some(id);
                            drag.invalid = None;
                            self.selected = Some(id);
                            self.selection = next
                                .tracks
                                .iter()
                                .flat_map(|track| &track.clips)
                                .filter(|clip| clip.id >= drag.original.next_id)
                                .map(|clip| clip.id)
                                .collect();
                        }
                        Err(error) => drag.invalid = Some(error),
                    }
                } else {
                    drag.invalid = next
                        .move_selection(&drag.members, drag.id, track.id, start)
                        .err();
                    if drag.invalid.is_none() {
                        self.selection = drag.members.iter().copied().collect();
                        self.selected = Some(drag.id);
                    }
                }
                if drag.invalid.is_none() {
                    self.project = next;
                }
            } else {
                drag.invalid = Some("请将片段放到同类型轨道中".into());
            }
            self.drag = Some(drag);
            cx.notify();
            return;
        }
        match self.timeline_drag {
            Some(TimelineDrag::Scrub { origin }) => {
                let fps = self.project.settings().frame_rate().fps();
                let time = (f64::from((position.x - origin) / px(self.zoom)) * fps).round() / fps;
                if (time - self.position).abs() > f64::EPSILON {
                    self.seek(time, cx);
                }
            }
            Some(TimelineDrag::Pan { pointer, offset }) => {
                self.smooth_pan(point(offset.x + position.x - pointer, offset.y), cx);
            }
            None => {}
        }
    }
}
