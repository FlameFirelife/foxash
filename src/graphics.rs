#[derive(Debug, Clone)]
pub struct TextLine {
    pub text: String,
    pub size: i32,
    pub color: u32,
}

#[derive(Debug, Clone)]
pub struct InputField {
    pub prompt: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct ImageAsset {
    pub name: String,
    pub mime_type: String,
    pub contents: Vec<u8>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

#[derive(Debug, Clone)]
pub enum SceneItem {
    Text(TextLine),
    Input(InputField),
    Image(ImageAsset),
}

#[derive(Debug, Clone)]
pub struct ButtonView {
    pub name: Option<String>,
    pub label: String,
    pub text_size: i32,
    pub text_color: u32,
    pub button_color: u32,
}

#[derive(Debug, Clone)]
pub struct WindowScene {
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub background: u32,
    pub items: Vec<SceneItem>,
    pub submit_button: ButtonView,
}

#[derive(Debug, Clone)]
pub enum WindowEvent {
    Input {
        values: Vec<String>,
        button: Option<String>,
    },
    ButtonPressed(String),
    ButtonHovered(String),
    Closed,
}

#[cfg(target_os = "windows")]
#[path = "graphics_windows.rs"]
mod platform;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[path = "graphics_linux.rs"]
mod platform;

#[cfg(all(target_os = "linux", not(target_env = "gnu")))]
#[path = "graphics_linux_unavailable.rs"]
mod platform;

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub use platform::launch_window;

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub use platform::WindowHandle;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod platform {
    use super::{ButtonView, SceneItem, WindowEvent, WindowScene};

    pub struct WindowHandle;

    pub fn launch_window(_scene: WindowScene) -> Result<WindowHandle, String> {
        Err("Foxash graphics are currently supported on Windows and Linux".to_string())
    }

    impl WindowHandle {
        pub fn wait_for_event(&self) -> Result<WindowEvent, String> {
            Err("Foxash graphics are currently supported on Windows and Linux".to_string())
        }

        pub fn complete_input(&self, _items: Vec<SceneItem>) -> Result<(), String> {
            Err("Foxash graphics are currently supported on Windows and Linux".to_string())
        }

        pub fn reject_input(&self, _error: String) -> Result<(), String> {
            Err("Foxash graphics are currently supported on Windows and Linux".to_string())
        }

        pub fn update_button(&self, _button: ButtonView) -> Result<(), String> {
            Err("Foxash graphics are currently supported on Windows and Linux".to_string())
        }

        pub fn close(&mut self) {}
        pub fn wait_until_closed(&mut self) {}
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub use platform::{launch_window, WindowHandle};
