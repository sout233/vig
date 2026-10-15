use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct ClipClipboard {
    format: String,
    project: Project,
    primary: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_remaps_sources_ids_groups_and_preserves_linked_offsets() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut source = Project::default();
        source.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 8.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        let video = source.insert_media(0, Some(1), Some(2.)).unwrap();
        source.clip_mut(2).unwrap().start = 2.5;
        source.clip_mut(video).unwrap().transform.opacity = 0.7;
        let payload = source.copy_clips(video).unwrap();
        let payload = ClipClipboard::from_json(&payload.to_json().unwrap()).unwrap();
        let original = serde_json::to_string(&source).unwrap();
        let mut destination = Project::default();
        destination.tracks.clear();
        let first = destination.paste_clips(&payload, None, 10.).unwrap();
        assert_eq!(destination.clip(first).unwrap().start, 10.);
        let linked = destination.linked_ids(first);
        assert_eq!(linked.len(), 2);
        let sound = linked.iter().find(|id| **id != first).unwrap();
        assert_eq!(destination.clip(*sound).unwrap().start, 10.5);
        assert_eq!(destination.clip(first).unwrap().transform.opacity, 0.7);
        let second = destination.paste_clips(&payload, None, 20.).unwrap();
        assert_ne!(first, second);
        assert_ne!(
            destination.clip(first).unwrap().group,
            destination.clip(second).unwrap().group
        );
        assert_eq!(
            destination.media.len(),
            1,
            "文件素材应复用，片段和绑定组应分配新 ID"
        );
        destination.validate().unwrap();
        assert_eq!(serde_json::to_string(&source).unwrap(), original);
        let audio_track = destination.clip_track(*sound).unwrap().id;
        let before = serde_json::to_string(&destination).unwrap();
        assert!(
            destination
                .paste_clips(&payload, Some(audio_track), 30.)
                .is_err()
        );
        assert_eq!(serde_json::to_string(&destination).unwrap(), before);
    }

    #[test]
    fn copied_text_survives_source_deletion_and_keeps_an_independent_generator() {
        let mut project = Project::default();
        let id = project.add_subtitle(0.).unwrap();
        project.clip_mut(id).unwrap().fade_in = 0.3;
        project.clip_mut(id).unwrap().transform.rotation = 20.;
        let payload = project.copy_clips(id).unwrap();
        project.delete(id, false);
        let pasted = project.paste_clips(&payload, Some(1), 4.).unwrap();
        assert_eq!(project.clip(pasted).unwrap().fade_in, 0.3);
        assert_eq!(project.clip(pasted).unwrap().transform.rotation, 20.);
        let media = project.clip(pasted).unwrap().media;
        project.media[media]
            .generator
            .as_mut()
            .unwrap()
            .as_text_mut()
            .unwrap()
            .text = "新的文字".into();
        let another = project.paste_clips(&payload, Some(1), 8.).unwrap();
        assert_eq!(
            project.media[project.clip(another).unwrap().media]
                .generator
                .as_ref()
                .unwrap()
                .as_text()
                .unwrap()
                .text,
            "请输入字幕"
        );
        assert!(ClipClipboard::from_json("普通文字").is_err());
        let mut malformed = serde_json::to_value(payload).unwrap();
        malformed["format"] = "other-format".into();
        assert!(ClipClipboard::from_json(&malformed.to_string()).is_err());
    }

    #[test]
    fn pasting_a_group_before_its_earliest_member_is_atomic() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut project = Project::default();
        project.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 3.,
            width: 640,
            height: 360,
            audio: true,
            frame_rate: None,
            generator: None,
        });
        project.append(0);
        project.clip_mut(2).unwrap().start = 1.;
        let payload = project.copy_clips(2).unwrap();
        let before = serde_json::to_string(&project).unwrap();
        assert!(project.paste_clips(&payload, Some(2), 0.).is_err());
        assert_eq!(serde_json::to_string(&project).unwrap(), before);
    }
}

impl ClipClipboard {
    pub fn primary_offset(&self) -> f64 {
        let origin = self
            .project
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .map(|clip| clip.start)
            .fold(f64::INFINITY, f64::min);
        self.project.clip(self.primary).unwrap().start - origin
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|error| error.to_string())
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let payload: Self =
            serde_json::from_str(text).map_err(|error| format!("剪贴板片段数据无效：{error}"))?;
        if payload.format != "vig-clips-v1" && payload.format != "vig-clips-v2" {
            return Err("剪贴板中没有 Vig 片段".into());
        }
        payload.project.validate()?;
        if payload.project.clip(payload.primary).is_none() {
            return Err("剪贴板主片段不存在".into());
        }
        let linked = payload.project.linked_ids(payload.primary);
        if payload.format == "vig-clips-v1"
            && payload
                .project
                .tracks
                .iter()
                .map(|track| track.clips.len())
                .sum::<usize>()
                != linked.len()
        {
            return Err("剪贴板片段组无效".into());
        }
        Ok(payload)
    }
}

impl Project {
    #[cfg(test)]
    pub fn copy_clips(&self, id: u64) -> Result<ClipClipboard, String> {
        self.copy_selection(&[id], id)
    }

    pub fn copy_selection(&self, ids: &[u64], primary: u64) -> Result<ClipClipboard, String> {
        let members = self.selection_members(ids)?;
        if !members.contains(&primary) {
            return Err("请选择要复制的片段".into());
        }
        let mut project = self.clone();
        for track in &mut project.tracks {
            track.clips.retain(|clip| members.contains(&clip.id));
        }
        project.media.clear();
        let mut sources = std::collections::HashMap::new();
        for clip in project.tracks.iter_mut().flat_map(|track| &mut track.clips) {
            clip.media = *sources.entry(clip.media).or_insert_with(|| {
                let index = project.media.len();
                project.media.push(self.media[clip.media].clone());
                index
            });
        }
        project.validate()?;
        Ok(ClipClipboard {
            format: "vig-clips-v2".into(),
            project,
            primary,
        })
    }

    pub fn paste_clips(
        &mut self,
        payload: &ClipClipboard,
        target: Option<u64>,
        start: f64,
    ) -> Result<u64, String> {
        let primary = payload
            .project
            .clip(payload.primary)
            .ok_or("剪贴板主片段不存在")?;
        let kind = payload.project.clip_track(payload.primary).unwrap().kind;
        if !start.is_finite() || start < 0. {
            return Err("粘贴位置无效".into());
        }
        if let Some(target) = target {
            if self
                .tracks
                .iter()
                .find(|track| track.id == target)
                .ok_or("粘贴轨道不存在")?
                .kind
                != kind
            {
                return Err(
                    "请选中同类型轨道后粘贴：视频及文本只能进入视频轨，音频只能进入音频轨".into(),
                );
            }
        }
        let mut next = self.clone();
        let mut sources = Vec::new();
        for media in &payload.project.media {
            let existing = next.media.iter().position(|source| {
                media.generator.is_none() && source.generator.is_none() && source.path == media.path
            });
            let index = existing.unwrap_or_else(|| {
                let index = next.media.len();
                next.media.push(media.clone());
                index
            });
            sources.push(index);
        }
        let mut groups = std::collections::HashMap::new();
        let source_lanes: Vec<_> = payload
            .project
            .tracks
            .iter()
            .filter(|track| track.kind == kind)
            .map(|track| track.id)
            .collect();
        let source_primary = source_lanes
            .iter()
            .position(|id| *id == payload.project.clip_track(payload.primary).unwrap().id)
            .unwrap();
        let target_primary = target.map(|id| {
            next.tracks
                .iter()
                .filter(|track| track.kind == kind)
                .position(|track| track.id == id)
                .unwrap()
        });
        let mut selected = None;
        for source_track in &payload.project.tracks {
            if source_track.clips.is_empty() {
                continue;
            }
            let destination = if source_track.kind == kind && target_primary.is_some() {
                let index = target_primary.unwrap() as isize
                    + source_lanes
                        .iter()
                        .position(|id| *id == source_track.id)
                        .unwrap() as isize
                    - source_primary as isize;
                let index = usize::try_from(index).map_err(|_| "粘贴片段超出可用轨道")?;
                while next
                    .tracks
                    .iter()
                    .filter(|track| track.kind == kind)
                    .count()
                    <= index
                {
                    next.add_track(kind);
                }
                Some(
                    next.tracks
                        .iter()
                        .filter(|track| track.kind == kind)
                        .nth(index)
                        .unwrap()
                        .id,
                )
            } else if let Some(existing) = next
                .tracks
                .iter()
                .find(|track| track.id == source_track.id && track.kind == source_track.kind)
            {
                Some(existing.id)
            } else {
                let index = payload
                    .project
                    .tracks
                    .iter()
                    .filter(|track| track.kind == source_track.kind)
                    .position(|track| track.id == source_track.id)
                    .unwrap();
                while next
                    .tracks
                    .iter()
                    .filter(|track| track.kind == source_track.kind)
                    .count()
                    <= index
                {
                    next.add_track(source_track.kind);
                }
                Some(
                    next.tracks
                        .iter()
                        .filter(|track| track.kind == source_track.kind)
                        .nth(index)
                        .unwrap()
                        .id,
                )
            };
            let track = destination.unwrap();
            for original in &source_track.clips {
                let mut clip = original.clone();
                clip.id = next.next_id;
                next.next_id += 1;
                clip.start = start + original.start - primary.start;
                clip.media = sources[original.media];
                clip.group = original
                    .group
                    .map(|group| *groups.entry(group).or_insert(clip.id));
                if original.id == payload.primary {
                    selected = Some(clip.id);
                }
                next.tracks
                    .iter_mut()
                    .find(|destination| destination.id == track)
                    .unwrap()
                    .clips
                    .push(clip);
            }
        }
        next.validate()?;
        next.sync_overlap_fades(self);
        *self = next;
        Ok(selected.unwrap())
    }
}
