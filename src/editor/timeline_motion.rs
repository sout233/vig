use super::*;
use crate::motion::{Motion, Viewport};
use std::time::Instant;

pub struct TimelineAnimation {
    motion: Motion,
    last_frame: Instant,
}

impl Editor {
    fn timeline_viewport(&self) -> Viewport {
        let offset = self.timeline_scroll.offset();
        Viewport {
            zoom: self.zoom,
            track_zoom: self.track_zoom,
            x: offset.x / px(1.),
            y: offset.y / px(1.),
        }
    }

    fn timeline_target(&self) -> Viewport {
        self.timeline_motion
            .as_ref()
            .map(|animation| animation.motion.target())
            .unwrap_or_else(|| self.timeline_viewport())
    }

    fn smooth_timeline(&mut self, mut target: Viewport, cx: &mut Context<Self>) {
        let bounds = self.timeline_scroll.bounds();
        let width = (self.project.duration() as f32 * target.zoom + 200.)
            .max(1000.)
            .max(self.timeline_viewport_width);
        let height = 30.
            + self
                .project
                .tracks
                .iter()
                .map(|track| self.track_height(track) / self.track_zoom * target.track_zoom)
                .sum::<f32>();
        target.x = target
            .x
            .clamp(-(width - bounds.size.width / px(1.)).max(0.), 0.);
        target.y = target
            .y
            .clamp(-(height - bounds.size.height / px(1.)).max(0.), 0.);
        if cx.reduce_motion() {
            self.timeline_motion = None;
            self.zoom = target.zoom;
            self.track_zoom = target.track_zoom;
            self.timeline_scroll
                .set_offset(point(px(target.x), px(target.y)));
            self.track_labels_scroll
                .set_offset(point(px(0.), px(target.y)));
        } else if let Some(animation) = &mut self.timeline_motion {
            animation.motion.retarget(target);
        } else {
            self.timeline_motion = Some(TimelineAnimation {
                motion: Motion::new(self.timeline_viewport(), target),
                last_frame: Instant::now(),
            });
        }
        cx.notify();
    }

    pub fn advance_timeline_motion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(animation) = &mut self.timeline_motion else {
            return false;
        };
        let now = Instant::now();
        let running = animation
            .motion
            .advance((now - animation.last_frame).as_secs_f32());
        animation.last_frame = now;
        let view = animation.motion.current();
        self.zoom = view.zoom;
        self.track_zoom = view.track_zoom;
        self.timeline_scroll
            .set_offset(point(px(view.x), px(view.y)));
        self.track_labels_scroll
            .set_offset(point(px(0.), px(view.y)));
        if !running {
            self.timeline_motion = None;
        }
        cx.notify();
        running
    }

    pub fn smooth_pan(&mut self, offset: Point<Pixels>, cx: &mut Context<Self>) {
        let mut target = self.timeline_target();
        target.x = offset.x / px(1.);
        target.y = offset.y / px(1.);
        self.smooth_timeline(target, cx);
    }

    pub fn zoom_timeline(&mut self, factor: f32, cx: &mut Context<Self>) {
        let mut target = self.timeline_target();
        let cursor = self.timeline_scroll.bounds().size.width / px(2.);
        let time = (cursor - target.x) / target.zoom;
        target.zoom = (target.zoom * factor).clamp(5., 400.);
        target.x = cursor - time * target.zoom;
        self.smooth_timeline(target, cx);
    }

    pub fn timeline_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(16.));
        let amount = (if delta.y != px(0.) { delta.y } else { delta.x }) / px(1.);
        let bounds = self.timeline_scroll.bounds();
        let mut target = self.timeline_target();
        if event.modifiers.control {
            let cursor =
                ((event.position.x - bounds.left()) / px(1.)).clamp(0., bounds.size.width / px(1.));
            let time = (cursor - target.x) / target.zoom;
            target.zoom = (target.zoom * (amount / 200.).exp()).clamp(5., 400.);
            target.x = cursor - time * target.zoom;
        } else if event.modifiers.alt {
            let cursor = ((event.position.y - bounds.top()) / px(1.))
                .clamp(30., (bounds.size.height / px(1.)).max(30.));
            let position = (cursor - target.y - 30.) / target.track_zoom;
            target.track_zoom = (target.track_zoom * (amount / 200.).exp()).max(0.75);
            target.y = cursor - 30. - position * target.track_zoom;
        } else if event.modifiers.shift {
            target.x += amount;
        } else {
            target.y += amount;
        }
        self.smooth_timeline(target, cx);
    }
}
