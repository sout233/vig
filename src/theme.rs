use gpui_kit::{
    App,
    component::{Theme, ThemeMode},
    rgb,
};
mod button;
mod material;
pub use button::AnimatedButton;
pub use material::MaterialStyle;

pub const INK: u32 = 0xffffff;

pub const ACCENT: u32 = 0xffffff;

pub const BORDER: u32 = 0x303030;

pub const BACKGROUND: u32 = 0x000000;

pub const SURFACE: u32 = 0x111111;

pub const RAISED: u32 = 0x1d1d1d;

pub const MUTED: u32 = 0xaaaaaa;

pub const MONITOR: u32 = 0x000000;

pub const VIDEO: u32 = 0x242424;

pub const VIDEO_BORDER: u32 = 0x555555;

pub const AUDIO: u32 = 0x1c1c1c;

pub const AUDIO_BORDER: u32 = 0x444444;

pub const AUDIO_TEXT: u32 = 0xffffff;

pub const ERROR: u32 = 0xf09c9c;

pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.font_family = "Microsoft YaHei UI".into();
        theme.background = rgb(BACKGROUND).into();
        theme.foreground = rgb(INK).into();
        theme.sidebar = rgb(SURFACE).into();
        theme.sidebar_foreground = theme.foreground;
        theme.muted = rgb(RAISED).into();
        theme.muted_foreground = rgb(MUTED).into();
        theme.primary = rgb(ACCENT).into();
        theme.primary_foreground = rgb(BACKGROUND).into();
        theme.border = rgb(BORDER).into();
        theme.title_bar = rgb(SURFACE).into();
        theme.title_bar_border = theme.border;
        theme.input = theme.border;
        theme.ring = theme.primary;
        theme.selection = theme.primary.opacity(0.22);
        theme.popover = rgb(RAISED).into();
        theme.popover_foreground = theme.foreground;
        theme.secondary = theme.muted;
        theme.secondary_foreground = theme.foreground;
        theme.accent = rgb(0x2b2b2b).into();
        theme.accent_foreground = theme.foreground;
        theme.button = theme.muted;
        theme.button_foreground = theme.foreground;
        theme.button_hover = rgb(0x2b2b2b).into();
        theme.button_active = rgb(0x393939).into();
        theme.button_primary = theme.primary;
        theme.button_primary_foreground = theme.primary_foreground;
        theme.button_primary_hover = rgb(0xe5e5e5).into();
        theme.button_primary_active = rgb(0xcccccc).into();
        theme.button_secondary = theme.muted;
        theme.button_secondary_foreground = theme.foreground;
        theme.button_secondary_hover = theme.button_hover;
        theme.button_secondary_active = theme.button_active;
        theme.list_hover = theme.primary.opacity(0.10);
        theme.list_active = theme.primary.opacity(0.18);
        theme.list_active_border = theme.primary;
        theme.sidebar_accent = theme.muted;
        theme.sidebar_accent_foreground = theme.foreground;
        theme.slider_bar = theme.primary;
    });
}
