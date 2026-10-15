use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum Generator {
    Text(TextGenerator),
    Solid { color: String },
}

impl Generator {
    pub fn as_text(&self) -> Option<&TextGenerator> {
        match self {
            Self::Text(text) => Some(text),
            Self::Solid { .. } => None,
        }
    }

    pub fn as_text_mut(&mut self) -> Option<&mut TextGenerator> {
        match self {
            Self::Text(text) => Some(text),
            Self::Solid { .. } => None,
        }
    }

    pub fn color(&self) -> &str {
        match self {
            Self::Text(text) => &text.color,
            Self::Solid { color } => color,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Text(text) => text.validate(),
            Self::Solid { color } => validate_color(color),
        }
    }
}

pub(super) fn validate_color(color: &str) -> Result<(), String> {
    if color.len() != 6 || !color.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err("颜色应为六位十六进制，如 FFFFFF".into());
    }
    Ok(())
}

impl Project {
    pub fn add_generator(
        &mut self,
        generator: Generator,
        start: f64,
        preferred: Option<u64>,
    ) -> Result<u64, String> {
        generator.validate()?;
        let mut next = self.clone();
        let media = next.media.len();
        let name = match &generator {
            Generator::Text(_) => "文本",
            Generator::Solid { .. } => "纯色",
        };
        let (width, height) = next.settings().dimensions();
        next.media.push(Media {
            path: PathBuf::new(),
            name: name.into(),
            duration: 3.,
            width,
            height,
            audio: false,
            frame_rate: None,
            generator: Some(generator),
        });
        let track = next
            .tracks
            .iter()
            .find(|track| Some(track.id) == preferred && track.kind == TrackKind::Video)
            .or_else(|| {
                next.tracks
                    .iter()
                    .find(|track| track.kind == TrackKind::Video)
            })
            .map(|track| track.id)
            .unwrap_or_else(|| next.add_track(TrackKind::Video));
        let id = next.insert_media(media, Some(track), Some(start))?;
        next.validate()?;
        *self = next;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_sources_use_regular_video_editing_and_roundtrip() {
        let mut project = Project::default();
        let track = project.add_track(TrackKind::Video);
        let id = project
            .add_generator(
                Generator::Solid {
                    color: "276BC4".into(),
                },
                1.,
                Some(track),
            )
            .unwrap();
        assert_eq!(project.clip_track(id).unwrap().id, track);
        assert!(!project.media[project.clip(id).unwrap().media].audio);
        project.trim_clip(id, TrimEdge::Out, 20.).unwrap();
        project.clip_mut(id).unwrap().transform.opacity = 0.5;
        let payload = project.copy_clips(id).unwrap();
        let pasted = project.paste_clips(&payload, Some(1), 30.).unwrap();
        let loaded = Project::from_json(&serde_json::to_string(&project).unwrap()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.version, 9);
        assert_eq!(
            loaded.media[loaded.clip(pasted).unwrap().media]
                .generator
                .as_ref()
                .unwrap()
                .color(),
            "276BC4"
        );
        assert_eq!(loaded.clip(pasted).unwrap().transform.opacity, 0.5);
    }

    #[test]
    fn version_seven_text_migrates_to_an_explicit_text_generator() {
        let mut project = Project::default();
        project.add_subtitle(0.).unwrap();
        let mut old = serde_json::to_value(project).unwrap();
        old["version"] = 7.into();
        let text = old["media"][0]["generator"]["data"].take();
        old["media"][0]["generator"] = text;
        let loaded = Project::from_json(&old.to_string()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.version, 9);
        assert!(matches!(
            &loaded.media[0].generator,
            Some(Generator::Text(_))
        ));
        old["media"][0]["generator"]
            .as_object_mut()
            .unwrap()
            .remove("font_file");
        assert!(
            Project::from_json(&old.to_string()).is_err(),
            "损坏的旧文本数据不能被解释为纯色"
        );
    }

    #[test]
    fn invalid_solid_colors_do_not_change_the_project() {
        let mut project = Project::default();
        let before = serde_json::to_string(&project).unwrap();
        assert!(
            project
                .add_generator(
                    Generator::Solid {
                        color: "red".into()
                    },
                    0.,
                    None
                )
                .is_err()
        );
        assert_eq!(serde_json::to_string(&project).unwrap(), before);
        assert!(
            serde_json::from_str::<Generator>(r#"{"kind":"Other","data":{"color":"FFFFFF"}}"#)
                .is_err()
        );
    }
}
