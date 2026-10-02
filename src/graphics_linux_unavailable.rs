use super::{ButtonView, SceneItem, WindowEvent, WindowScene};

const GRAPHICS_UNAVAILABLE: &str =
    "Foxash Linux graphics require a GNU/Linux build and GTK 3; this binary uses a non-GNU Linux target";

pub struct WindowHandle;

pub fn launch_window(_scene: WindowScene) -> Result<WindowHandle, String> {
    Err(GRAPHICS_UNAVAILABLE.to_string())
}

impl WindowHandle {
    pub fn wait_for_event(&mut self) -> Result<WindowEvent, String> {
        Err(GRAPHICS_UNAVAILABLE.to_string())
    }

    pub fn wait_for_input(&mut self) -> Result<(Vec<String>, Option<String>), String> {
        Err(GRAPHICS_UNAVAILABLE.to_string())
    }

    pub fn complete_input(&self, _items: Vec<SceneItem>) -> Result<(), String> {
        Err(GRAPHICS_UNAVAILABLE.to_string())
    }

    pub fn reject_input(&self, _error: String) -> Result<(), String> {
        Err(GRAPHICS_UNAVAILABLE.to_string())
    }

    pub fn update_button(&self, _button: ButtonView) -> Result<(), String> {
        Err(GRAPHICS_UNAVAILABLE.to_string())
    }

    pub fn close(&mut self) {}

    pub fn wait_until_closed(&mut self) {}
}
