#[cfg(windows)]
mod native {
    #[derive(Clone, Copy, Default)]
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetCursorPos(point: *mut Point) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn ShowCursor(show: i32) -> i32;
        fn GetAsyncKeyState(key: i32) -> i16;
    }

    pub struct RelativePointer {
        origin: Point,
        anchor: Point,
        scale: f32,
        hide_calls: usize,
    }

    impl RelativePointer {
        pub fn begin(
            position: (f32, f32),
            viewport: (f32, f32),
            scale: f32,
        ) -> Result<Self, String> {
            let mut origin = Point::default();
            if unsafe { GetCursorPos(&mut origin) } == 0 {
                return Err(format!(
                    "读取光标位置失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            let anchor = Point {
                x: origin.x + ((viewport.0 / 2. - position.0) * scale).round() as i32,
                y: origin.y + ((viewport.1 / 2. - position.1) * scale).round() as i32,
            };
            let mut pointer = Self {
                origin,
                anchor,
                scale,
                hide_calls: 0,
            };
            // Balance the Win32 visibility counter exactly when the drag ends.
            loop {
                pointer.hide_calls += 1;
                if unsafe { ShowCursor(0) } < 0 {
                    break;
                }
            }
            if unsafe { SetCursorPos(anchor.x, anchor.y) } == 0 {
                return Err(format!(
                    "重定位光标失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(pointer)
        }

        pub fn delta(&mut self) -> Result<f32, String> {
            let mut position = Point::default();
            if unsafe { GetCursorPos(&mut position) } == 0 {
                return Err(format!(
                    "读取拖动位置失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            let delta = (position.x - self.anchor.x) as f32 / self.scale;
            if position.x != self.anchor.x || position.y != self.anchor.y {
                if unsafe { SetCursorPos(self.anchor.x, self.anchor.y) } == 0 {
                    return Err(format!(
                        "重定位拖动光标失败：{}",
                        std::io::Error::last_os_error()
                    ));
                }
            }
            Ok(delta)
        }
    }

    impl Drop for RelativePointer {
        fn drop(&mut self) {
            if unsafe { SetCursorPos(self.origin.x, self.origin.y) } == 0 {
                eprintln!("恢复拖动光标位置失败：{}", std::io::Error::last_os_error());
            }
            for _ in 0..self.hide_calls {
                unsafe {
                    ShowCursor(1);
                }
            }
        }
    }

    pub fn left_button_down() -> bool {
        unsafe { GetAsyncKeyState(1) < 0 }
    }

    pub fn shift_down() -> bool {
        unsafe { GetAsyncKeyState(0x10) < 0 }
    }
}
#[cfg(windows)]
pub use native::{RelativePointer, left_button_down, shift_down};

#[cfg(not(windows))]
pub struct RelativePointer;

#[cfg(not(windows))]
impl RelativePointer {
    pub fn begin(_: (f32, f32), _: (f32, f32), _: f32) -> Result<Self, String> {
        Err("当前平台尚未实现数字的无限拖动".into())
    }

    pub fn delta(&mut self) -> Result<f32, String> {
        Err("当前平台尚未实现数字的无限拖动".into())
    }
}

#[cfg(not(windows))]
pub fn left_button_down() -> bool {
    false
}

#[cfg(not(windows))]
pub fn shift_down() -> bool {
    false
}
