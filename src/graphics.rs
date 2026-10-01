#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct TextLine {
    pub text: String,
    pub size: i32,
    pub color: u32,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct InputField {
    pub prompt: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct WindowScene {
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub background: u32,
    pub text: Vec<TextLine>,
    pub inputs: Vec<InputField>,
}

#[cfg(target_os = "windows")]
#[path = "graphics_windows.rs"]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::{launch_window, WindowHandle};

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{TextLine, WindowScene};

    pub struct WindowHandle;

    pub fn launch_window(_scene: WindowScene) -> Result<WindowHandle, String> {
        Err("Foxash graphics are currently supported on Windows only".to_string())
    }

    impl WindowHandle {
        pub fn wait_for_input(&self) -> Result<Vec<String>, String> {
            Err("Foxash graphics are currently supported on Windows only".to_string())
        }

        pub fn complete_input(&self, _lines: Vec<TextLine>) -> Result<(), String> {
            Err("Foxash graphics are currently supported on Windows only".to_string())
        }

        pub fn reject_input(&self, _error: String) -> Result<(), String> {
            Err("Foxash graphics are currently supported on Windows only".to_string())
        }

        pub fn close(&mut self) {}

        pub fn wait_until_closed(&mut self) {}
    }
}

#[cfg(not(target_os = "windows"))]
pub use platform::{launch_window, WindowHandle};
