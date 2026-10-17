use crate::theme::*;
mod preview_menu;
mod properties;
use crate::theme::AnimatedButton as Button;
use crate::{editor::*, model::timecode};
use gpui_kit::assets::IconName;
use gpui_kit::base::Disableable;
use gpui_kit::{
    component::{
        ActiveTheme, Selectable, TitleBar,
        button::ButtonVariants,
        input::Input,
        resizable::{h_resizable, resizable_panel, v_resizable},
    },
    prelude::*,
    *,
};

#[derive(Clone)]
struct MediaDrag {
    index: usize,
    name: String,
}

impl Render for MediaDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .bg(rgb(VIDEO))
            .border_1()
            .border_color(rgb(ACCENT))
            .text_color(rgb(INK))
            .child(self.name.clone())
    }
}

impl Editor {
    fn preview_resolution_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::media::preview::PreviewResolution;
        use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
        let resolution = self.preview_resolution();
        let settings = self.project.settings().clone();
        let owner = cx.entity().downgrade();
        Button::new("preview-resolution")
            .label(resolution.label())
            .accessibility_label("预览分辨率")
            .tooltip("预览分辨率")
            .compact()
            .ghost()
            .disabled(self.job == Some(Job::Export))
            .dropdown_menu(move |mut menu, _, _| {
                for choice in [
                    PreviewResolution::Full,
                    PreviewResolution::Half,
                    PreviewResolution::Quarter,
                ] {
                    let owner = owner.clone();
                    let (width, height) = choice.dimensions(&settings);
                    menu = menu.item(
                        PopupMenuItem::new(format!("{}  ·  {width}×{height}", choice.label()))
                            .checked(choice == resolution)
                            .on_click(move |_, window, cx| {
                                let owner = owner.clone();
                                window.defer(cx, move |_, cx| {
                                    owner
                                        .update(cx, |this, cx| {
                                            this.set_preview_resolution(choice, cx)
                                        })
                                        .expect("预览分辨率菜单所属编辑器不存在");
                                });
                            }),
                    );
                }
                menu
            })
    }

    fn library(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query.to_lowercase();
        div()
            .id("library-panel")
            .test_support()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .p_4()
            .gap_4()
            .bg(rgb(SURFACE))
            .panel_material()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("素材库"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("{} 个素材", self.project.media.len())),
                    ),
            )
            .child(
                Button::new("import")
                    .button_material(false)
                    .label("+ 导入视频 / 音频")
                    .w_full()
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, w, cx| this.import(w, cx))),
            )
            .child(Input::new(&self.search).input_material())
            .child(
                div()
                    .id("media-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .children(
                        self.project
                            .media
                            .iter()
                            .enumerate()
                            .filter(|(_, m)| m.name.to_lowercase().contains(&query))
                            .map(|(i, m)| {
                                div()
                                    .id(("asset", i))
                                    .test_support()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .p_2()
                                    .rounded_lg()
                                    .bg(rgb(BACKGROUND))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .clip_material(false)
                                    .when(!self.busy && self.job != Some(Job::Export), |d| {
                                        d.on_drag(
                                            MediaDrag {
                                                index: i,
                                                name: m.display_name(),
                                            },
                                            |drag, _, _, cx| cx.new(|_| drag.clone()),
                                        )
                                    })
                                    .child(
                                        div()
                                            .h(px(105.))
                                            .rounded_md()
                                            .overflow_hidden()
                                            .bg(rgb(MONITOR))
                                            .border_1()
                                            .border_color(rgb(BORDER))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when_some(
                                                self.thumbnails.get(&m.path).cloned(),
                                                |d, image| {
                                                    d.child(
                                                        img(image)
                                                            .size_full()
                                                            .object_fit(ObjectFit::Contain),
                                                    )
                                                },
                                            )
                                            .when_some(m.generator.as_ref(), |d, generator| {
                                                match generator {
                                                    crate::model::Generator::Text(_) => {
                                                        d.child(div().text_2xl().child("T"))
                                                    }
                                                    crate::model::Generator::Solid { color } => {
                                                        let color = u32::from_str_radix(color, 16)
                                                            .expect("已验证的素材颜色无效");
                                                        d.bg(rgb(color))
                                                    }
                                                }
                                            })
                                            .when(m.width == 0, |d| {
                                                d.child(
                                                    div()
                                                        .text_2xl()
                                                        .text_color(rgb(ACCENT))
                                                        .child("♫  AUDIO"),
                                                )
                                            }),
                                    )
                                    .child(
                                        div().text_sm().overflow_hidden().child(m.display_name()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .justify_between()
                                            .items_center()
                                            .child(div().text_xs().text_color(rgb(MUTED)).child(
                                                if m.width > 0 {
                                                    format!(
                                                        "{} × {} · {:.1}s",
                                                        m.width, m.height, m.duration
                                                    )
                                                } else {
                                                    format!("音频 · {:.1}s", m.duration)
                                                },
                                            ))
                                            .child(
                                                Button::new(("add", i))
                                                    .label("+")
                                                    .ghost()
                                                    .disabled(
                                                        self.busy || self.job == Some(Job::Export),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, w, cx| this.add(i, w, cx),
                                                    )),
                                            ),
                                    )
                            }),
                    ),
            )
    }

    fn monitor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (play_icon, play_label) = match self.job {
            Some(Job::Play) => (IconName::Pause, "暂停"),
            _ => (IconName::Play, "播放"),
        };
        div()
            .id("monitor-panel")
            .test_support()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .p_5()
            .gap_4()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("序列预览"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.preview_resolution_button(cx))
                            .child(self.preview_menu_button(cx)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .bg(rgb(MONITOR))
                    .rounded_lg()
                    .recessed_material()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(self.image.clone(), |d, image| {
                        d.child(img(image).size_full().object_fit(ObjectFit::Contain))
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_family("Consolas")
                            .text_color(rgb(ACCENT))
                            .child(timecode(
                                self.position,
                                self.project.settings().frame_rate(),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("back-frame")
                                    .label("‹")
                                    .ghost()
                                    .disabled(self.busy || self.job.is_some())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.seek(
                                            this.position
                                                - this
                                                    .project
                                                    .settings()
                                                    .frame_rate()
                                                    .frame_duration(),
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new("play")
                                    .button_material(true)
                                    .icon(play_icon)
                                    .accessibility_label(play_label)
                                    .tooltip(format!("{play_label}（Space）"))
                                    .primary()
                                    .disabled(self.busy || self.job == Some(Job::Export))
                                    .on_click(cx.listener(|this, _, _, cx| this.play(cx))),
                            )
                            .child(
                                Button::new("forward-frame")
                                    .label("›")
                                    .ghost()
                                    .disabled(self.busy || self.job.is_some())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.seek(
                                            this.position
                                                + this
                                                    .project
                                                    .settings()
                                                    .frame_rate()
                                                    .frame_duration(),
                                            cx,
                                        )
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_family("Consolas")
                            .text_color(rgb(MUTED))
                            .child(timecode(
                                self.project.duration(),
                                self.project.settings().frame_rate(),
                            )),
                    ),
            )
    }

    pub(super) fn clip_properties(&self, cx: &mut Context<Self>) -> AnyElement {
        let clip = self.selected.and_then(|id| self.project.clip(id));
        let audio = self
            .selected
            .and_then(|id| self.project.clip_track(id))
            .is_some_and(|t| t.kind == crate::model::TrackKind::Audio);
        div()
            .id("clip-properties")
            .test_support()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .p_0()
            .gap_3()
            .when_some(clip, |d, c| {
                d.child(
                    div()
                        .text_sm()
                        .text_color(rgb(ACCENT))
                        .child(self.project.media[c.media].display_name()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!("片段时长 {:.3}s", c.duration())),
                )
                .children(
                    [
                        "时间线起点 / 秒",
                        "源入点 / 秒",
                        "源出点 / 秒",
                        "速度 / 0.25–4 倍",
                        "音量 / 0–200%",
                        "淡入 / 秒",
                        "淡出 / 秒",
                    ]
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != 4 || audio)
                    .map(|(i, label)| {
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(div().text_xs().child(*label))
                            .child(
                                div().w(px(87.)).child(
                                    Input::new(&self.fields[i])
                                        .input_material()
                                        .disabled(self.busy || self.job == Some(Job::Export)),
                                ),
                            )
                    }),
                )
                .when(audio, |d| {
                    d.child(
                        Button::new("mute")
                            .button_material(false)
                            .label(if c.muted {
                                "恢复片段声音"
                            } else {
                                "静音片段"
                            })
                            .disabled(self.busy || self.job == Some(Job::Export))
                            .on_click(cx.listener(|this, _, w, cx| {
                                if let Some(id) = this.selected {
                                    this.checkpoint();
                                    if let Some(c) = this.project.clip_mut(id) {
                                        c.muted = !c.muted;
                                    }
                                    this.changed(w, cx);
                                }
                            })),
                    )
                })
            })
            .into_any_element()
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("editor-drop-zone")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(INK))
            .text_sm()
            .key_context("Vig")
            .track_focus(&self.focus)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.track_label_resize.is_some() {
                    this.resize_track_labels(event.position.x, cx);
                } else if this.track_resize.is_some() {
                    this.resize_track(event.position.y, cx);
                } else if this.number_drag.is_some() {
                    this.move_number(event.modifiers.shift, window, cx);
                } else {
                    this.move_timeline_pointer(event.position, cx);
                }
            }))
            .capture_any_mouse_up(cx.listener(Self::finish_pointer_drag))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::finish_pointer_drag))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.border_2().border_color(rgb(ACCENT)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.import_media(Some(paths.paths().to_vec()), true, None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Import, w, cx| this.import(w, cx)))
            .on_action(cx.listener(|this, _: &Open, w, cx| this.open(w, cx)))
            .on_action(cx.listener(|this, _: &NewProject, w, cx| {
                if this.job != Some(Job::Export) {
                    this.new_project(w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Save, w, cx| this.save(w, cx)))
            .on_action(cx.listener(|this, _: &CopyClips, _, cx| this.copy_selection(cx)))
            .on_action(cx.listener(|this, _: &PasteClips, w, cx| this.paste_selection(w, cx)))
            .on_action(cx.listener(|this, _: &Play, _, cx| {
                if !this.busy && this.job != Some(Job::Export) {
                    this.play(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Split, w, cx| {
                if !this.busy && this.job != Some(Job::Export) {
                    this.split(w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Delete, w, cx| {
                if !this.busy && this.job != Some(Job::Export) {
                    this.delete(false, w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Undo, w, cx| {
                if !this.busy && this.job != Some(Job::Export) {
                    this.history(false, w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Redo, w, cx| {
                if !this.busy && this.job != Some(Job::Export) {
                    this.history(true, w, cx);
                }
            }))
            .child(TitleBar::new().child(div().text_xs().child("Vig")))
            .child(self.menu_bar(cx))
            .child(
                div().flex().flex_1().min_h_0().min_w_0().child(
                    v_resizable("workspace-timeline-split")
                        .with_state(&self.timeline_split)
                        .child(
                            resizable_panel().child(
                                h_resizable("workspace-columns")
                                    .with_state(&self.workspace_split)
                                    .child(
                                        resizable_panel()
                                            .size(px(252.))
                                            .flex_none()
                                            .child(self.library(cx)),
                                    )
                                    .child(resizable_panel().child(self.monitor(cx)))
                                    .child(
                                        resizable_panel()
                                            .size(px(250.))
                                            .flex_none()
                                            .child(self.inspector(cx)),
                                    ),
                            ),
                        )
                        .child(
                            resizable_panel()
                                .size(px(280.))
                                .flex_none()
                                .child(self.timeline(cx)),
                        ),
                ),
            )
            .when(self.job == Some(Job::Export), |d| {
                d.child(
                    div().h(px(3.)).w_full().bg(rgb(BORDER)).child(
                        div()
                            .h_full()
                            .w(relative(self.progress as f32))
                            .bg(rgb(ACCENT)),
                    ),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_5()
                    .py_2()
                    .bg(rgb(SURFACE))
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(rgb(if self.error.is_some() { ERROR } else { MUTED }))
                            .child(self.error.clone().unwrap_or_else(|| self.status.clone())),
                    )
                    .when(self.job == Some(Job::Export), |view| {
                        view.child(
                            Button::new("cancel-render-from-editor")
                                .label("取消渲染")
                                .button_material(false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel
                                        .store(true, std::sync::atomic::Ordering::Relaxed);
                                    this.status = "正在取消渲染…".into();
                                    cx.notify();
                                })),
                        )
                    }),
            )
    }
}
mod menus;
mod timeline;
