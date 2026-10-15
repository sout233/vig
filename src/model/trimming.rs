use super::*;

#[derive(Clone, Copy, PartialEq)]
pub enum TrimEdge {
    In,
    Out,
}

impl Project {
    /// Pointer overshoot stops at the shared source boundary of the linked group.
    /// Explicit trim commands retain strict validation in `trim_clip`.
    pub fn drag_trim_edge(&mut self, id: u64, edge: TrimEdge, time: f64) -> Result<(), String> {
        if !time.is_finite() {
            return Err("裁剪位置无效".into());
        }
        let selected = self.clip(id).ok_or("片段不存在")?;
        let origin = match edge {
            TrimEdge::In => selected.start,
            TrimEdge::Out => selected.end(),
        };
        let mut delta = time - origin;
        for member in self.linked_ids(id) {
            let clip = self.clip(member).unwrap();
            match edge {
                TrimEdge::In => {
                    delta = delta.max((-clip.source_in / clip.speed).max(-clip.start));
                }
                TrimEdge::Out => {
                    let media = self.media.get(clip.media).ok_or("片段素材引用无效")?;
                    if media.generator.is_none() {
                        delta = delta.min((media.duration - clip.source_out) / clip.speed);
                    }
                }
            }
        }
        self.trim_clip(id, edge, origin + delta)
    }

    pub fn trim_clip(&mut self, id: u64, edge: TrimEdge, time: f64) -> Result<(), String> {
        if !time.is_finite() {
            return Err("裁剪位置无效".into());
        }
        let previous = self.clone();
        let selected = self.clip(id).ok_or("片段不存在")?;
        let delta = time
            - match edge {
                TrimEdge::In => selected.start,
                TrimEdge::Out => selected.end(),
            };
        let originals: Vec<_> = self
            .linked_ids(id)
            .into_iter()
            .map(|member| self.clip(member).unwrap().clone())
            .collect();
        for original in &originals {
            let clip = self.clip_mut(original.id).unwrap();
            match edge {
                TrimEdge::In => {
                    clip.source_in = original.source_in + delta * original.speed;
                    clip.start = original.start + delta;
                }
                TrimEdge::Out => {
                    clip.source_out = original.source_out + delta * original.speed;
                }
            }
            clip.fit_fades();
        }
        if let Err(error) = self.validate() {
            for original in originals {
                let member = original.id;
                *self.clip_mut(member).unwrap() = original;
            }
            return Err(format!("裁剪失败：{error}；请保持源区间有效"));
        }
        self.sync_overlap_fades(&previous);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_overshoot_stops_at_the_linked_source_boundaries() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 10.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        p.trim_clip(1, TrimEdge::In, 2.).unwrap();
        p.trim_clip(1, TrimEdge::Out, 8.).unwrap();
        p.drag_trim_edge(1, TrimEdge::Out, 100.).unwrap();
        for id in [1, 2] {
            assert_eq!(p.clip(id).unwrap().source_out, 10.);
            assert_eq!(p.clip(id).unwrap().end(), 10.);
        }
        let snapshot = serde_json::to_string(&p).unwrap();
        p.drag_trim_edge(2, TrimEdge::Out, 200.).unwrap();
        assert_eq!(snapshot, serde_json::to_string(&p).unwrap());
        p.drag_trim_edge(2, TrimEdge::In, -100.).unwrap();
        for id in [1, 2] {
            assert_eq!(p.clip(id).unwrap().start, 0.);
            assert_eq!(p.clip(id).unwrap().source_in, 0.);
        }
        p.validate().unwrap();
        assert!(p.drag_trim_edge(1, TrimEdge::Out, f64::NAN).is_err());
    }

    #[test]
    fn pointer_extension_uses_the_most_restrictive_linked_speed_and_source() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 10.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        p.trim_clip(1, TrimEdge::In, 2.).unwrap();
        p.trim_clip(1, TrimEdge::Out, 6.).unwrap();
        let audio = p.clip_mut(2).unwrap();
        audio.source_out = 9.;
        audio.speed = 2.;
        p.drag_trim_edge(1, TrimEdge::Out, 100.).unwrap();
        assert_eq!(p.clip(1).unwrap().end(), 6.5);
        assert_eq!(p.clip(1).unwrap().source_out, 6.5);
        assert_eq!(p.clip(2).unwrap().source_out, 10.);
        assert_eq!(p.clip(2).unwrap().end(), 6.);
        p.validate().unwrap();
    }

    #[test]
    fn either_linked_edge_trims_both_and_unlink_restores_independent_edits() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 10.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        p.trim_clip(1, TrimEdge::In, 2.).unwrap();
        for id in [1, 2] {
            let clip = p.clip(id).unwrap();
            assert_eq!((clip.start, clip.source_in, clip.end()), (2., 2., 10.));
        }
        p.trim_clip(2, TrimEdge::Out, 8.).unwrap();
        for id in [1, 2] {
            assert_eq!(p.clip(id).unwrap().source_out, 8.);
        }
        // 不同起点与速度的绑定成员保持时间偏移，按自己的速度裁剪源区间。
        let audio = p.clip_mut(2).unwrap();
        audio.start = 3.;
        audio.speed = 2.;
        p.trim_clip(2, TrimEdge::In, 4.).unwrap();
        assert_eq!(p.clip(1).unwrap().start, 3.);
        assert_eq!(p.clip(1).unwrap().source_in, 3.);
        assert_eq!(p.clip(2).unwrap().source_in, 4.);
        let snapshot = serde_json::to_string(&p).unwrap();
        assert!(p.trim_clip(1, TrimEdge::Out, 11.).is_err());
        assert_eq!(snapshot, serde_json::to_string(&p).unwrap());
        p.unlink(1);
        p.trim_clip(2, TrimEdge::Out, 5.).unwrap();
        assert_eq!(p.clip(1).unwrap().source_out, 8.);
        assert_eq!(p.clip(2).unwrap().source_out, 6.);
    }

    #[test]
    fn linked_trim_allows_overlap() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 10.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        p.trim_clip(1, TrimEdge::Out, 4.).unwrap();
        p.insert_media(0, Some(1), Some(5.)).unwrap();
        p.trim_clip(2, TrimEdge::Out, 6.).unwrap();
        assert_eq!(p.clip(1).unwrap().end(), 6.);
        assert_eq!(p.clip(2).unwrap().end(), 6.);
        p.validate().unwrap();
    }

    #[test]
    fn edges_follow_source_speed_and_invalid_edits_are_atomic() {
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
        p.append(0);
        let c = p.clip_mut(1).unwrap();
        c.start = 3.;
        c.source_in = 2.;
        c.source_out = 8.;
        c.speed = 2.;
        c.fade_in = 1.;
        c.fade_out = 1.;
        p.trim_clip(1, TrimEdge::In, 4.).unwrap();
        assert_eq!(p.clip(1).unwrap().source_in, 4.);
        assert_eq!(p.clip(1).unwrap().end(), 6.);
        p.trim_clip(1, TrimEdge::Out, 5.).unwrap();
        assert_eq!(p.clip(1).unwrap().source_out, 6.);
        assert_eq!(p.clip(1).unwrap().fade_in, 0.5);
        p.trim_clip(1, TrimEdge::In, 3.).unwrap();
        assert_eq!(p.clip(1).unwrap().source_in, 2.);
        p.trim_clip(1, TrimEdge::Out, 7.).unwrap();
        assert_eq!(p.clip(1).unwrap().source_out, 10.);
        let snapshot = serde_json::to_string(&p).unwrap();
        for (edge, time) in [
            (TrimEdge::In, 1.),
            (TrimEdge::Out, 8.),
            (TrimEdge::In, 7.),
            (TrimEdge::Out, f64::NAN),
        ] {
            assert!(p.trim_clip(1, edge, time).is_err());
            assert_eq!(snapshot, serde_json::to_string(&p).unwrap());
        }
        p.append(0);
        p.clip_mut(2).unwrap().source_in = 2.;
        p.trim_clip(2, TrimEdge::In, 6.).unwrap();
        assert_eq!(p.clip(2).unwrap().start, 6.);
    }
}
