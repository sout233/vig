use super::*;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};

#[derive(Clone, Copy)]
enum ClipCommand {
    Unlink,
    Link,
    Duplicate,
    Copy,
    Paste,
    Split,
    Delete,
    Ripple,
    Mute,
    ResetFades,
    Start,
    End,
}

impl Editor {
    pub(super) fn clip_menu(
        &self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + use<> {
        let owner = cx.entity().downgrade();
        let focus = self.focus.clone();
        let clip = self.project.clip(id).unwrap();
        let locked = self.busy || self.job == Some(Job::Export);
        let audio = self
            .project
            .linked_ids(id)
            .into_iter()
            .find(|id| self.project.clip_track(*id).unwrap().kind == TrackKind::Audio);
        let muted = audio.is_some_and(|id| self.project.clip(id).unwrap().muted);
        let items = [
            (
                "解除音视频绑定",
                ClipCommand::Unlink,
                locked || clip.group.is_none(),
            ),
            (
                "绑定同源音视频",
                ClipCommand::Link,
                locked || clip.group.is_some() || self.project.link_candidate(id).is_none(),
            ),
            ("复制到序列末尾", ClipCommand::Duplicate, locked),
            ("复制", ClipCommand::Copy, locked),
            ("粘贴到播放头", ClipCommand::Paste, locked),
            (
                "在播放头处分割",
                ClipCommand::Split,
                locked || self.position <= clip.start || self.position >= clip.end(),
            ),
            ("删除", ClipCommand::Delete, locked),
            ("删除并闭合空隙", ClipCommand::Ripple, locked),
            (
                if muted {
                    "恢复音频声音"
                } else {
                    "静音音频"
                },
                ClipCommand::Mute,
                locked || audio.is_none(),
            ),
            (
                "重置淡入淡出",
                ClipCommand::ResetFades,
                locked || clip.fade_in + clip.fade_out == 0.,
            ),
            ("定位到片段起点", ClipCommand::Start, locked),
            ("定位到片段终点", ClipCommand::End, locked),
        ];
        move |mut menu, _, _| {
            menu = menu.action_context(focus.clone());
            for (label, command, disabled) in items {
                let owner = owner.clone();
                let action: Option<Box<dyn Action>> = match command {
                    ClipCommand::Copy => Some(Box::new(CopyClips)),
                    ClipCommand::Paste => Some(Box::new(PasteClips)),
                    _ => None,
                };
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .when_some(action, |item, action| item.action(action))
                        .disabled(disabled)
                        .on_click(move |_, window, cx| {
                            let owner = owner.clone();
                            window.defer(cx, move |window, cx| {
                                owner
                                    .update(cx, |this, cx| {
                                        this.close_color_editor(cx);
                                        this.selected = Some(id);
                                        this.selected_track =
                                            this.project.clip_track(id).map(|t| t.id);
                                        this.focus.focus(window, cx);
                                        match command {
                                            ClipCommand::Copy => this.copy_selection(cx),
                                            ClipCommand::Paste => this.paste_selection(window, cx),
                                            ClipCommand::Split => this.split(window, cx),
                                            ClipCommand::Delete => this.delete(false, window, cx),
                                            ClipCommand::Ripple => this.delete(true, window, cx),
                                            ClipCommand::Start => {
                                                this.seek(this.project.clip(id).unwrap().start, cx)
                                            }
                                            ClipCommand::End => {
                                                this.seek(this.project.clip(id).unwrap().end(), cx)
                                            }
                                            _ => {
                                                let mut next = this.project.clone();
                                                let mut selected = id;
                                                let result = match command {
                                                    ClipCommand::Unlink => {
                                                        next.unlink(id);
                                                        Ok(())
                                                    }
                                                    ClipCommand::Link => next.link(id),
                                                    ClipCommand::Duplicate => {
                                                        let members = this.selection_members();
                                                        let primary = members
                                                            .iter()
                                                            .copied()
                                                            .min_by(|a, b| {
                                                                next.clip(*a)
                                                                    .unwrap()
                                                                    .start
                                                                    .total_cmp(
                                                                        &next
                                                                            .clip(*b)
                                                                            .unwrap()
                                                                            .start,
                                                                    )
                                                            })
                                                            .unwrap();
                                                        next.copy_selection(&members, primary)
                                                            .and_then(|payload| {
                                                                next.paste_clips(
                                                                    &payload,
                                                                    None,
                                                                    next.duration(),
                                                                )
                                                            })
                                                            .map(|id| {
                                                                selected = id;
                                                            })
                                                    }
                                                    ClipCommand::Mute => {
                                                        let clip =
                                                            next.clip_mut(audio.unwrap()).unwrap();
                                                        clip.muted = !clip.muted;
                                                        Ok(())
                                                    }
                                                    ClipCommand::ResetFades => {
                                                        let clip = next.clip_mut(id).unwrap();
                                                        clip.fade_in = 0.;
                                                        clip.fade_out = 0.;
                                                        Ok(())
                                                    }
                                                    _ => unreachable!(),
                                                };
                                                match result {
                                                    Ok(()) => {
                                                        if matches!(command, ClipCommand::Duplicate)
                                                        {
                                                            this.selection = next
                                                                .tracks
                                                                .iter()
                                                                .flat_map(|track| &track.clips)
                                                                .filter(|clip| {
                                                                    clip.id >= this.project.next_id
                                                                })
                                                                .map(|clip| clip.id)
                                                                .collect();
                                                        } else if matches!(
                                                            command,
                                                            ClipCommand::Unlink
                                                        ) {
                                                            this.selection.clear();
                                                        }
                                                        this.checkpoint();
                                                        this.project = next;
                                                        this.selected = Some(selected);
                                                        this.changed(window, cx);
                                                    }
                                                    Err(error) => {
                                                        this.error = Some(error);
                                                        cx.notify();
                                                    }
                                                }
                                            }
                                        }
                                    })
                                    .expect("右键菜单所属编辑器不存在");
                            });
                        }),
                );
            }
            menu
        }
    }
}
