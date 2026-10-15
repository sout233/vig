use super::*;

impl Editor {
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let members = self.selection_members();
        let Some(primary) = self.selected else {
            self.error = Some("请选择要复制的片段".into());
            cx.notify();
            return;
        };
        let kind = self.project.clip_track(primary).unwrap().kind;
        let id = self
            .project
            .tracks
            .iter()
            .filter(|track| track.kind == kind)
            .flat_map(|track| &track.clips)
            .filter(|clip| members.binary_search(&clip.id).is_ok())
            .min_by(|a, b| a.start.total_cmp(&b.start))
            .unwrap()
            .id;
        let result = self
            .project
            .copy_selection(&members, id)
            .and_then(|payload| payload.to_json());
        match result {
            Ok(metadata) => {
                let name = self.project.media[self.project.clip(id).unwrap().media].display_name();
                cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(name, metadata));
                self.status = "片段已复制".into();
                self.error = None;
                cx.notify();
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn paste_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.close_color_editor(cx);
        let result = (|| {
            let clipboard = cx.read_from_clipboard().ok_or("剪贴板为空或无法读取")?;
            let metadata = clipboard.metadata().ok_or("剪贴板中没有 Vig 片段")?;
            let payload = crate::model::ClipClipboard::from_json(metadata)?;
            let mut next = self.project.clone();
            let first_id = next.next_id;
            let id = next.paste_clips(
                &payload,
                self.selected_track,
                self.position + payload.primary_offset(),
            )?;
            let members = next
                .tracks
                .iter()
                .flat_map(|track| &track.clips)
                .filter(|clip| clip.id >= first_id)
                .map(|clip| clip.id)
                .collect();
            Ok::<_, String>((next, id, members))
        })();
        match result {
            Ok((next, id, members)) => {
                self.checkpoint();
                self.project = next;
                self.selected = Some(id);
                self.number_edit = None;
                self.selection = members;
                self.selected_track = self.project.clip_track(id).map(|track| track.id);
                self.changed(window, cx);
                self.status = "片段已粘贴到播放头位置".into();
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }
}
