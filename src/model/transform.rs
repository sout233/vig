use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            scale: 1.,
            rotation: 0.,
            opacity: 1.,
        }
    }
}

impl Transform {
    pub fn values(&self) -> [f64; 5] {
        [
            self.x,
            self.y,
            self.scale * 100.,
            self.rotation,
            self.opacity * 100.,
        ]
    }

    pub fn from_values(values: [f64; 5]) -> Result<Self, String> {
        let transform = Self {
            x: values[0],
            y: values[1],
            scale: values[2] / 100.,
            rotation: values[3],
            opacity: values[4] / 100.,
        };
        transform.validate()?;
        Ok(transform)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.values().iter().any(|value| !value.is_finite())
            || self.scale <= 0.
            || !(0. ..=1.).contains(&self.opacity)
        {
            return Err("变换参数无效：缩放必须大于零，不透明度应在 0–100% 之间".into());
        }
        Ok(())
    }
}
