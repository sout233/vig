use gpui_kit::base::{
    Disableable,
    motion::{Transition, transition},
};
use gpui_kit::component::{
    ActiveTheme, Colorize, Icon, Selectable, Sizable, Size,
    button::{Button as NativeButton, ButtonCustomVariant, ButtonVariant, ButtonVariants},
    menu::DropdownMenu,
};
use gpui_kit::{prelude::*, *};
use std::time::{Duration, Instant};

#[cfg(all(test, feature = "ui-verify"))]
mod tests;

#[derive(Default)]
struct Interaction {
    hovered: bool,
    held: bool,
    pulse_until: Option<Instant>,
}

#[derive(IntoElement)]
pub struct AnimatedButton {
    inner: NativeButton,
    id: ElementId,
    variant: ButtonVariant,
    disabled: bool,
}

impl AnimatedButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            inner: NativeButton::new(id.clone()),
            id,
            variant: ButtonVariant::Default,
            disabled: false,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.inner = self.inner.label(label);
        self
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.inner = self.inner.icon(icon.into());
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.inner = self.inner.accessibility_label(label);
        self
    }

    pub fn compact(mut self) -> Self {
        self.inner = self.inner.compact();
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.inner = self.inner.on_click(handler);
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.inner.interactivity().tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
        });
        self
    }
}

impl Styled for AnimatedButton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}

impl ParentElement for AnimatedButton {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(children);
    }
}

impl InteractiveElement for AnimatedButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}

impl Disableable for AnimatedButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.inner = self.inner.disabled(disabled);
        self
    }
}

impl ButtonVariants for AnimatedButton {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant.clone();
        self.inner = self.inner.with_variant(variant);
        self
    }
}

impl Sizable for AnimatedButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.inner = self.inner.with_size(size);
        self
    }
}

impl Selectable for AnimatedButton {
    fn selected(mut self, selected: bool) -> Self {
        self.inner = self.inner.selected(selected);
        self
    }

    fn is_selected(&self) -> bool {
        self.inner.is_selected()
    }

    fn open(mut self, open: bool) -> Self {
        self.inner = self.inner.open(open);
        self
    }

    fn is_open(&self) -> bool {
        self.inner.is_open()
    }
}

impl DropdownMenu for AnimatedButton {}

impl RenderOnce for AnimatedButton {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state((self.id.clone(), "button-pointer"), cx, |_, _| {
            Interaction::default()
        });
        if self.disabled || cx.reduce_motion() {
            state.update(cx, |state, _| *state = Interaction::default());
            return self.inner.into_any_element();
        }
        let pointer = state.read(cx);
        let pulse = pointer
            .pulse_until
            .is_some_and(|until| Instant::now() < until);
        let pressed = pointer.held && pointer.hovered || pulse;
        let hovered = pointer.hovered;
        if pulse && !cx.reduce_motion() {
            window.request_animation_frame();
        }
        let hover = transition(
            (self.id.clone(), "button-hover"),
            if hovered { 1f32 } else { 0. },
            Transition::new(Duration::from_millis(160)),
            window,
            cx,
        );
        let pressure = transition(
            (self.id.clone(), "button-pressure"),
            if pressed { 1f32 } else { 0. },
            Transition::new(Duration::from_millis(if pressed { 75 } else { 150 })),
            window,
            cx,
        );
        let material = self.inner.style().background.is_some();
        let light = matches!(self.variant, ButtonVariant::Primary);
        let theme = cx.theme();
        let selected = self.inner.is_selected() || self.inner.is_open();
        let idle = if selected {
            theme.button_active
        } else if self.variant.is_ghost() {
            theme.transparent
        } else if light {
            theme.button_primary
        } else {
            theme.button
        };
        let (top, bottom) = if material {
            if light {
                (rgb(0xf5f5f5).into(), rgb(0xe8e8e8).into())
            } else {
                (rgb(0x303030).into(), rgb(0x282828).into())
            }
        } else {
            (idle, idle)
        };
        let hover_color = if light {
            theme.button_primary_hover
        } else {
            theme.button_hover
        };
        let active_color = if light {
            theme.button_primary_active
        } else {
            theme.button_active
        };
        // Colorize weights `self` by the factor, unlike a conventional lerp.
        let mix = |color: Hsla| {
            let color = if hover == 0. {
                color
            } else if hover == 1. {
                hover_color
            } else {
                color.mix_oklab(hover_color, 1. - hover)
            };
            if pressure == 0. {
                color
            } else if pressure == 1. {
                active_color
            } else {
                color.mix_oklab(active_color, 1. - pressure)
            }
        };
        let (top, bottom) = (mix(top), mix(bottom));
        let foreground = if light {
            theme.button_primary_foreground
        } else {
            theme.button_foreground
        };
        let mut surface = Style::default();
        surface.corner_radii = Corners {
            top_left: theme.radius.into(),
            top_right: theme.radius.into(),
            bottom_left: theme.radius.into(),
            bottom_right: theme.radius.into(),
        };
        surface.refine(self.inner.style());
        surface.background = Some(
            linear_gradient(
                180.,
                linear_color_stop(top, 0.),
                linear_color_stop(bottom, 1.),
            )
            .into(),
        );
        if let Some(mut shadows) = self.inner.style().box_shadow.clone() {
            for shadow in &mut shadows {
                if shadow.inset {
                    shadow.color = shadow.color.opacity(1. - pressure * 0.3);
                } else {
                    shadow.offset.y *= 1. - pressure * 0.5;
                    shadow.blur_radius *= 1. - pressure * 0.25;
                }
            }
            surface.box_shadow = shadows;
        }
        // Native hover/active styles accept only solid colors. Keep their fill
        // transparent and paint the animated material once around the native control.
        let clear = transparent_black();
        let variant = ButtonCustomVariant::new(cx)
            .color(clear)
            .hover(clear)
            .active(clear)
            .foreground(foreground);
        self.inner = self
            .inner
            .custom(variant)
            .bg(clear)
            .shadow_none()
            .border_color(clear);
        let down = state.clone();
        let up = state.clone();
        let outside = state.clone();
        let keyboard = state.clone();
        let control = self
            .inner
            .on_hover(move |hovered, _, cx| {
                state.update(cx, |state, cx| {
                    if state.hovered != *hovered {
                        state.hovered = *hovered;
                        cx.notify();
                    }
                });
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                down.update(cx, |state, cx| {
                    state.held = true;
                    state.pulse_until = Some(Instant::now() + Duration::from_millis(90));
                    cx.notify();
                });
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                up.update(cx, |state, cx| {
                    if state.held {
                        state.held = false;
                        cx.notify();
                    }
                });
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                outside.update(cx, |state, cx| {
                    if state.held {
                        state.held = false;
                        cx.notify();
                    }
                });
            })
            .on_key_down(move |event, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    keyboard.update(cx, |state, cx| {
                        state.pulse_until = Some(Instant::now() + Duration::from_millis(120));
                        cx.notify();
                    });
                }
            })
            .into_any_element();
        ButtonSurface {
            control,
            style: surface,
        }
        .into_any_element()
    }
}

/// Shares the native button's layout and hitboxes; adds no container or input handler.
struct ButtonSurface {
    control: AnyElement,
    style: Style,
}

impl IntoElement for ButtonSurface {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ButtonSurface {
    type RequestLayoutState = ();

    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.control.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.control.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.style.paint(bounds, window, cx, |window, cx| {
            self.control.paint(window, cx)
        });
    }
}
