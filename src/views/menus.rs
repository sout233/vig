use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

#[derive(Clone, Copy)]
enum Command {
    New,
    Open,
    Import,
    Save,
    Export,
    ProjectProperties,
    #[cfg(feature = "asr")]
    AutoSubtitles,
    Undo,
    Redo,
    Copy,
    Paste,
    Split,
    Delete,
    Ripple,
    AddVideoTrack,
    AddAudioTrack,
    AddSubtitle,
    AddSolid,
    DeleteTrack,
    TrackUp,
    TrackDown,
    Play,
    Previous,
    Next,
    Start,
    Snap,
    ZoomIn,
    ZoomOut,
    Help,
    About,
}

impl Command {
    fn action(self) -> Option<Box<dyn Action>> {
        Some(match self {
            Self::New => Box::new(NewProject),
            Self::Open => Box::new(Open),
            Self::Import => Box::new(Import),
            Self::Save => Box::new(Save),
            Self::Undo => Box::new(Undo),
            Self::Redo => Box::new(Redo),
            Self::Copy => Box::new(CopyClips),
            Self::Paste => Box::new(PasteClips),
            Self::Split => Box::new(Split),
            Self::Delete => Box::new(Delete),
            Self::Play => Box::new(Play),
            _ => return None,
        })
    }
}

impl Editor {
    pub(super) fn menu_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.job == Some(Job::Export);
        let selected = self.selected.is_some();
        let playing = self.job == Some(Job::Play);
        let menus = [
            (
                "file-menu",
                "文件",
                vec![
                    ("新建项目", Command::New, locked),
                    ("打开项目…", Command::Open, locked),
                    ("导入素材…", Command::Import, locked),
                    ("保存项目", Command::Save, self.busy),
                    ("工程属性…", Command::ProjectProperties, locked),
                    (
                        "渲染…",
                        Command::Export,
                        self.busy || self.job.is_some() || self.project.duration() == 0.,
                    ),
                ],
            ),
            (
                "edit-menu",
                "编辑",
                vec![
                    ("添加文本素材", Command::AddSubtitle, locked),
                    ("添加纯色素材", Command::AddSolid, locked),
                    ("复制片段", Command::Copy, locked || !selected),
                    ("粘贴片段", Command::Paste, locked),
                    ("撤销", Command::Undo, locked || self.undo.is_empty()),
                    ("重做", Command::Redo, locked || self.redo.is_empty()),
                    ("在播放头处分割", Command::Split, locked || !selected),
                    ("删除片段", Command::Delete, locked || !selected),
                    (
                        "删除并闭合空隙",
                        Command::Ripple,
                        locked || self.selected.is_none(),
                    ),
                ],
            ),
            (
                "track-menu",
                "轨道",
                vec![
                    ("添加视频轨", Command::AddVideoTrack, locked),
                    ("添加音频轨", Command::AddAudioTrack, locked),
                    (
                        "删除所选轨道及其中片段",
                        Command::DeleteTrack,
                        locked || self.selected_track.is_none(),
                    ),
                    (
                        "上移所选轨道",
                        Command::TrackUp,
                        locked
                            || self.selected_track.is_none()
                            || self.project.tracks.first().map(|t| t.id) == self.selected_track,
                    ),
                    (
                        "下移所选轨道",
                        Command::TrackDown,
                        locked
                            || self.selected_track.is_none()
                            || self.project.tracks.last().map(|t| t.id) == self.selected_track,
                    ),
                ],
            ),
            (
                "play-menu",
                "播放",
                vec![
                    (
                        if playing { "暂停" } else { "播放" },
                        Command::Play,
                        locked || self.project.duration() == 0.,
                    ),
                    ("上一帧", Command::Previous, self.busy || self.job.is_some()),
                    ("下一帧", Command::Next, self.busy || self.job.is_some()),
                    ("回到序列起点", Command::Start, locked),
                ],
            ),
            (
                "view-menu",
                "视图",
                vec![
                    (
                        if self.snap {
                            "✓ 时间线吸附"
                        } else {
                            "时间线吸附"
                        },
                        Command::Snap,
                        false,
                    ),
                    ("放大时间线", Command::ZoomIn, false),
                    ("缩小时间线", Command::ZoomOut, false),
                ],
            ),
            #[cfg(feature = "asr")]
            (
                "tools-menu",
                "工具",
                vec![("自动生成字幕…", Command::AutoSubtitles, locked)],
            ),
            (
                "help-menu",
                "帮助",
                vec![
                    ("快捷键与操作说明", Command::Help, false),
                    ("关于 Vig", Command::About, false),
                ],
            ),
        ];
        div()
            .flex()
            .items_center()
            .px_4()
            .h(px(32.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(BORDER))
            .panel_material()
            .children(menus.into_iter().map(|(id, label, items)| {
                let owner = cx.entity().downgrade();
                let focus = self.focus.clone();

                Button::new(id)
                    .label(label)
                    .ghost()
                    .dropdown_menu(move |mut menu, _, _| {
                        menu = menu.action_context(focus.clone());

                        for (label, command, disabled) in &items {
                            let owner = owner.clone();
                            let command = *command;

                            menu = menu.item(
                                PopupMenuItem::new(*label)
                                    .when_some(command.action(), |item, action| item.action(action))
                                    .disabled(*disabled)
                                    .on_click(move |_, window, cx| {
                                        let owner = owner.clone();

                                        window.defer(cx, move |window, cx| {
                                            owner
                                                .update(cx, |this, cx| {
                                                    this.focus.focus(window, cx);

                                                    match command {
                                                        Command::New => this.new_project(window, cx),
                                                        Command::Open => this.open(window, cx),
                                                        Command::Import => this.import(window, cx),
                                                        Command::Save => this.save(window, cx),
                                                        Command::Export => this.export(window, cx),
                                                        Command::ProjectProperties => {
                                                            this.project_properties(window, cx)
                                                        }
                                                        #[cfg(feature = "asr")]
                                                        Command::AutoSubtitles => {
                                                            this.auto_subtitles(window, cx)
                                                        }
                                                        Command::Undo => this.history(false, window, cx),
                                                        Command::Redo => this.history(true, window, cx),
                                                        Command::Copy => this.copy_selection(cx),
                                                        Command::Paste => this.paste_selection(window, cx),
                                                        Command::AddSubtitle => this.add_subtitle(window, cx),
                                                        Command::AddSolid => this.add_solid(window, cx),
                                                        Command::Split => this.split(window, cx),
                                                        Command::Delete => this.delete(false, window, cx),
                                                        Command::Ripple => this.delete(true, window, cx),
                                                        Command::AddVideoTrack => {
                                                            this.add_track(crate::model::TrackKind::Video, window, cx)
                                                        }
                                                        Command::AddAudioTrack => {
                                                            this.add_track(crate::model::TrackKind::Audio, window, cx)
                                                        }
                                                        Command::DeleteTrack => {
                                                            if let Some(id) = this.selected_track {
                                                                this.delete_track(id, window, cx);
                                                            }
                                                        }
                                                        Command::TrackUp | Command::TrackDown => {
                                                            if let Some((index, id)) = this.selected_track
                                                                .and_then(|id| this.project.tracks.iter().position(|t| t.id == id))
                                                                .zip(this.selected_track)
                                                            {
                                                                let target = if matches!(command, Command::TrackUp) {
                                                                    index - 1
                                                                } else {
                                                                    index + 1
                                                                };

                                                                this.reorder_track(id, target, window, cx);
                                                            }
                                                        }
                                                        Command::Play => this.play(cx),
                                                        Command::Previous => {
                                                            this.seek(this.position - this.project.settings().frame_rate().frame_duration(), cx)
                                                        }
                                                        Command::Next => {
                                                            this.seek(this.position + this.project.settings().frame_rate().frame_duration(), cx)
                                                        }
                                                        Command::Start => this.seek(0., cx),
                                                        Command::Snap => {
                                                            this.snap = !this.snap;
                                                            cx.notify();
                                                        }
                                                        Command::ZoomIn => this.zoom_timeline(1.5, cx),
                                                        Command::ZoomOut => this.zoom_timeline(1. / 1.5, cx),
                                                        Command::Help | Command::About => {
                                                            let (title, description) = if matches!(command, Command::About) {
                                                                let edition = if cfg!(feature = "asr") { "完整版（含自动字幕）" } else { "精简版（不含自动字幕）" };
                                                                ("关于 Vig", format!("Vig 0.1.0 · {edition}\nPowered By GPUI-Kit\n\nBoxicons · MIT License\n{}", include_str!("../../assets/icons/LICENSE")))
                                                            } else {
                                                                ("操作说明", "idk".to_owned())
                                                            };

                                                            let dialog = rfd::AsyncMessageDialog::new().set_parent(window);
                                                            cx.spawn(async move |_, _| {
                                                                dialog
                                                                    .set_title(title)
                                                                    .set_description(description)
                                                                    .show()
                                                                    .await;
                                                            })
                                                            .detach();
                                                        }
                                                    }
                                                })
                                                .expect("菜单所属编辑器不存在");
                                        });
                                    }),
                            );
                        }

                        menu
                    })
            }))
    }
}
