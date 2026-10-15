use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextGenerator {
    pub text: String,
    pub font_file: PathBuf,
    pub font_size: f64,
    pub color: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_text_uses_video_tracks_and_regular_clip_editing() {
        let mut project = Project::default();
        let id = project.add_subtitle(1.).unwrap();
        assert_eq!(project.clip_track(id).unwrap().kind, TrackKind::Video);
        assert!(project.tracks[1].clips.is_empty());
        project.trim_clip(id, TrimEdge::Out, 12.).unwrap();
        let target = project.add_track(TrackKind::Video);
        project.move_clip(id, target, 2.).unwrap();
        project.clip_mut(id).unwrap().fade_in = 1.;
        project.clip_mut(id).unwrap().transform =
            Transform::from_values([10., -20., 80., 35., 60.]).unwrap();
        project.split_linked(id, 5.).unwrap();
        let copy = project.duplicate(id);
        assert_eq!(
            project.clip(copy).unwrap().transform,
            project.clip(id).unwrap().transform
        );
        assert!(project.move_clip(copy, 2, 1.).is_err());
        let loaded = Project::from_json(&serde_json::to_string(&project).unwrap()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.clip(id).unwrap().transform.opacity, 0.6);
        assert!(
            loaded.media[loaded.clip(id).unwrap().media]
                .generator
                .is_some()
        );
    }

    #[test]
    fn old_captions_migrate_to_regular_video_events_with_their_position() {
        let windows = std::env::var_os("WINDIR").unwrap();
        let font = PathBuf::from(windows).join("Fonts/msyh.ttc");
        let old = serde_json::json!({
            "version": 6, "name": "旧项目", "media": [],
            "tracks": [{"id": 1, "kind": "Video", "clips": []}],
            "next_id": 8, "next_track_id": 2,
            "subtitles": [{"id": 7, "text": "字幕", "start": 2., "duration": 5.,
                "font_file": font, "font_size": 48., "color": "FFFFFF", "x": 0.25, "y": 0.8}]
        });
        let project = Project::from_json(&old.to_string()).unwrap();
        project.validate().unwrap();
        let clip = project.clip(7).unwrap();
        assert_eq!(project.version, 9);
        assert_eq!((clip.start, clip.duration()), (2., 5.));
        assert_eq!(clip.transform.x, -25.);
        assert!((clip.transform.y + 10.).abs() < 1e-9);
        assert_eq!(project.clip_track(7).unwrap().id, project.tracks[0].id);
        assert!(
            !serde_json::to_value(project)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("subtitles")
        );
    }
}

impl TextGenerator {
    pub fn validate(&self) -> Result<(), String> {
        if self.text.trim().is_empty() || self.text.contains('\0') {
            return Err("请输入有效的字幕文字".into());
        }
        if !self.font_size.is_finite() || self.font_size <= 0. {
            return Err("字幕字号必须大于零".into());
        }
        super::generators::validate_color(&self.color)?;
        if !self.font_file.is_file() {
            return Err(format!("字幕字体不存在：{}", self.font_file.display()));
        }
        Ok(())
    }
}

impl Project {
    pub fn add_subtitle(&mut self, start: f64) -> Result<u64, String> {
        let windows = std::env::var_os("WINDIR").ok_or("无法确定 Windows 字体目录")?;
        let generator = TextGenerator {
            text: "请输入字幕".into(),
            font_file: PathBuf::from(windows).join("Fonts/msyh.ttc"),
            font_size: 48.,
            color: "FFFFFF".into(),
        };
        self.add_generator(Generator::Text(generator), start, None)
    }

    pub(super) fn migrate_subtitles(&mut self, value: serde_json::Value) -> Result<(), String> {
        #[derive(Deserialize)]
        struct LegacySubtitle {
            id: u64,
            text: String,
            start: f64,
            duration: f64,
            font_file: PathBuf,
            font_size: f64,
            color: String,
            x: f64,
            y: f64,
        }
        let subtitles: Vec<LegacySubtitle> =
            serde_json::from_value(value).map_err(|e| e.to_string())?;
        if subtitles.is_empty() {
            return Ok(());
        }
        // Legacy captions were rendered above every video track; retain that compositing order.
        let track_id = self.add_track(TrackKind::Video);
        self.reorder_track(track_id, 0);
        for old in subtitles {
            if !old.duration.is_finite()
                || old.duration <= 0.
                || !old.x.is_finite()
                || !old.y.is_finite()
            {
                return Err("旧字幕参数无效".into());
            }
            let media = self.media.len();
            self.media.push(Media {
                path: PathBuf::new(),
                name: "文本".into(),
                duration: old.duration,
                width: 1920,
                height: 1080,
                audio: false,
                frame_rate: None,
                generator: Some(Generator::Text(TextGenerator {
                    text: old.text,
                    font_file: old.font_file,
                    font_size: old.font_size,
                    color: old.color,
                })),
            });
            self.tracks[0].clips.push(Clip {
                id: old.id,
                media,
                start: old.start,
                source_in: 0.,
                source_out: old.duration,
                speed: 1.,
                volume: 1.,
                muted: false,
                fade_in: 0.,
                fade_out: 0.,
                group: None,
                fade_in_curve: FadeCurve::default(),
                fade_out_curve: FadeCurve::default(),
                transform: Transform {
                    x: (old.x - 0.5) * 100.,
                    y: (old.y - 0.9) * 100.,
                    ..Transform::default()
                },
            });
        }
        Ok(())
    }
}
