use super::*;

impl Project {
    /// Place a source once, then route its linked audio at the same time.
    pub fn insert_media(
        &mut self,
        media: usize,
        preferred: Option<u64>,
        time: Option<f64>,
    ) -> Result<u64, String> {
        let source = self.media.get(media).ok_or("素材不存在")?;
        let kind = if source.width > 0 {
            TrackKind::Video
        } else {
            TrackKind::Audio
        };
        let track = self
            .tracks
            .iter()
            .find(|t| Some(t.id) == preferred && t.kind == kind)
            .or_else(|| self.tracks.iter().find(|t| t.kind == kind));
        if time.is_some() && track.map(|t| t.id) != preferred {
            return Err("视频只能放入视频轨，音频只能放入音频轨".into());
        }
        let start = time.unwrap_or_else(|| {
            track
                .map(|t| t.clips.iter().map(Clip::end).fold(0., f64::max))
                .unwrap_or(0.)
        });
        if !start.is_finite() || start < 0. {
            return Err("目标位置无效".into());
        }
        let split_audio = kind == TrackKind::Video && source.audio;
        let previous = self.clone();
        let id = self.append_to(media, preferred);
        self.clip_mut(id).unwrap().start = start;
        if split_audio {
            self.separate_audio(self.clip(id).unwrap().clone());
        }
        self.sync_overlap_fades(&previous);
        Ok(id)
    }

    pub(super) fn separate_audio(&mut self, mut clip: Clip) {
        let group = clip.id;
        self.clip_mut(clip.id).unwrap().group = Some(group);
        clip.group = Some(group);
        let video_index = self
            .tracks
            .iter()
            .position(|t| t.clips.iter().any(|c| c.id == clip.id))
            .unwrap();
        let available = |t: &&Track| {
            t.kind == TrackKind::Audio
                && t.clips
                    .iter()
                    .all(|c| clip.start >= c.end() - 0.001 || clip.end() <= c.start + 0.001)
        };
        // Overlapping events on one video track share their paired audio lane.
        let paired_lane = self.tracks[video_index]
            .ordered_clips()
            .into_iter()
            .filter(|other| {
                other.id != clip.id && other.start < clip.end() && other.end() > clip.start
            })
            .filter_map(|other| other.group)
            .find_map(|group| {
                self.tracks
                    .iter()
                    .find(|track| {
                        track.kind == TrackKind::Audio
                            && track.clips.iter().any(|audio| audio.group == Some(group))
                    })
                    .map(|track| track.id)
            });
        // Otherwise prefer a free audio lane below the video, then the remaining lanes.
        let target = paired_lane.or_else(|| {
            self.tracks[video_index + 1..]
                .iter()
                .find(available)
                .or_else(|| self.tracks[..video_index].iter().find(available))
                .map(|t| t.id)
        });
        let target = target.unwrap_or_else(|| {
            let id = self.add_track(TrackKind::Audio);
            self.reorder_track(id, video_index + 1);
            id
        });
        clip.id = self.next_id;
        self.next_id += 1;
        self.tracks
            .iter_mut()
            .find(|t| t.id == target)
            .unwrap()
            .clips
            .push(clip);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::NamedTempFile, Project) {
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
        (file, p)
    }

    #[test]
    fn overlapping_video_events_share_the_linked_audio_lane() {
        let (_file, mut p) = fixture();
        p.insert_media(0, Some(1), Some(0.)).unwrap();
        p.insert_media(0, Some(1), Some(2.)).unwrap();
        assert_eq!(p.clip_track(2).unwrap().id, p.clip_track(4).unwrap().id);
        assert_eq!(p.tracks.len(), 2);
        assert_eq!(p.linked_ids(3), vec![3, 4]);
        let audio = p.clip_track(4).unwrap();
        assert_eq!(audio.crossfade_ranges(p.clip(4).unwrap()), (2., 0.));
        p.validate().unwrap();
    }

    #[test]
    fn video_and_audio_have_matching_source_and_time() {
        let (_file, mut p) = fixture();
        let id = p.insert_media(0, Some(1), Some(2.)).unwrap();
        let video = p.clip(id).unwrap();
        let sound = &p.tracks[1].clips[0];
        assert_ne!(sound.id, video.id);
        assert_eq!(sound.start, 2.);
        assert_eq!(sound.media, video.media);
        assert_eq!(sound.source_in, video.source_in);
        assert_eq!(sound.source_out, video.source_out);
        p.validate().unwrap();
        let loaded = Project::from_json(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(loaded.next_id, 3, "打开新项目不能再次拆分");
        loaded.validate().unwrap();
    }

    #[test]
    fn occupied_audio_lane_creates_one_and_free_lane_is_reused() {
        let (_file, mut p) = fixture();
        p.insert_media(0, Some(1), Some(0.)).unwrap();
        let video = p.add_track(TrackKind::Video);
        p.insert_media(0, Some(video), Some(1.)).unwrap();
        let audio = p.clip_track(4).unwrap().id;
        assert_ne!(audio, 2);
        assert_eq!(p.tracks.len(), 4);
        p.insert_media(0, Some(video), Some(6.)).unwrap();
        assert_eq!(p.clip_track(6).unwrap().id, audio);
        assert_eq!(p.tracks.len(), 4);
        p.validate().unwrap();
    }

    #[test]
    fn missing_audio_track_and_silent_video() {
        let (_file, mut p) = fixture();
        p.tracks.retain(|t| t.kind == TrackKind::Video);
        p.insert_media(0, Some(1), Some(0.)).unwrap();
        assert_eq!(p.tracks[1].kind, TrackKind::Audio);
        p.media[0].audio = false;
        p.tracks
            .iter_mut()
            .filter(|t| t.kind == TrackKind::Audio)
            .for_each(|t| t.clips.clear());
        p.clean_links();
        let video = p.add_track(TrackKind::Video);
        let count = p.next_id;
        p.insert_media(0, Some(video), Some(0.)).unwrap();
        assert_eq!(p.next_id, count + 1);
        p.validate().unwrap();
    }

    #[test]
    fn migrate_embedded_audio_preserves_trim_speed_and_volume() {
        let (_file, mut p) = fixture();
        p.append_to(0, Some(1));
        let clip = p.clip_mut(1).unwrap();
        clip.source_in = 1.;
        clip.source_out = 3.;
        clip.start = 5.;
        clip.speed = 2.;
        clip.volume = 0.4;
        clip.fade_in = 0.2;
        clip.fade_out = 0.2;
        p.version = 2;
        let loaded = Project::from_json(&serde_json::to_string(&p).unwrap()).unwrap();
        loaded.validate().unwrap();
        let sound = loaded.clip(2).unwrap();
        assert_eq!(sound.start, 5.);
        assert_eq!(sound.source_in, 1.);
        assert_eq!(sound.source_out, 3.);
        assert_eq!(sound.speed, 2.);
        assert_eq!(sound.volume, 0.4);
        assert_eq!(sound.fade_in, 0.2);
        assert_eq!(sound.fade_out, 0.2);
    }
}
