#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod asr;
mod editor;
mod media;
mod model;
mod motion;
mod pointer_drag;
mod theme;
mod track_order;
#[cfg(feature = "ui-verify")]
mod verify;
mod views;
use gpui_kit::{component::TitleBar, *};

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            theme::apply(cx);
            editor::bind_keys(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let (handle, view) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1380.), px(900.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(1080.), px(720.))),
                    ..TitleBar::window_options()
                },
                cx,
                |window, cx| cx.new(|cx| editor::Editor::new(window, cx)),
            )
            .expect("无法打开 Vig 窗口");
            #[cfg(feature = "ui-verify")]
            if let Some(directory) = std::env::var_os("VIG_VERIFY_DIR") {
                verify::run(handle, view.clone(), directory.into(), cx);
            }
            let _ = (handle, view);
            cx.activate(true);
        });
}
