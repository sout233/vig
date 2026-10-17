use super::*;
use crate::{
    model::{FrameRate, VideoSettings},
    theme::{AnimatedButton as Button, MaterialStyle},
};
use gpui_kit::{
    component::{ActiveTheme, WindowExt, button::ButtonVariants, input::Input},
    prelude::*,
};

struct ProjectProperties {
    fields: [Entity<InputState>; 3],
    owner: WeakEntity<Editor>,
    error: Option<String>,
}

impl ProjectProperties {
    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let result = (|| {
            let width = self.fields[0]
                .read(cx)
                .value()
                .trim()
                .parse()
                .map_err(|_| "宽度必须是整数")?;
            let height = self.fields[1]
                .read(cx)
                .value()
                .trim()
                .parse()
                .map_err(|_| "高度必须是整数")?;
            let rate = self.fields[2].read(cx).value().parse::<FrameRate>()?;
            VideoSettings::new(width, height, rate)
        })();
        match result {
            Ok(settings) => {
                self.owner
                    .update(cx, |editor, cx| {
                        editor.apply_project_settings(settings, window, cx)
                    })
                    .expect("工程属性所属编辑器不存在");
                true
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                false
            }
        }
    }
}

impl Render for ProjectProperties {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("project-properties-form")
            .flex()
            .flex_col()
            .gap_3()
            .children(
                [
                    ("project-width", "宽度 / 像素"),
                    ("project-height", "高度 / 像素"),
                    ("project-fps", "帧率 / FPS"),
                ]
                .into_iter()
                .zip(&self.fields)
                .map(|((id, label), field)| {
                    let focus = field.clone();
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_4()
                        .child(label)
                        .child(
                            div()
                                .w(px(180.))
                                .child(Input::new(field).id(id).input_material())
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    focus.update(cx, |state, cx| state.focus(window, cx));
                                }),
                        )
                }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("帧率支持整数、小数或分数，例如 60、29.97、30000/1001，宽高须为偶数"),
            )
            .when_some(self.error.clone(), |element, error| {
                element.child(div().text_sm().text_color(cx.theme().danger).child(error))
            })
    }
}

#[cfg(all(test, feature = "ui-verify"))]
mod tests {
    use super::{Editor, VideoSettings};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };

    #[gpui_kit::test]
    fn project_properties_menu_edits_save_cancel_and_undo(cx: &mut TestAppContext) {
        let (handle, editor) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(cx);
            crate::editor::bind_keys(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(1380.), px(900.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Editor::new(window, cx)),
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("file-menu", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.within("popup-menu").click(4usize, cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
        })
        .unwrap();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        for (id, text) in [
            ("project-width", "720"),
            ("project-height", "1280"),
            ("project-fps", "60000/1001"),
        ] {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click(id, cx);
                window.render_frame(cx);
                assert_eq!(
                    window.find(id).focused(),
                    Some(true),
                    "{id} 点击后应获得焦点：{:?}",
                    window.find(id)
                );
            })
            .unwrap();
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.press("ctrl-a", cx);
                for character in text.chars() {
                    window.press(&character.to_string(), cx);
                }
                window.render_frame(cx);
                assert_eq!(window.find(id).value(), Some(text), "{id} 必须收到实际输入");
            })
            .unwrap();
        }
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("save-project-properties", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            let project = &editor.read(cx).project;
            assert_eq!(project.settings().dimensions(), (720, 1280));
            assert_eq!(project.settings().frame_rate().to_string(), "60000/1001");
            assert!(editor.read(cx).dirty);
            assert_eq!(editor.read(cx).preview_cache.dimensions(), (360, 640));
            editor.update(cx, |editor, cx| editor.project_properties(window, cx));
            window.render_frame(cx);
            window.click("project-width", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("ctrl-a", cx);
            for character in "1920".chars() {
                window.press(&character.to_string(), cx);
            }
            window.click("cancel-project-properties", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            assert_eq!(editor.read(cx).project.settings().dimensions(), (720, 1280));
            window.render_frame(cx);
            window.click("undo", cx);
            assert_eq!(
                editor.read(cx).project.settings(),
                &VideoSettings::default()
            );
            assert_eq!(editor.read(cx).preview_cache.dimensions(), (960, 540));
        })
        .unwrap();
    }
}

impl Editor {
    pub fn project_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        self.stop();
        let (width, height) = self.project.settings().dimensions();
        let values = [
            width.to_string(),
            height.to_string(),
            self.project.settings().frame_rate().to_string(),
        ];
        let fields = values.map(|value| {
            cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(value, window, cx);
                state
            })
        });
        let owner = cx.entity().downgrade();
        let form = cx.new(|_| ProjectProperties {
            fields,
            owner,
            error: None,
        });
        window.open_dialog(cx, move |dialog, _, _| {
            let apply = form.clone();
            let confirm = form.clone();
            dialog
                .title("工程属性")
                .overlay_closable(false)
                .child(form.clone())
                .on_ok(move |_, window, cx| apply.update(cx, |form, cx| form.apply(window, cx)))
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("cancel-project-properties")
                                .label("取消")
                                .button_material(false)
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("save-project-properties")
                                .label("保存")
                                .primary()
                                .button_material(true)
                                .on_click(move |_, window, cx| {
                                    if confirm.update(cx, |form, cx| form.apply(window, cx)) {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
        });
        cx.notify();
    }

    fn apply_project_settings(
        &mut self,
        settings: VideoSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project.settings() == &settings {
            return;
        }
        self.checkpoint();
        self.project
            .set_settings(settings)
            .expect("已经验证的工程属性无效");
        self.changed(window, cx);
    }

    pub fn offer_first_video_settings(
        &mut self,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = path else {
            return;
        };
        self.busy = true;
        self.stop();
        cx.notify();
        let dialog = rfd::AsyncMessageDialog::new().set_parent(window);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move {
                let source = media::probe(&path)?;
                let rate = source.frame_rate.ok_or("素材没有视频帧率")?;
                let settings = VideoSettings::new(source.width, source.height, rate)?;
                Ok::<_, String>((source.display_name(), settings))
            }).await;
            match result {
                Ok((name, settings)) => {
                    let (width, height) = settings.dimensions();
                    let use_source = dialog.set_title("同步属性")
                        .set_description(format!("是否将工程属性设置为视频“{name}”的属性？\n\n分辨率：{width}×{height}\n帧率：{} FPS\n\n选择“否”将保留当前工程属性", settings.frame_rate()))
                        .set_buttons(rfd::MessageButtons::YesNo).show().await == rfd::MessageDialogResult::Yes;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.busy = false;
                        if use_source { this.apply_project_settings(settings, window, cx); }
                        cx.notify();
                    });
                }
                Err(error) => { let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    this.error = Some(format!("读取首个视频的工程属性失败：{error}"));
                    cx.notify();
                }); }
            }
        }).detach();
    }
}
