use super::*;

const MIN_TRACK_HEIGHT: f32 = 64.;

impl Editor {
    pub fn resize_track_labels(&mut self, pointer: Pixels, cx: &mut Context<Self>) {
        let Some((origin, width)) = self.track_label_resize else {
            return;
        };
        let total = (self.timeline_scroll.bounds().right()
            - self.track_labels_scroll.bounds().left())
            / px(1.);
        self.track_label_width =
            (width + (pointer - origin) / px(1.)).clamp(120., (total - 160.).max(120.));
        cx.notify();
    }

    pub fn track_height(&self, track: &crate::model::Track) -> f32 {
        self.track_heights
            .get(&track.id)
            .copied()
            .unwrap_or(track.kind.height())
            * self.track_zoom
    }

    pub fn resize_track(&mut self, pointer: Pixels, cx: &mut Context<Self>) {
        let Some(drag) = &self.track_resize else {
            return;
        };
        // Reserve enough space for the track title, commands and clip label.
        let height = (drag.height + (pointer - drag.pointer) / px(1.)).max(MIN_TRACK_HEIGHT);
        self.track_heights.insert(drag.id, height / self.track_zoom);
        cx.notify();
    }

    pub fn set_all_track_heights(&mut self, minimize: bool, cx: &mut Context<Self>) {
        self.timeline_motion = None;
        self.track_resize = None;
        self.track_zoom = 1.;
        self.track_heights.clear();
        if minimize {
            self.track_heights.extend(
                self.project
                    .tracks
                    .iter()
                    .map(|track| (track.id, MIN_TRACK_HEIGHT)),
            );
        }
        let content_height = 30.
            + self
                .project
                .tracks
                .iter()
                .map(|track| self.track_height(track))
                .sum::<f32>();
        let viewport_height = self.timeline_scroll.bounds().size.height / px(1.);
        let offset = self.timeline_scroll.offset();
        let y = (offset.y / px(1.)).clamp(-(content_height - viewport_height).max(0.), 0.);
        self.timeline_scroll.set_offset(point(offset.x, px(y)));
        self.track_labels_scroll.set_offset(point(px(0.), px(y)));
        cx.notify();
    }

    pub fn clear_track(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.close_color_editor(cx);
        self.checkpoint();
        self.project
            .tracks
            .iter_mut()
            .find(|track| track.id == id)
            .unwrap()
            .clips
            .clear();
        self.project.clean_links();
        self.selected = self.selected.filter(|id| self.project.clip(*id).is_some());
        self.changed(window, cx);
    }

    pub fn add_track(&mut self, kind: TrackKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.close_color_editor(cx);
        self.checkpoint();
        let id = self.project.add_track(kind);
        if let Some(index) = self
            .selected_track
            .and_then(|id| self.project.tracks.iter().position(|t| t.id == id))
        {
            self.project.reorder_track(id, index + 1);
        }
        self.selected_track = Some(id);
        self.changed(window, cx);
    }

    pub fn delete_track(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.close_color_editor(cx);
        self.checkpoint();
        self.project.tracks.retain(|t| t.id != id);
        self.project.clean_links();
        self.selected = self.selected.filter(|id| self.project.clip(*id).is_some());
        self.changed(window, cx);
    }

    pub fn reorder_track(
        &mut self,
        id: u64,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        if self.project.tracks.iter().position(|t| t.id == id) == Some(index) {
            return;
        }
        self.close_color_editor(cx);
        self.checkpoint();
        self.project.reorder_track(id, index);
        self.changed(window, cx);
    }
}
