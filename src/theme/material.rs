use gpui_kit::{prelude::*, *};

pub trait MaterialStyle: Styled + Sized {
    fn panel_material(self) -> Self {
        self.bg(linear_gradient(
            180.,
            linear_color_stop(rgb(0x181818), 0.),
            linear_color_stop(rgb(0x141414), 1.),
        ))
        .shadow(vec![BoxShadow {
            color: rgba(0xffffff10).into(),
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        }])
    }

    fn button_material(self, light: bool) -> Self {
        let (top, bottom) = if light {
            (0xf5f5f5, 0xe8e8e8)
        } else {
            (0x303030, 0x282828)
        };
        self.bg(linear_gradient(
            180.,
            linear_color_stop(rgb(top), 0.),
            linear_color_stop(rgb(bottom), 1.),
        ))
        .border_1()
        .border_color(rgb(if light { 0xffffff } else { 0x303030 }))
        .shadow(vec![
            BoxShadow {
                color: rgba(0xffffff4d).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(1.),
                spread_radius: px(0.),
                inset: true,
            },
            BoxShadow {
                color: rgba(0x00000033).into(),
                offset: point(px(0.), px(-2.)),
                blur_radius: px(4.),
                spread_radius: px(0.),
                inset: true,
            },
            BoxShadow {
                color: rgba(0x00000059).into(),
                offset: point(px(0.), px(4.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
                inset: false,
            },
        ])
    }

    fn recessed_material(self) -> Self {
        self.border_1().border_color(rgb(0x303030)).shadow(vec![
            BoxShadow {
                color: rgba(0x000000cc).into(),
                offset: point(px(0.), px(2.)),
                blur_radius: px(10.),
                spread_radius: px(0.),
                inset: true,
            },
            BoxShadow {
                color: rgba(0xffffff10).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: false,
            },
        ])
    }

    fn input_material(self) -> Self {
        self.bg(rgb(0x090909))
            .border_1()
            .border_color(rgb(0x303030))
            .shadow(vec![
                BoxShadow {
                    color: rgba(0x00000040).into(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                    inset: true,
                },
                BoxShadow {
                    color: rgba(0xffffff10).into(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: false,
                },
            ])
    }

    fn clip_material(self, video: bool) -> Self {
        let (top, bottom) = if video {
            (0x2b2b2b, 0x252525)
        } else {
            (0x242424, 0x1f1f1f)
        };
        self.bg(linear_gradient(
            180.,
            linear_color_stop(rgb(top), 0.),
            linear_color_stop(rgb(bottom), 1.),
        ))
        .shadow(vec![
            BoxShadow {
                color: rgba(0xffffff18).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            },
            BoxShadow {
                color: rgba(0x000000aa).into(),
                offset: point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(0.),
                inset: false,
            },
        ])
    }
}

impl<T: Styled> MaterialStyle for T {}
