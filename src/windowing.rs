use gpui_kit::{AnyWindowHandle, App, Entity, Render, Window, WindowOptions};

/// Opens an auxiliary window above its owner without making other applications stay behind it.
pub fn open_owned_window<V: Render>(
    parent: &mut Window,
    mut options: WindowOptions,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> Result<(AnyWindowHandle, Entity<V>), String> {
    // GPUI's Windows backend does not attach an owner for WindowKind::Floating.
    options.kind = gpui_kit::WindowKind::Floating;
    let (handle, view) =
        gpui_kit::open_window(options, cx, build).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    if !cx.is_test() {
        let result = handle
            .update(cx, |_, child, _| native::attach(child, parent))
            .map_err(|error| error.to_string())
            .and_then(|result| result);
        if let Err(error) = result {
            handle
                .update(cx, |_, window, _| window.remove_window())
                .map_err(|close_error| format!("{error}；关闭窗口失败：{close_error}"))?;
            return Err(error);
        }
    }
    #[cfg(not(windows))]
    let _ = parent;
    Ok((handle, view))
}

#[cfg(windows)]
mod native {
    use gpui_kit::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const GWLP_HWNDPARENT: i32 = -8;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowLongPtrW(window: isize, index: i32, value: isize) -> isize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetLastError(code: u32);
    }

    pub(super) fn attach(child: &Window, parent: &Window) -> Result<(), String> {
        let hwnd = |window: &Window| match HasWindowHandle::window_handle(window)
            .map_err(|error| error.to_string())?
            .as_raw()
        {
            RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
            _ => Err("窗口未提供 Win32 句柄".to_string()),
        };
        set_owner(hwnd(child)?, hwnd(parent)?)
    }

    fn set_owner(child: isize, parent: isize) -> Result<(), String> {
        // These are top-level windows: GWLP_HWNDPARENT sets ownership, not WS_CHILD parenting.
        // A zero return can mean the previous owner was absent, so clear/check last error.
        unsafe {
            SetLastError(0);
        }
        let previous = unsafe { SetWindowLongPtrW(child, GWLP_HWNDPARENT, parent) };
        let error = std::io::Error::last_os_error();
        if previous == 0 && error.raw_os_error() != Some(0) {
            return Err(format!("设置窗口归属失败：{error}"));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::set_owner;

        #[link(name = "user32")]
        unsafe extern "system" {
            fn CreateWindowExW(
                ex_style: u32,
                class: *const u16,
                title: *const u16,
                style: u32,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                parent: isize,
                menu: isize,
                instance: isize,
                parameter: *const (),
            ) -> isize;
            fn GetWindow(window: isize, command: u32) -> isize;
            fn DestroyWindow(window: isize) -> i32;
        }

        #[test]
        fn auxiliary_window_has_native_owner_and_is_destroyed_with_it() {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let create = || unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    class.as_ptr(),
                    0x00CF0000,
                    0,
                    0,
                    100,
                    100,
                    0,
                    0,
                    0,
                    std::ptr::null(),
                )
            };
            let parent = create();
            let child = create();
            assert_ne!(parent, 0);
            assert_ne!(child, 0);
            set_owner(child, parent).unwrap();
            assert_eq!(unsafe { GetWindow(child, 4) }, parent);
            assert_ne!(unsafe { DestroyWindow(parent) }, 0);
            assert_eq!(
                unsafe { DestroyWindow(child) },
                0,
                "owned window is destroyed with its owner"
            );
            assert!(set_owner(0, parent).is_err());
        }
    }
}
