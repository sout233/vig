use super::*;
use std::collections::BTreeSet;

impl Project {
    pub fn selection_members(&self, ids: &[u64]) -> Result<BTreeSet<u64>, String> {
        let mut members = BTreeSet::new();
        let by_id: std::collections::HashMap<_, _> = self
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .map(|clip| (clip.id, clip.group))
            .collect();
        let mut groups = std::collections::HashSet::new();
        for id in ids {
            let group = by_id.get(id).ok_or("所选片段不存在")?;
            members.insert(*id);
            if let Some(group) = group {
                groups.insert(*group);
            }
        }
        for clip in self.tracks.iter().flat_map(|track| &track.clips) {
            if clip.group.is_some_and(|group| groups.contains(&group)) {
                members.insert(clip.id);
            }
        }
        Ok(members)
    }

    pub fn move_selection(
        &mut self,
        ids: &[u64],
        primary: u64,
        target: u64,
        start: f64,
    ) -> Result<(), String> {
        let members = self.selection_members(ids)?;
        if !members.contains(&primary) {
            return Err("拖动片段不在选择中".into());
        }
        let source = self.clip_track(primary).unwrap();
        let target = self
            .tracks
            .iter()
            .find(|track| track.id == target)
            .ok_or("目标轨道不存在")?;
        if source.kind != target.kind {
            return Err("请将片段放到同类型轨道中".into());
        }
        let same_kind: Vec<_> = self
            .tracks
            .iter()
            .filter(|track| track.kind == source.kind)
            .map(|track| track.id)
            .collect();
        let offset = same_kind.iter().position(|id| *id == target.id).unwrap() as isize
            - same_kind.iter().position(|id| *id == source.id).unwrap() as isize;
        let delta = start - self.clip(primary).unwrap().start;
        let mut placements = Vec::new();
        for track in &self.tracks {
            let destination = if track.kind == source.kind
                && track.clips.iter().any(|clip| members.contains(&clip.id))
            {
                let index =
                    same_kind.iter().position(|id| *id == track.id).unwrap() as isize + offset;
                *same_kind
                    .get(usize::try_from(index).map_err(|_| "所选片段超出可用轨道")?)
                    .ok_or("所选片段超出可用轨道")?
            } else {
                track.id
            };
            for clip in track.clips.iter().filter(|clip| members.contains(&clip.id)) {
                let mut clip = clip.clone();
                clip.start += delta;
                placements.push((destination, clip));
            }
        }
        let mut next = self.clone();
        for track in &mut next.tracks {
            track.clips.retain(|clip| !members.contains(&clip.id));
        }
        for (track, clip) in placements {
            next.tracks
                .iter_mut()
                .find(|destination| destination.id == track)
                .unwrap()
                .clips
                .push(clip);
        }
        next.validate()?;
        next.sync_overlap_fades(self);
        *self = next;
        Ok(())
    }

    pub fn delete_selection(&mut self, ids: &[u64], ripple: bool) -> Result<(), String> {
        let members = self.selection_members(ids)?;
        let mut next = self.clone();
        for track in &mut next.tracks {
            let mut intervals: Vec<_> = track
                .clips
                .iter()
                .filter(|clip| members.contains(&clip.id))
                .map(|clip| (clip.start, clip.end()))
                .collect();
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f64, f64)> = Vec::new();
            for (start, end) in intervals {
                if let Some(last) = merged.last_mut().filter(|last| start <= last.1) {
                    last.1 = last.1.max(end);
                } else {
                    merged.push((start, end));
                }
            }
            track.clips.retain(|clip| !members.contains(&clip.id));
            if ripple {
                for clip in &mut track.clips {
                    clip.start -= merged
                        .iter()
                        .filter(|(_, end)| *end <= clip.start)
                        .map(|(start, end)| end - start)
                        .sum::<f64>();
                }
            }
        }
        next.clean_links();
        next.validate()?;
        next.sync_overlap_fades(self);
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiple_groups_copy_move_and_delete_keep_offsets_and_distinct_bindings() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut project = Project::default();
        project.media.push(Media {
            path: file.path().into(),
            name: "movie.mp4".into(),
            duration: 3.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: Some(FrameRate::default()),
            generator: None,
        });
        let first = project.insert_media(0, Some(1), Some(1.)).unwrap();
        let second = project.insert_media(0, Some(1), Some(6.)).unwrap();
        let payload = project.copy_selection(&[first, second], first).unwrap();
        let payload = ClipClipboard::from_json(&payload.to_json().unwrap()).unwrap();
        let first_id = project.next_id;
        let pasted = project.paste_clips(&payload, Some(1), 12.).unwrap();
        let copies: Vec<_> = project
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .filter(|clip| clip.id >= first_id)
            .map(|clip| clip.id)
            .collect();
        assert_eq!(copies.len(), 4);
        assert_eq!(
            project.linked_ids(pasted).len(),
            2,
            "两个音视频组不能被合成一个绑定组"
        );
        let video: Vec<_> = copies
            .iter()
            .copied()
            .filter(|id| project.clip_track(*id).unwrap().kind == TrackKind::Video)
            .collect();
        assert_ne!(
            project.clip(video[0]).unwrap().group,
            project.clip(video[1]).unwrap().group
        );
        project.move_selection(&copies, pasted, 1, 14.).unwrap();
        assert_eq!(project.clip(video[0]).unwrap().start, 14.);
        assert_eq!(project.clip(video[1]).unwrap().start, 19.);
        let before = serde_json::to_string(&project).unwrap();
        assert!(project.move_selection(&copies, pasted, 2, 10.).is_err());
        assert_eq!(serde_json::to_string(&project).unwrap(), before);
        project.delete_selection(&copies, false).unwrap();
        assert_eq!(project.video().count(), 2);
        assert_eq!(project.linked_ids(first).len(), 2);
    }

    #[test]
    fn mixed_selection_pastes_its_earliest_event_at_the_playhead() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut project = Project::default();
        let video = project
            .add_generator(
                Generator::Solid {
                    color: "FFFFFF".into(),
                },
                5.,
                Some(1),
            )
            .unwrap();
        project.media.push(Media {
            path: file.path().into(),
            name: "audio.wav".into(),
            duration: 2.,
            width: 0,
            height: 0,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        let audio = project.insert_media(1, Some(2), Some(1.)).unwrap();
        let payload = project.copy_selection(&[video, audio], video).unwrap();
        assert_eq!(payload.primary_offset(), 4.);
        let id = project
            .paste_clips(&payload, Some(1), 20. + payload.primary_offset())
            .unwrap();
        assert_eq!(project.clip(id).unwrap().start, 24.);
        assert!(
            project
                .tracks
                .iter()
                .filter(|track| track.kind == TrackKind::Audio)
                .flat_map(|track| &track.clips)
                .any(|clip| clip.start == 20.)
        );
    }
}
