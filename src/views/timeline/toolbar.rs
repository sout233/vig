use super::*;

impl Editor {
    // Keep button construction in separate frames: debug builders hold large temporaries.
    pub(super) fn timeline_creation_buttons(&self, cx: &mut Context<Self>) -> [AnyElement; 3] {
        let locked = self.busy || self.job == Some(Job::Export);
        let owner = cx.entity().downgrade();
        [
            Button::new("add-video-track")
                .icon(IconName::Video)
                .accessibility_label("添加视频轨")
                .tooltip("添加视频轨")
                .ghost()
                .disabled(locked)
                .on_click(
                    cx.listener(|this, _, window, cx| this.add_track(TrackKind::Video, window, cx)),
                )
                .into_any_element(),
            Button::new("add-generator")
                .icon(gpui_kit::component::Icon::default().data(include_bytes!(
                    "../../../assets/icons/bxs--category-alt.svg"
                )))
                .accessibility_label("生成式素材")
                .tooltip("生成式素材")
                .ghost()
                .disabled(locked)
                .dropdown_menu(move |mut menu, _, _| {
                    for (label, solid) in [("文本", false), ("纯色", true)] {
                        let owner = owner.clone();
                        menu =
                            menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                let owner = owner.clone();
                                window.defer(cx, move |window, cx| {
                                    owner
                                        .update(cx, |this, cx| {
                                            if solid {
                                                this.add_solid(window, cx);
                                            } else {
                                                this.add_subtitle(window, cx);
                                            }
                                        })
                                        .expect("生成式素材菜单所属编辑器不存在");
                                });
                            }));
                    }
                    menu
                })
                .into_any_element(),
            Button::new("add-audio-track")
                .icon(IconName::Music)
                .accessibility_label("添加音频轨")
                .tooltip("添加音频轨")
                .ghost()
                .disabled(locked)
                .on_click(
                    cx.listener(|this, _, window, cx| this.add_track(TrackKind::Audio, window, cx)),
                )
                .into_any_element(),
        ]
    }

    pub(super) fn timeline_edit_buttons(&self, cx: &mut Context<Self>) -> [AnyElement; 5] {
        let locked = self.busy || self.job == Some(Job::Export);
        [
            Button::new("undo")
                .icon(IconName::Undo2)
                .accessibility_label("撤销")
                .tooltip("撤销（Ctrl+Z）")
                .ghost()
                .disabled(locked || self.undo.is_empty())
                .on_click(cx.listener(|this, _, window, cx| this.history(false, window, cx)))
                .into_any_element(),
            Button::new("redo")
                .icon(IconName::Redo2)
                .accessibility_label("重做")
                .tooltip("重做（Ctrl+Shift+Z）")
                .ghost()
                .disabled(locked || self.redo.is_empty())
                .on_click(cx.listener(|this, _, window, cx| this.history(true, window, cx)))
                .into_any_element(),
            Button::new("split")
                .icon(IconName::Scissors)
                .accessibility_label("分割")
                .tooltip("在播放头处分割（S）")
                .ghost()
                .disabled(locked || self.selected.is_none())
                .on_click(cx.listener(|this, _, window, cx| this.split(window, cx)))
                .into_any_element(),
            Button::new("delete")
                .icon(IconName::Trash)
                .accessibility_label("删除片段")
                .tooltip("删除片段（Delete）")
                .ghost()
                .disabled(locked || self.selected.is_none())
                .on_click(cx.listener(|this, _, window, cx| this.delete(false, window, cx)))
                .into_any_element(),
            Button::new("ripple")
                .icon(IconName::FoldHorizontal)
                .accessibility_label("删除并闭合空隙")
                .tooltip("删除片段并闭合空隙")
                .ghost()
                .disabled(locked || self.selected.is_none())
                .on_click(cx.listener(|this, _, window, cx| this.delete(true, window, cx)))
                .into_any_element(),
        ]
    }

    pub(super) fn timeline_navigation_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .gap_2()
            .items_center()
            .child(
                Button::new("snap")
                    .icon(IconName::Magnet)
                    .accessibility_label("时间线吸附")
                    .tooltip(if self.snap {
                        "关闭时间线吸附"
                    } else {
                        "开启时间线吸附"
                    })
                    .selected(self.snap)
                    .text_color(rgb(if self.snap { ACCENT } else { MUTED }))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.snap = !this.snap;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("zoom-out")
                    .label("−")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_timeline(1. / 1.5, cx))),
            )
            .child(div().text_xs().child(format!("{:.0} px/s", self.zoom)))
            .child(
                Button::new("zoom-in")
                    .label("+")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_timeline(1.5, cx))),
            )
            .into_any_element()
    }
}
