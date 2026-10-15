use crate::theme::{AnimatedButton, MaterialStyle};
use gpui_kit::base::Disableable;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Context, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, WindowBounds, WindowOptions, div, point, px, size,
};

struct Buttons {
    clicks: usize,
    disabled: bool,
}

impl Render for Buttons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        div()
            .p_4()
            .child(
                AnimatedButton::new("animated-button")
                    .label("按钮")
                    .button_material(false)
                    .tooltip("提示")
                    .disabled(self.disabled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clicks += 1;
                        cx.notify();
                    })),
            )
            .child(
                AnimatedButton::new("animated-menu")
                    .label("菜单")
                    .ghost()
                    .dropdown_menu(move |menu, _, _| {
                        let owner = owner.clone();
                        menu.item(PopupMenuItem::new("执行").on_click(move |_, window, cx| {
                            let owner = owner.clone();
                            window.defer(cx, move |_, cx| {
                                owner
                                    .update(cx, |this, cx| {
                                        this.clicks += 1;
                                        cx.notify();
                                    })
                                    .unwrap();
                            });
                        }))
                    }),
            )
    }
}

#[gpui_kit::test]
fn hover_preserves_the_animated_gradient_surface(cx: &mut TestAppContext) {
    let (handle, _) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::theme::apply(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| Buttons {
                clicks: 0,
                disabled: false,
            })
        })
        .unwrap()
    });
    let initial = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window
                .find("animated-button")
                .bounds()
                .scale(window.scale_factor());
            let initial = window
                .painted_quads()
                .into_iter()
                .find(|quad| quad.bounds == bounds && !quad.background.is_transparent())
                .expect("正常按钮应绘制渐变背景");
            assert!(format!("{:?}", initial.background).starts_with("LinearGradient"));
            window.hover("animated-button", cx);
            initial
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let hovered = window
            .painted_quads()
            .into_iter()
            .find(|quad| quad.bounds == initial.bounds && !quad.background.is_transparent())
            .expect("hover 按钮应绘制背景");
        assert_eq!(
            hovered.background, initial.background,
            "hover 首帧不能将动画渐变替换为纯色背景"
        );
        assert_eq!(
            hovered.corner_radii, initial.corner_radii,
            "hover 不应改变圆角"
        );
    })
    .unwrap();
    for milliseconds in [1, 15, 64, 80] {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(milliseconds));
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let background = window
                .painted_quads()
                .into_iter()
                .find(|quad| quad.bounds == initial.bounds && !quad.background.is_transparent())
                .unwrap()
                .background;
            assert!(
                format!("{background:?}").starts_with("LinearGradient"),
                "过渡过程中不能突然切换为纯色背景"
            );
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        let hover_color = cx.theme().button_hover;
        let expected = gpui_kit::linear_gradient(
            180.,
            gpui_kit::linear_color_stop(hover_color, 0.),
            gpui_kit::linear_color_stop(hover_color, 1.),
        );
        let actual = window
            .painted_quads()
            .into_iter()
            .find(|quad| quad.bounds == initial.bounds && !quad.background.is_transparent())
            .unwrap()
            .background;
        assert_eq!(
            actual, expected,
            "动画应完成到 hover 背景，不能冻结在初始颜色"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn animated_button_preserves_clicks_and_disabled_behavior(cx: &mut TestAppContext) {
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(400.), px(200.)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                cx.new(|_| Buttons {
                    clicks: 0,
                    disabled: false,
                })
            },
        )
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover("animated-button", cx);
        window.click("animated-button", cx);
        assert_eq!(view.read(cx).clicks, 1);
        view.update(cx, |this, cx| {
            this.disabled = true;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("animated-button", cx);
        assert_eq!(view.read(cx).clicks, 1);
        view.update(cx, |this, cx| {
            this.disabled = false;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("animated-button", cx);
        assert_eq!(view.read(cx).clicks, 2);
        window.click("animated-menu", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("popup-menu").visible());
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    cx.update(|cx| assert_eq!(view.read(cx).clicks, 3));
}
