use super::*;

impl Project {
    pub fn linked_ids(&self, id: u64) -> Vec<u64> {
        let clip = self.clip(id).expect("片段不存在");
        self.tracks
            .iter()
            .flat_map(|t| &t.clips)
            .filter(|c| c.id == id || clip.group.is_some() && c.group == clip.group)
            .map(|c| c.id)
            .collect()
    }

    pub fn unlink(&mut self, id: u64) {
        for id in self.linked_ids(id) {
            self.clip_mut(id).unwrap().group = None;
        }
    }

    pub fn clean_links(&mut self) {
        let mut counts = std::collections::HashMap::new();
        for c in self.tracks.iter().flat_map(|t| &t.clips) {
            if let Some(g) = c.group {
                *counts.entry(g).or_insert(0) += 1;
            }
        }
        for c in self.tracks.iter_mut().flat_map(|t| &mut t.clips) {
            if c.group.is_some_and(|g| counts[&g] < 2) {
                c.group = None;
            }
        }
    }

    pub fn link_candidate(&self, id: u64) -> Option<u64> {
        let clip = self.clip(id)?;
        let kind = self.clip_track(id)?.kind;
        let mut candidates = self
            .tracks
            .iter()
            .filter(|t| t.kind != kind)
            .flat_map(|t| &t.clips)
            .filter(|c| {
                c.group.is_none()
                    && c.media == clip.media
                    && c.start < clip.end()
                    && c.end() > clip.start
            });
        let candidate = candidates.next()?.id;
        if candidates.next().is_some() {
            None
        } else {
            Some(candidate)
        }
    }

    pub fn link(&mut self, id: u64) -> Result<(), String> {
        if self.clip(id).ok_or("片段不存在")?.group.is_some() {
            return Err("片段已经绑定".into());
        }
        let other = self
            .link_candidate(id)
            .ok_or("没有唯一可绑定的同源视频 / 音频块")?;
        self.clip_mut(id).unwrap().group = Some(id);
        self.clip_mut(other).unwrap().group = Some(id);
        Ok(())
    }

    #[cfg(test)]
    pub fn move_linked(&mut self, id: u64, track: u64, start: f64) -> Result<(), String> {
        let delta = start - self.clip(id).ok_or("片段不存在")?.start;
        let mut next = self.clone();
        for member in self.linked_ids(id) {
            let target = if member == id {
                track
            } else {
                self.clip_track(member).unwrap().id
            };
            next.move_clip(member, target, self.clip(member).unwrap().start + delta)?;
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    #[cfg(test)]
    pub fn duplicate(&mut self, id: u64) -> u64 {
        let ids = self.linked_ids(id);
        let origin = ids
            .iter()
            .map(|id| self.clip(*id).unwrap().start)
            .fold(f64::INFINITY, f64::min);
        let delta = self.duration() - origin;
        let group = if ids.len() > 1 {
            Some(self.next_id)
        } else {
            None
        };
        let mut selected = 0;
        for old in ids {
            let track = self.clip_track(old).unwrap().id;
            let mut clip = self.clip(old).unwrap().clone();
            clip.id = self.next_id;
            self.next_id += 1;
            clip.start += delta;
            clip.group = group;
            if old == id {
                selected = clip.id;
            }
            self.tracks
                .iter_mut()
                .find(|t| t.id == track)
                .unwrap()
                .clips
                .push(clip);
        }
        selected
    }

    pub fn split_linked(&mut self, id: u64, time: f64) -> Result<(), String> {
        let members = self.linked_ids(id);
        let mut next = self.clone();
        let right_group = if members.len() > 1 {
            Some(next.next_id)
        } else {
            None
        };
        for member in members {
            let right = next.next_id;
            next.split(member, time)?;
            next.clip_mut(right).unwrap().group = right_group;
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_move_allows_audio_overlap_and_rejects_negative_time_atomically() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 4.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        p.media.push(Media {
            path: file.path().into(),
            name: "音频".into(),
            duration: 4.,
            width: 0,
            height: 0,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        p.insert_media(1, Some(2), Some(5.)).unwrap();
        p.move_linked(1, 1, 2.).unwrap();
        p.validate().unwrap();
        assert_eq!(p.clip(2).unwrap().start, 2.);
        let snapshot = serde_json::to_string(&p).unwrap();
        assert!(p.move_linked(1, 1, -1.).is_err());
        assert_eq!(
            snapshot,
            serde_json::to_string(&p).unwrap(),
            "非法移动应恢复整个绑定组"
        );
    }

    #[test]
    fn paired_move_unlink_rebind_split_and_copy_are_consistent() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 4.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        assert_eq!(p.linked_ids(1), vec![1, 2]);
        p.move_linked(1, 1, 2.).unwrap();
        assert_eq!(p.clip(2).unwrap().start, 2.);
        p.move_linked(2, 2, 3.).unwrap();
        assert_eq!(p.clip(1).unwrap().start, 3.);
        p.unlink(1);
        p.move_linked(1, 1, 3.5).unwrap();
        assert_eq!(p.clip(2).unwrap().start, 3.);
        p.link(1).unwrap();
        p.move_linked(2, 2, 4.).unwrap();
        assert_eq!(p.clip(1).unwrap().start, 4.5);
        p.split_linked(1, 5.).unwrap();
        assert_eq!(p.linked_ids(1), vec![1, 2]);
        assert_eq!(p.linked_ids(3), vec![3, 4]);
        let id = p.duplicate(3);
        assert_eq!(p.linked_ids(id).len(), 2);
        assert_ne!(p.clip(id).unwrap().group, p.clip(3).unwrap().group);
        let snapshot = serde_json::to_string(&p).unwrap();
        assert!(p.move_linked(id, 1, -1.).is_err());
        assert_eq!(snapshot, serde_json::to_string(&p).unwrap());
        p.validate().unwrap();
        let loaded = Project::from_json(&snapshot).unwrap();
        assert_eq!(loaded.linked_ids(id).len(), 2);
        p.delete(1, false);
        assert_eq!(p.clip(2).unwrap().group, None);
        p.validate().unwrap();
    }
}
