use super::*;
use gpui_kit::component::{
    WindowExt,
    menu::{DropdownMenu, PopupMenuItem},
};

#[cfg(all(test, feature = "ui-verify"))]
mod tests;

#[derive(Clone, Copy)]
enum PreviewCommand {
    Properties,
    Copy,
    ProjectProperties,
}

impl Editor {
    pub(super) fn preview_menu_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let locked = self.busy || self.job == Some(Job::Export);

        Button::new("preview-menu")
            .icon(IconName::Menu)
            .accessibility_label("预览菜单")
            .tooltip("预览菜单")
            .compact()
            .ghost()
            .dropdown_menu(move |mut menu, _, _| {
                for (label, command, disabled) in [
                    ("属性…", PreviewCommand::Properties, false),
                    ("复制属性", PreviewCommand::Copy, false),
                    ("工程属性…", PreviewCommand::ProjectProperties, locked),
                ] {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).disabled(disabled).on_click(
                        move |_, window, cx| {
                            let owner = owner.clone();
                            window.defer(cx, move |window, cx| {
                                owner
                                    .update(cx, |editor, cx| match command {
                                        PreviewCommand::Properties => {
                                            editor.show_preview_properties(window, cx)
                                        }
                                        PreviewCommand::ProjectProperties => {
                                            editor.project_properties(window, cx)
                                        }
                                        PreviewCommand::Copy => {
                                            let text = editor
                                                .preview_property_rows()
                                                .into_iter()
                                                .map(|(_, label, value)| {
                                                    format!("{label}：{value}")
                                                })
                                                .collect::<Vec<_>>()
                                                .join("\n");
                                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                                            editor.status = "预览属性已复制".into();
                                            cx.notify();
                                        }
                                    })
                                    .expect("预览菜单所属编辑器不存在");
                            });
                        },
                    ));
                }
                menu
            })
    }

    fn preview_property_rows(&self) -> [(&'static str, &'static str, String); 5] {
        let settings = self.project.settings();
        let (width, height) = settings.dimensions();
        let (preview_width, preview_height) = self.preview_resolution().dimensions(settings);
        let rate = settings.frame_rate();
        let mut fps = format!("{:.3} FPS", rate.fps());
        if rate.to_string().contains('/') {
            fps.push_str(&format!(" ({rate})"));
        }
        let (mut a, mut b) = (width, height);
        while b != 0 {
            (a, b) = (b, a % b);
        }

        [
            (
                "preview-project-size",
                "工程分辨率",
                format!("{width} × {height}"),
            ),
            (
                "preview-render-size",
                "预览分辨率",
                format!(
                    "{preview_width} × {preview_height} ({})",
                    self.preview_resolution().label()
                ),
            ),
            ("preview-frame-rate", "帧率", fps),
            (
                "preview-aspect-ratio",
                "画面比例",
                format!("{}:{}", width / a, height / a),
            ),
            (
                "preview-duration",
                "时间线时长",
                timecode(self.project.duration(), rate),
            ),
        ]
    }

    fn show_preview_properties(&self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.preview_property_rows();
        window.open_dialog(cx, move |dialog, _, cx| {
            dialog
                .title("预览属性")
                .child(
                    div()
                        .id("preview-properties")
                        .test_support()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(rows.iter().map(|(id, label, value)| {
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_6()
                                .child(div().text_color(cx.theme().muted_foreground).child(*label))
                                .child(div().id(*id).test_support().child(value.clone()))
                        })),
                )
                .footer(
                    div().flex().justify_end().child(
                        Button::new("close-preview-properties")
                            .label("关闭")
                            .button_material(false)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
                )
        });
    }
}
