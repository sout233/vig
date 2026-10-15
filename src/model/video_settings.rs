use super::*;
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

impl Default for FrameRate {
    fn default() -> Self {
        Self {
            numerator: 30,
            denominator: 1,
        }
    }
}

impl FrameRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, String> {
        if numerator == 0 || denominator == 0 {
            return Err("帧率必须大于零".into());
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }

    pub fn fps(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }

    pub fn frame_duration(self) -> f64 {
        1. / self.fps()
    }

    pub fn validate(self) -> Result<(), String> {
        Self::new(self.numerator, self.denominator).map(|_| ())
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(f, "{}", self.numerator)
        } else {
            write!(f, "{}/{}", self.numerator, self.denominator)
        }
    }
}

impl FromStr for FrameRate {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (numerator, denominator) = if let Some((n, d)) = text.split_once('/') {
            (
                n.trim().parse().map_err(|_| "帧率分子无效")?,
                d.trim().parse().map_err(|_| "帧率分母无效")?,
            )
        } else if let Some((whole, fraction)) = text.split_once('.') {
            let numerator = format!("{whole}{fraction}")
                .parse()
                .map_err(|_| "帧率无效")?;
            let denominator = 10u32
                .checked_pow(fraction.len().try_into().map_err(|_| "帧率精度超出范围")?)
                .ok_or("帧率精度超出范围")?;
            (numerator, denominator)
        } else {
            (text.parse().map_err(|_| "帧率无效")?, 1)
        };
        Self::new(numerator, denominator)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoSettings {
    width: u32,
    height: u32,
    frame_rate: FrameRate,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::default(),
        }
    }
}

impl VideoSettings {
    pub fn new(width: u32, height: u32, frame_rate: FrameRate) -> Result<Self, String> {
        let settings = Self {
            width,
            height,
            frame_rate,
        };
        settings.validate()?;
        Ok(settings)
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.width < 2 || self.height < 2 || self.width % 2 != 0 || self.height % 2 != 0 {
            return Err("视频宽度与高度必须为不小于 2 的偶数（MP4 使用 YUV 4:2:0）".into());
        }
        self.frame_rate.validate()
    }
}

impl Project {
    pub fn settings(&self) -> &VideoSettings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: VideoSettings) -> Result<(), String> {
        settings.validate()?;
        for media in self
            .media
            .iter_mut()
            .filter(|media| media.generator.is_some())
        {
            (media.width, media.height) = settings.dimensions();
        }
        self.settings = settings;
        Ok(())
    }

    /// Only real video placed on a track counts; importing or adding generators does not.
    pub fn first_video_match(&mut self, previous: &Project) -> Option<PathBuf> {
        if self.first_video_match_offered {
            return None;
        }
        let real_video = |project: &Project| {
            project
                .video()
                .any(|clip| project.media[clip.media].generator.is_none())
        };
        if !real_video(self) {
            return None;
        }
        self.first_video_match_offered = true;
        if real_video(previous) {
            return None;
        }
        self.video()
            .map(|clip| &self.media[clip.media])
            .find(|media| media.generator.is_none())
            .map(|media| media.path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_legacy_defaults_and_invalid_values() {
        let mut project = Project::default();
        let settings = VideoSettings::new(1080, 1920, "30000/1001".parse().unwrap()).unwrap();
        project.set_settings(settings.clone()).unwrap();
        let loaded = Project::from_json(&serde_json::to_string(&project).unwrap()).unwrap();
        assert_eq!(loaded.settings(), &settings);
        assert_eq!(loaded.settings().frame_rate().to_string(), "30000/1001");
        let mut old = serde_json::to_value(&project).unwrap();
        old["version"] = 8.into();
        old.as_object_mut().unwrap().remove("settings");
        assert_eq!(
            Project::from_json(&old.to_string()).unwrap().settings(),
            &VideoSettings::default()
        );
        for rate in ["0", "NaN", "60/0", "-1", "29.97.1", ""] {
            assert!(rate.parse::<FrameRate>().is_err(), "{rate}");
        }
        assert_eq!(
            "29.97".parse::<FrameRate>().unwrap().to_string(),
            "2997/100"
        );
        assert_eq!(
            "60000/2002".parse::<FrameRate>().unwrap().to_string(),
            "30000/1001"
        );
        assert!(VideoSettings::new(0, 1080, FrameRate::default()).is_err());
        assert!(VideoSettings::new(1921, 1080, FrameRate::default()).is_err());
        old["settings"] = serde_json::json!({"width": 1080, "height": 1920, "frame_rate": {"numerator": 30, "denominator": 0}});
        assert!(Project::from_json(&old.to_string()).is_err());
    }

    #[test]
    fn first_video_matching_only_follows_first_real_video_placement() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut project = Project::default();
        project
            .add_generator(
                Generator::Solid {
                    color: "FFFFFF".into(),
                },
                0.,
                Some(1),
            )
            .unwrap();
        let previous = project.clone();
        project.media.push(Media {
            path: file.path().into(),
            name: "竖屏.mp4".into(),
            duration: 1.,
            width: 1080,
            height: 1920,
            audio: true,
            frame_rate: Some("60".parse().unwrap()),
            generator: None,
        });
        assert!(
            project.first_video_match(&previous).is_none(),
            "只导入素材不应询问"
        );
        let previous = project.clone();
        project.insert_media(1, Some(1), Some(0.)).unwrap();
        assert_eq!(
            project.first_video_match(&previous),
            Some(file.path().into())
        );
        let saved = Project::from_json(&serde_json::to_string(&project).unwrap()).unwrap();
        let mut next = saved.clone();
        next.insert_media(1, Some(1), Some(2.)).unwrap();
        assert!(
            next.first_video_match(&saved).is_none(),
            "选择保留工程属性后，重新打开也不能重复询问"
        );
        let mut legacy = previous.clone();
        legacy.insert_media(1, Some(1), Some(0.)).unwrap();
        let previous = legacy.clone();
        legacy.insert_media(1, Some(1), Some(2.)).unwrap();
        assert!(
            legacy.first_video_match(&previous).is_none(),
            "已有视频的旧工程无需询问"
        );
    }
}
