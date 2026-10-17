use super::*;
use gpui_kit::component::{
    color_picker::ColorPicker,
    input::Textarea,
    tab::{Tab, TabBar},
};

impl Editor {
    pub(super) fn inspector(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected.and_then(|id| self.project.clip(id));
        div()
            .id("inspector")
            .test_support()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            .panel_material()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("属性"))
            .child(
                TabBar::new("inspector-tabs")
                    .underline()
                    .selected_index(usize::from(self.transform_tab))
                    .child(Tab::new().label("属性"))
                    .child(Tab::new().label("变换"))
                    .on_click(cx.listener(|this, index: &usize, window, cx| {
                        if this.number_edit.is_some() {
                            this.apply_number(this.number_edit.unwrap(), window, cx);
                            if this.error.is_some() {
                                return;
                            }
                        }
                        this.number_edit = None;
                        this.transform_tab = *index == 1;
                        cx.notify();
                    })),
            )
            .when_some(selected, |d, clip| {
                let body = if self.transform_tab {
                    self.transform_properties(cx)
                } else if self.project.media[clip.media].generator.is_some() {
                    self.generator_properties(cx)
                } else {
                    self.clip_properties(cx)
                };
                d.child(
                    div()
                        .id("inspector-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body),
                )
            })
            .into_any_element()
    }

    fn property_number(&self, index: usize, label: &str, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.busy || self.job == Some(Job::Export);
        let value = self.property_values(self.selected.unwrap())[index];
        let unit = match index {
            0 | 1 => "秒",
            3 | 4 | 5 | 7 => "%",
            6 => "°",
            _ => "",
        };
        let control = if self.number_edit == Some(index) {
            Input::new(&self.number_fields[index])
                .disabled(locked)
                .input_material()
                .when(!unit.is_empty(), |input| {
                    input.suffix(div().text_xs().child(unit))
                })
                .into_any_element()
        } else {
            div()
                .id(("property-number", index))
                .h_8()
                .px_2()
                .flex()
                .items_center()
                .justify_end()
                .child(format!("{value:.3}{unit}"))
                .when(!locked, |d| {
                    d.cursor(CursorStyle::ResizeLeftRight)
                        .hover(|d| d.underline())
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.begin_number(index, event, window, cx);
                    }),
                )
                .into_any_element()
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(div().text_xs().child(label.to_owned()))
            .child(
                div()
                    .w(px(if unit.is_empty() { 87. } else { 104. }))
                    .flex_shrink_0()
                    .child(control),
            )
            .into_any_element()
    }

    fn generator_properties(&self, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.busy || self.job == Some(Job::Export);
        let generator = self.project.media
            [self.project.clip(self.selected.unwrap()).unwrap().media]
            .generator
            .as_ref()
            .unwrap();
        let text = generator.as_text().is_some();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .when(text, |d| {
                d.child(
                    Textarea::new(&self.subtitle_text)
                        .h(px(120.))
                        .w_full()
                        .disabled(locked)
                        .input_material(),
                )
            })
            .children(
                ["起点", "显示时长", "字号（1080p）"]
                    .into_iter()
                    .enumerate()
                    .filter(|(index, _)| *index != 2 || text)
                    .map(|(index, label)| self.property_number(index, label, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .child(if text { "文字颜色" } else { "颜色" }),
                    )
                    .child(if locked {
                        Button::new("generator-color-readonly")
                            .label("选择颜色")
                            .disabled(true)
                            .into_any_element()
                    } else {
                        ColorPicker::new(&self.generator_color)
                            .label("选择颜色")
                            .accessibility_label(if text {
                                "文字颜色"
                            } else {
                                "纯色素材颜色"
                            })
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn transform_properties(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(
                ["X 轴位置", "Y 轴位置", "缩放", "旋转", "不透明度"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, label)| self.property_number(index + 3, label, cx)),
            )
            .into_any_element()
    }
}
