#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Viewport {
    pub zoom: f32,
    pub track_zoom: f32,
    pub x: f32,
    pub y: f32,
}

pub(crate) struct Motion {
    current: Viewport,
    target: Viewport,
}

impl Motion {
    pub fn new(current: Viewport, target: Viewport) -> Self {
        Self { current, target }
    }

    pub fn current(&self) -> Viewport {
        self.current
    }

    pub fn target(&self) -> Viewport {
        self.target
    }

    pub fn retarget(&mut self, target: Viewport) {
        self.target = target;
    }

    pub fn advance(&mut self, seconds: f32) -> bool {
        // Exponential easing has the same response at different frame rates.
        let blend = 1. - (-seconds / 0.055).exp();
        self.current.zoom += (self.target.zoom - self.current.zoom) * blend;
        self.current.track_zoom += (self.target.track_zoom - self.current.track_zoom) * blend;
        self.current.x += (self.target.x - self.current.x) * blend;
        self.current.y += (self.target.y - self.current.y) * blend;
        let settled = (self.current.zoom - self.target.zoom).abs() < 0.005
            && (self.current.track_zoom - self.target.track_zoom).abs() < 0.0001
            && (self.current.x - self.target.x).abs() < 0.05
            && (self.current.y - self.target.y).abs() < 0.05;
        if settled {
            self.current = self.target;
        }
        !settled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> Viewport {
        Viewport {
            zoom: 60.,
            track_zoom: 1.,
            x: 0.,
            y: 0.,
        }
    }

    #[test]
    fn frame_rate_independent_and_retargeting_preserves_current_position() {
        let target = Viewport {
            zoom: 120.,
            track_zoom: 2.,
            x: -240.,
            y: -180.,
        };
        let mut thirty = Motion::new(origin(), target);
        let mut sixty = Motion::new(origin(), target);
        for _ in 0..5 {
            thirty.advance(1. / 30.);
        }
        for _ in 0..10 {
            sixty.advance(1. / 60.);
        }
        assert!((thirty.current().x - sixty.current().x).abs() < 0.001);
        for rate in [120, 144, 240] {
            let mut high_refresh = Motion::new(origin(), target);
            for _ in 0..rate / 6 {
                high_refresh.advance(1. / rate as f32);
            }
            let shown = high_refresh.current();
            let expected = sixty.current();
            assert!(
                (shown.x - expected.x).abs() < 0.001,
                "{rate} Hz horizontal motion"
            );
            assert!(
                (shown.y - expected.y).abs() < 0.001,
                "{rate} Hz vertical motion"
            );
            assert!(
                (shown.zoom - expected.zoom).abs() < 0.001,
                "{rate} Hz horizontal zoom"
            );
            assert!(
                (shown.track_zoom - expected.track_zoom).abs() < 0.0001,
                "{rate} Hz vertical zoom"
            );
        }
        let current = sixty.current();
        sixty.retarget(origin());
        assert_eq!(sixty.current(), current);
        sixty.advance(1. / 60.);
        assert!(sixty.current().x > current.x && sixty.current().x < 0.);
    }

    #[test]
    fn zoom_anchor_remains_fixed_and_animation_settles_exactly() {
        let mut current = origin();
        current.x = -100.;
        let cursor = 200.;
        let time = (cursor - current.x) / current.zoom;
        let mut target = current;
        target.zoom = 150.;
        target.x = cursor - time * target.zoom;
        let mut motion = Motion::new(current, target);
        for _ in 0..120 {
            let running = motion.advance(1. / 60.);
            let view = motion.current();
            assert!((time * view.zoom + view.x - cursor).abs() < 0.001);
            if !running {
                break;
            }
        }
        assert_eq!(motion.current(), target);
        assert!(!motion.advance(1. / 60.));
    }
}
