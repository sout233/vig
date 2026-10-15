use super::{Clip, Project, Track};

impl Clip {
    pub fn fade_expression(&self, time: &str) -> String {
        let mut value = "1".to_owned();
        if self.fade_in > 0. {
            let curve = self
                .fade_in_curve
                .expression(&format!("({time}-{:.9})/{:.9}", self.start, self.fade_in));
            value = format!("min({value},({curve}))");
        }
        if self.fade_out > 0. {
            let curve = self.fade_out_curve.expression(&format!(
                "({time}-{:.9})/{:.9}",
                self.end() - self.fade_out,
                self.fade_out
            ));
            value = format!("min({value},1-({curve}))");
        }
        format!(
            "gte({time},{:.9})*lt({time},{:.9})*({value})",
            self.start,
            self.end()
        )
    }
}

impl Track {
    pub fn ordered_clips(&self) -> Vec<&Clip> {
        let mut clips: Vec<_> = self.clips.iter().collect();
        clips.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.id.cmp(&b.id)));
        clips
    }

    /// Placement determines the default durations of the ordinary fade handles.
    pub fn crossfade_ranges(&self, clip: &Clip) -> (f64, f64) {
        let previous_end = self
            .clips
            .iter()
            .filter(|other| {
                other.start < clip.start || other.start == clip.start && other.id < clip.id
            })
            .map(Clip::end)
            .fold(clip.start, f64::max);
        if previous_end > clip.end() {
            return (clip.duration() / 2., clip.duration() / 2.);
        }
        let outgoing = self
            .clips
            .iter()
            .filter(|other| {
                (other.start > clip.start || other.start == clip.start && other.id > clip.id)
                    && other.start < clip.end()
                    && other.end() >= clip.end()
            })
            .map(|other| clip.end() - other.start)
            .fold(0., f64::max);
        ((previous_end - clip.start).max(0.), outgoing)
    }

    /// Convert ordinary fade weights to overlay alpha without applying them twice.
    #[cfg(test)]
    pub fn video_alpha(&self, clip: &Clip, time: f64) -> f64 {
        let gain = |c: &Clip| {
            if time >= c.start && time < c.end() {
                c.brightness(time - c.start)
            } else {
                0.
            }
        };
        let total: f64 = self.clips.iter().map(gain).sum();
        let later: f64 = self
            .clips
            .iter()
            .filter(|c| c.start > clip.start || c.start == clip.start && c.id > clip.id)
            .map(gain)
            .sum();
        let remaining = total.max(1.) - later;
        if remaining == 0. {
            0.
        } else {
            gain(clip) / remaining
        }
    }

    pub fn video_alpha_expression(&self, clip: &Clip, time: &str) -> String {
        let gain = |c: &Clip| c.fade_expression(time);
        let overlapping: Vec<_> = self
            .clips
            .iter()
            .filter(|c| c.start < clip.end() && c.end() > clip.start)
            .collect();
        let total = overlapping
            .iter()
            .map(|c| gain(c))
            .collect::<Vec<_>>()
            .join("+");
        let later = overlapping
            .iter()
            .filter(|c| c.start > clip.start || c.start == clip.start && c.id > clip.id)
            .map(|c| gain(c))
            .collect::<Vec<_>>();
        let remaining = if later.is_empty() {
            format!("max(1,({total}))")
        } else {
            format!("max(1,({total}))-({})", later.join("+"))
        };
        format!("if(gt(({remaining}),0),({})/({remaining}),0)", gain(clip))
    }
}

impl Project {
    pub fn sync_overlap_fades(&mut self, previous: &Project) {
        for track in &mut self.tracks {
            let changes: Vec<_> = track
                .clips
                .iter()
                .map(|clip| {
                    let current = track.crossfade_ranges(clip);
                    let old = previous
                        .clip(clip.id)
                        .map(|old| previous.clip_track(clip.id).unwrap().crossfade_ranges(old))
                        .unwrap_or((0., 0.));
                    (clip.id, old, current)
                })
                .collect();
            for (id, old, current) in changes {
                let clip = track.clips.iter_mut().find(|c| c.id == id).unwrap();
                if old.0 != current.0 {
                    clip.fade_in = current.0;
                }
                if old.1 != current.1 {
                    clip.fade_out = current.1;
                }
                clip.fit_fades();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Media, Project};

    #[test]
    fn overlapping_placement_blends_and_separation_removes_the_transition() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 4.,
            width: 640,
            height: 360,
            audio: false,
            frame_rate: None,
            generator: None,
        });
        let first = p.insert_media(0, Some(1), Some(0.)).unwrap();
        let second = p.insert_media(0, Some(1), Some(2.)).unwrap();
        p.validate().unwrap();
        let track = p.clip_track(second).unwrap();
        let clip = p.clip(second).unwrap();
        assert_eq!(track.crossfade_ranges(clip), (2., 0.));
        assert_eq!(track.video_alpha(clip, 2.), 0.);
        assert_eq!(track.video_alpha(clip, 3.), 0.5);
        assert_eq!(track.video_alpha(clip, 4.), 1.);
        assert_eq!(p.clip(first).unwrap().fade_out, 2.);
        assert_eq!(clip.fade_in, 2., "自动转场使用普通淡入参数");
        p.clip_mut(second).unwrap().fade_in = 1.;
        let edited = p.clone();
        p.sync_overlap_fades(&edited);
        assert_eq!(
            p.clip(second).unwrap().fade_in,
            1.,
            "未改变重叠时保留手柄编辑"
        );
        assert_eq!(
            p.clip_track(second)
                .unwrap()
                .video_alpha(p.clip(second).unwrap(), 3.),
            2. / 3.
        );
        let saved = Project::from_json(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(
            saved.clip(second).unwrap().fade_in,
            1.,
            "保存与打开应保留实际手柄参数"
        );
        let mut legacy = serde_json::to_value(&p).unwrap();
        legacy["version"] = 4.into();
        for track in legacy["tracks"].as_array_mut().unwrap() {
            for clip in track["clips"].as_array_mut().unwrap() {
                clip["fade_in"] = 0.into();
                clip["fade_out"] = 0.into();
            }
        }
        let migrated = Project::from_json(&legacy.to_string()).unwrap();
        assert_eq!(migrated.clip(first).unwrap().fade_out, 2.);
        assert_eq!(migrated.clip(second).unwrap().fade_in, 2.);
        p.move_clip(second, 1, 4.).unwrap();
        let track = p.clip_track(second).unwrap();
        assert_eq!(track.crossfade_ranges(p.clip(second).unwrap()), (0., 0.));
        assert_eq!(p.clip(first).unwrap().fade_out, 0.);
        assert_eq!(p.clip(second).unwrap().fade_in, 0.);
        let loaded = Project::from_json(&serde_json::to_string(&p).unwrap()).unwrap();
        loaded.validate().unwrap();
    }

    #[test]
    fn contained_event_blends_in_then_returns_to_the_background() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 10.,
            width: 640,
            height: 360,
            audio: false,
            frame_rate: None,
            generator: None,
        });
        p.insert_media(0, Some(1), Some(0.)).unwrap();
        let id = p.insert_media(0, Some(1), Some(2.)).unwrap();
        p.trim_clip(id, super::super::TrimEdge::Out, 6.).unwrap();
        let track = p.clip_track(id).unwrap();
        let clip = p.clip(id).unwrap();
        assert_eq!(track.crossfade_ranges(clip), (2., 2.));
        assert_eq!(track.video_alpha(clip, 3.), 1. / 3.);
        assert_eq!(track.video_alpha(clip, 4.), 0.5);
        assert_eq!(track.video_alpha(clip, 5.), 1. / 3.);
        assert_eq!(track.video_alpha(clip, 6.), 0.);
    }
}
