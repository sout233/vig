use super::*;
use crate::{
    media::render_settings::{RenderFormat, RenderSettings},
    model::{FrameRate, VideoSettings},
    theme::{AnimatedButton as Button, MaterialStyle},
};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, IndexPath, TitleBar,
        button::ButtonVariants,
        input::Input,
        progress::Progress,
        select::{Select, SelectItem, SelectState},
    },
    prelude::*,
};

#[cfg(all(test, feature = "ui-verify"))]
mod tests;

#[derive(Clone)]
struct Choice {
    value: usize,
    label: SharedString,
}

impl SelectItem for Choice {
    type Value = usize;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &usize {
        &self.value
    }
}

struct RenderWindow {
    owner: WeakEntity<Editor>,
    fields: [Entity<InputState>; 3],
    format: Entity<SelectState<Vec<Choice>>>,
    quality: Entity<SelectState<Vec<Choice>>>,
    picking: bool,
    launched: bool,
    destination: Option<PathBuf>,
    error: Option<String>,
    _subscription: Subscription,
}

impl RenderWindow {
    fn settings(&self, cx: &App) -> Result<RenderSettings, String> {
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
        let fps = self.fields[2].read(cx).value().parse::<FrameRate>()?;
        let format = self
            .format
            .read(cx)
            .selected_value()
            .copied()
            .ok_or("请选择视频格式")?;
        let quality = self
            .quality
            .read(cx)
            .selected_value()
            .copied()
            .ok_or("请选择画质")?;
        Ok(RenderSettings {
            video: VideoSettings::new(width, height, fps)?,
            format: RenderFormat::ALL[format],
            quality,
        })
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = match self.settings(cx) {
            Ok(settings) => settings,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(owner) = self.owner.upgrade() else {
            self.error = Some("工程窗口已关闭，无法开始渲染".into());
            cx.notify();
            return;
        };
        if self.picking || owner.read(cx).busy || owner.read(cx).job == Some(Job::Export) {
            return;
        }
        self.picking = true;
        self.error = None;
        let extension = settings.format.extension();
        let name = format!("{}.{}", owner.read(cx).project.name, extension);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title("渲染到文件")
                .set_file_name(name)
                .add_filter(settings.format.label(), &[extension])
                .save_file()
                .await;
            let _ = this.update(cx, |this, cx| {
                this.picking = false;
                if let Some(file) = file {
                    let path = file.path().to_path_buf();
                    let result = owner.update(cx, |editor, cx| {
                        if editor.busy || editor.job == Some(Job::Export) {
                            return Err("已有任务正在执行".to_string());
                        }
                        if editor.path.as_ref() == Some(&path) {
                            return Err("渲染文件不能覆盖项目文件".into());
                        }
                        if editor.project.media.iter().any(|media| media.path == path) {
                            return Err("渲染文件不能覆盖源素材".into());
                        }
                        editor.start_render(path.clone(), settings, cx);
                        Ok(())
                    });
                    match result {
                        Ok(()) => {
                            this.launched = true;
                            this.destination = Some(path);
                        }
                        Err(error) => this.error = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for RenderWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div()
                .size_full()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child(TitleBar::new().child("渲染"))
                .child(div().p_6().child("工程窗口已关闭。正在执行的渲染已取消。"))
                .into_any_element();
        };
        let editor = owner.read(cx);
        let running = editor.job == Some(Job::Export);
        let disabled = self.picking || running || editor.busy;
        let progress = editor.progress as f32 * 100.;
        let status = editor.status.clone();
        let error = self
            .error
            .clone()
            .or_else(|| self.launched.then(|| editor.error.clone()).flatten());
        let fields = [
            ("render-width", "宽度 / 像素"),
            ("render-height", "高度 / 像素"),
            ("render-fps", "帧率 / FPS"),
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
                        .w(px(220.))
                        .child(Input::new(field).id(id).disabled(disabled).input_material())
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            if !disabled {
                                focus.update(cx, |state, cx| state.focus(window, cx));
                            }
                        }),
                )
        });
        let format = Select::new(&self.format)
            .id("render-format")
            .accessibility_label("视频格式")
            .disabled(disabled)
            .w_full();
        let quality = Select::new(&self.quality)
            .id("render-quality")
            .accessibility_label("画质")
            .disabled(disabled)
            .w_full();
        let buttons = div()
            .flex()
            .justify_end()
            .gap_2()
            .when(running, |view| {
                view.child(
                    Button::new("cancel-render")
                        .label("取消渲染")
                        .button_material(false)
                        .on_click(move |_, _, cx| {
                            owner.update(cx, |editor, cx| {
                                editor.cancel.store(true, Ordering::Relaxed);
                                editor.status = "正在取消渲染…".into();
                                cx.notify();
                            });
                        }),
                )
            })
            .child(
                Button::new("start-render")
                    .label(if self.picking {
                        "选择保存位置…"
                    } else {
                        "渲染到文件…"
                    })
                    .primary()
                    .button_material(true)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
            );
        let body = div()
            .id("render-form")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .p_6()
            .gap_4()
            .child(div().text_lg().child("渲染设置"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("这些参数都是临时的（我没写持久化）"),
            )
            .children(fields)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("可输入 60、29.97 或 30000/1001 之类的（为什么有分数？）"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child("视频格式")
                    .child(div().w(px(280.)).child(format)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child("画质")
                    .child(div().w(px(280.)).child(quality)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("暂不支持设定音频参数"),
            )
            .when_some(self.destination.clone(), |view, path| {
                view.child(div().text_sm().child(format!("输出：{}", path.display())))
            })
            .when(self.launched, |view| {
                view.child(div().text_sm().child(status))
            })
            .when(running, |view| {
                view.child(
                    Progress::new("render-progress")
                        .accessibility_label("渲染进度")
                        .value(progress),
                )
                .child(div().text_sm().child(format!("{progress:.1}%")))
            })
            .when_some(error, |view, error| {
                view.child(
                    div()
                        .id("render-error")
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(buttons);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child("渲染"))
            .child(body)
            .into_any_element()
    }
}

impl Editor {
    pub fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.job == Some(Job::Export) {
            return;
        }
        if self.project.duration() == 0. {
            self.error = Some("时间线为空，无法渲染".into());
            cx.notify();
            return;
        }
        self.close_color_editor(cx);
        let owner = cx.entity();
        let (width, height) = self.project.settings().dimensions();
        let values = [
            width.to_string(),
            height.to_string(),
            self.project.settings().frame_rate().to_string(),
        ];
        window.defer(cx, move |_, cx| {
            let error_owner = owner.clone();
            let result = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(620.), px(660.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(580.), px(640.))),
                    ..TitleBar::window_options()
                },
                cx,
                move |window, cx| {
                    window.set_window_title("Vig · 渲染");
                    cx.new(|cx| {
                        let fields = values.map(|value| {
                            cx.new(|cx| {
                                let mut field = InputState::new(window, cx);
                                field.set_value(value, window, cx);
                                field
                            })
                        });
                        let format = cx.new(|cx| {
                            SelectState::new(
                                RenderFormat::ALL
                                    .into_iter()
                                    .enumerate()
                                    .map(|(value, format)| Choice {
                                        value,
                                        label: format.label().into(),
                                    })
                                    .collect::<Vec<_>>(),
                                Some(IndexPath::new(0)),
                                window,
                                cx,
                            )
                        });
                        let quality = cx.new(|cx| {
                            SelectState::new(
                                ["高画质", "均衡", "较小文件"]
                                    .into_iter()
                                    .enumerate()
                                    .map(|(value, label)| Choice {
                                        value,
                                        label: label.into(),
                                    })
                                    .collect::<Vec<_>>(),
                                Some(IndexPath::new(1)),
                                window,
                                cx,
                            )
                        });
                        RenderWindow {
                            owner: owner.downgrade(),
                            fields,
                            format,
                            quality,
                            picking: false,
                            launched: false,
                            destination: None,
                            error: None,
                            _subscription: cx.observe(&owner, |_, _, cx| cx.notify()),
                        }
                    })
                },
            );
            if let Err(error) = result {
                error_owner.update(cx, |editor, cx| {
                    editor.error = Some(format!("无法打开渲染窗口：{error}"));
                    cx.notify();
                });
            }
        });
    }
}
