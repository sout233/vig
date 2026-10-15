use super::*;
use std::collections::BTreeSet;

pub struct Marquee {
    pub origin: Point<Pixels>,
    pub current: Point<Pixels>,
    initial: BTreeSet<u64>,
}

impl Marquee {
    pub fn bounds(&self) -> Bounds<Pixels> {
        let left = self.origin.x.min(self.current.x);
        let top = self.origin.y.min(self.current.y);
        Bounds::new(
            point(left, top),
            size(
                (self.current.x - self.origin.x).abs(),
                (self.current.y - self.origin.y).abs(),
            ),
        )
    }
}

impl Editor {
    pub fn selection_members(&self) -> Vec<u64> {
        let mut ids = self.selection.clone();
        if let Some(primary) = self.selected {
            ids.extend(self.project.linked_ids(primary));
        }
        let ids: Vec<_> = ids.into_iter().collect();
        self.project
            .selection_members(&ids)
            .expect("编辑器选中片段不存在")
            .into_iter()
            .collect()
    }

    pub fn is_clip_selected(&self, id: u64) -> bool {
        self.selection.contains(&id) || self.selected == Some(id)
    }

    pub(super) fn reconcile_selection(&mut self) {
        self.selection.retain(|id| self.project.clip(*id).is_some());
        self.selected = self.selected.filter(|id| self.project.clip(*id).is_some());
        if let Some(id) = self.selected {
            if !self.selection.contains(&id) {
                self.selection.clear();
            }
            self.selection.extend(self.project.linked_ids(id));
        } else {
            self.selected = self.selection.first().copied();
        }
    }

    pub fn click_clip(
        &mut self,
        id: u64,
        control: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<u64> {
        self.close_color_editor(cx);
        self.number_edit = None;
        self.timeline_motion = None;
        let group = self.project.linked_ids(id);
        let mut drag_members = self.selection_members();
        self.selection = drag_members.iter().copied().collect();
        if control {
            if group.iter().all(|member| self.is_clip_selected(*member)) {
                for member in &group {
                    self.selection.remove(member);
                }
                self.selected = self.selection.first().copied();
            } else {
                self.selection.extend(group.iter().copied());
                self.selected = Some(id);
                drag_members = self.selection_members();
            }
        } else {
            if !self.is_clip_selected(id) {
                self.selection = group.iter().copied().collect();
            }
            self.selected = Some(id);
            drag_members = self.selection_members();
        }
        self.selected_track = self
            .selected
            .and_then(|id| self.project.clip_track(id))
            .map(|track| track.id);
        self.refresh_fields(window, cx);
        self.focus.focus(window, cx);
        cx.notify();
        drag_members
    }

    pub fn begin_marquee(
        &mut self,
        position: Point<Pixels>,
        additive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_color_editor(cx);
        self.timeline_motion = None;
        self.timeline_drag = None;
        self.number_edit = None;
        self.focus.focus(window, cx);
        let origin =
            position - self.timeline_scroll.bounds().origin - self.timeline_scroll.offset();
        let initial = if additive {
            self.selection_members().into_iter().collect()
        } else {
            BTreeSet::new()
        };
        self.selection = initial.clone();
        self.selected = self.selection.first().copied();
        self.marquee = Some(Marquee {
            origin,
            current: origin,
            initial,
        });
        cx.notify();
    }

    pub fn move_marquee(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(marquee) = &mut self.marquee else {
            return;
        };
        let current =
            position - self.timeline_scroll.bounds().origin - self.timeline_scroll.offset();
        if current == marquee.current {
            return;
        }
        marquee.current = current;
        let bounds = marquee.bounds();
        let mut selected = marquee.initial.clone();
        let mut y = px(30.);
        for track in &self.project.tracks {
            let height = px(self.track_height(track));
            if y + height > bounds.top() && y < bounds.bottom() {
                for clip in &track.clips {
                    let clip_bounds = Bounds::new(
                        point(px(clip.start as f32 * self.zoom), y + px(8.)),
                        size(
                            px((clip.duration() as f32 * self.zoom).max(2.)),
                            height - px(16.),
                        ),
                    );
                    if bounds.intersects(&clip_bounds) {
                        selected.insert(clip.id);
                    }
                }
            }
            y += height;
        }
        let ids: Vec<_> = selected.into_iter().collect();
        let selected = self
            .project
            .selection_members(&ids)
            .expect("框选片段不存在");
        if selected != self.selection {
            self.selection = selected;
            self.selected = self.selection.first().copied();
            self.selected_track = self
                .selected
                .and_then(|id| self.project.clip_track(id))
                .map(|track| track.id);
        }
        cx.notify();
    }
}
