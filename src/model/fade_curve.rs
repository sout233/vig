use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FadeCurve {
    Linear,
    #[default]
    Smooth,
    EaseIn,
    EaseOut,
    Sine,
}

impl FadeCurve {
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::Smooth, "平滑 S 曲线"),
        (Self::Linear, "线性（直线）"),
        (Self::EaseIn, "缓入（二次曲线）"),
        (Self::EaseOut, "缓出（二次曲线）"),
        (Self::Sine, "正弦曲线"),
    ];

    pub fn value(self, progress: f64) -> f64 {
        let t = progress.clamp(0., 1.);
        match self {
            Self::Linear => t,
            Self::Smooth => t * t * (3. - 2. * t),
            Self::EaseIn => t * t,
            Self::EaseOut => 1. - (1. - t).powi(2),
            Self::Sine => (1. - (std::f64::consts::PI * t).cos()) / 2.,
        }
    }

    pub fn expression(self, progress: &str) -> String {
        let t = format!("clip({progress},0,1)");
        match self {
            Self::Linear => t,
            Self::Smooth => format!("({t})*({t})*(3-2*({t}))"),
            Self::EaseIn => format!("pow({t},2)"),
            Self::EaseOut => format!("1-pow(1-({t}),2)"),
            Self::Sine => format!("(1-cos(PI*({t})))/2"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FadeCurve;

    #[test]
    fn curves_have_exact_endpoints_and_monotonic_progress() {
        for (curve, _) in FadeCurve::ALL {
            assert_eq!(curve.value(0.), 0.);
            assert_eq!(curve.value(1.), 1.);
            let mut last = 0.;
            for i in 0..=100 {
                let value = curve.value(f64::from(i) / 100.);
                assert!(value >= last && value <= 1.);
                last = value;
            }
        }
        assert_eq!(FadeCurve::Smooth.value(0.25), 0.15625);
        assert_eq!(FadeCurve::Linear.value(0.25), 0.25);
        assert_eq!(FadeCurve::EaseIn.value(0.5), 0.25);
        assert_eq!(FadeCurve::EaseOut.value(0.5), 0.75);
    }

    #[test]
    fn independent_curve_choices_survive_save_and_load() {
        use crate::model::{Generator, Project};
        let mut project = Project::default();
        let id = project
            .add_generator(
                Generator::Solid {
                    color: "FFFFFF".into(),
                },
                0.,
                Some(1),
            )
            .unwrap();
        let clip = project.clip_mut(id).unwrap();
        assert_eq!(clip.fade_in_curve, FadeCurve::Smooth);
        clip.fade_in = 1.;
        clip.fade_out = 1.;
        clip.fade_in_curve = FadeCurve::Linear;
        clip.fade_out_curve = FadeCurve::EaseIn;
        let loaded = Project::from_json(&serde_json::to_string(&project).unwrap()).unwrap();
        let clip = loaded.clip(id).unwrap();
        assert_eq!(clip.brightness(0.25), 0.25);
        assert_eq!(clip.brightness(2.25), 0.9375);
        assert_eq!(clip.fade_in_curve, FadeCurve::Linear);
        assert_eq!(clip.fade_out_curve, FadeCurve::EaseIn);
        let mut invalid = serde_json::to_value(&project).unwrap();
        invalid["tracks"][0]["clips"][0]["fade_in_curve"] = "Unknown".into();
        assert!(Project::from_json(&invalid.to_string()).is_err());
    }
}
