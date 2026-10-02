use super::{ButtonView, SceneItem, WindowEvent, WindowScene};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW, DEFAULT_CHARSET,
    DEFAULT_PITCH, DEFAULT_QUALITY, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_NORMAL, PAINTSTRUCT,
    TRANSPARENT,
};
use windows_sys::Win32::Graphics::GdiPlus::{
    GdipCreateBitmapFromFile, GdipCreateFromHDC, GdipDeleteGraphics, GdipDisposeImage,
    GdipDrawImageRectI, GdipGetImageDimension, GdiplusShutdown, GdiplusStartup,
    GdiplusStartupInput, GpBitmap, GpGraphics, GpImage,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, GetWindowTextW, KillTimer, LoadCursorW, PostQuitMessage,
    RegisterClassW, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    TranslateMessage, UnregisterClassW, WindowFromPoint, BN_CLICKED, BS_OWNERDRAW, CREATESTRUCTW,
    CS_HREDRAW, CS_VREDRAW, ES_AUTOHSCROLL, GWLP_USERDATA, IDC_ARROW, MSG, SWP_NOMOVE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_DRAWITEM,
    WM_ERASEBKGND, WM_NCCREATE, WM_PAINT, WM_TIMER, WNDCLASSW, WS_CHILD, WS_EX_CLIENTEDGE,
    WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
};

const SUBMIT_BUTTON_ID: usize = 1;
const FIRST_INPUT_ID: usize = 100;
const COMMAND_TIMER_ID: usize = 1;
const COMMAND_POLL_MILLISECONDS: u32 = 35;
static NEXT_CLASS_ID: AtomicUsize = AtomicUsize::new(1);

enum WindowCommand {
    CompleteInput(Vec<SceneItem>),
    RejectInput(String),
    UpdateButton(ButtonView),
    Close,
}

pub struct WindowHandle {
    command_sender: Sender<WindowCommand>,
    event_receiver: Receiver<WindowEvent>,
    thread: Option<JoinHandle<()>>,
    pending_events: VecDeque<WindowEvent>,
}

impl WindowHandle {
    pub fn wait_for_event(&mut self) -> Result<WindowEvent, String> {
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(event);
        }
        self.event_receiver
            .recv()
            .map_err(|_| "The Foxash window stopped unexpectedly".to_string())
    }

    pub fn wait_for_input(&mut self) -> Result<(Vec<String>, Option<String>), String> {
        loop {
            match self.event_receiver.recv() {
                Ok(WindowEvent::Input { values, button }) => return Ok((values, button)),
                Ok(WindowEvent::Closed) => {
                    return Err("The window was closed before its input was submitted".to_string())
                }
                Ok(event) => self.pending_events.push_back(event),
                Err(_) => return Err("The Foxash window stopped unexpectedly".to_string()),
            }
        }
    }

    pub fn complete_input(&self, items: Vec<SceneItem>) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::CompleteInput(items))
            .map_err(|_| "The Foxash window has already closed".to_string())
    }

    pub fn reject_input(&self, error: String) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::RejectInput(error))
            .map_err(|_| "The Foxash window has already closed".to_string())
    }

    pub fn update_button(&self, button: ButtonView) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::UpdateButton(button))
            .map_err(|_| "The Foxash window has already closed".to_string())
    }

    pub fn close(&mut self) {
        if self.thread.is_some() {
            let _ = self.command_sender.send(WindowCommand::Close);
            self.wait_until_closed();
        }
    }

    pub fn wait_until_closed(&mut self) {
        if self.thread.is_none() {
            return;
        }

        loop {
            match self.event_receiver.recv() {
                Ok(WindowEvent::Closed) | Err(_) => break,
                Ok(_) => {}
            }
        }

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WindowHandle {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn launch_window(scene: WindowScene) -> Result<WindowHandle, String> {
    let (command_sender, command_receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    let (ready_sender, ready_receiver) = mpsc::channel();

    let thread = thread::spawn(move || {
        if let Err(error) = run_window(scene, command_receiver, event_sender, &ready_sender) {
            let _ = ready_sender.send(Err(error));
        }
    });

    match ready_receiver.recv() {
        Ok(Ok(())) => Ok(WindowHandle {
            command_sender,
            event_receiver,
            thread: Some(thread),
            pending_events: VecDeque::new(),
        }),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            Err("The Foxash window thread stopped during startup".to_string())
        }
    }
}

struct WindowState {
    scene: WindowScene,
    edits: Vec<HWND>,
    button: HWND,
    error: Option<String>,
    submitted: bool,
    submission_pending: bool,
    command_receiver: Receiver<WindowCommand>,
    event_sender: Sender<WindowEvent>,
    instance: windows_sys::Win32::Foundation::HINSTANCE,
    hover_was_inside: bool,
    button_override: Option<String>,
    native_images: Vec<Option<*mut GpImage>>,
}

fn run_window(
    mut scene: WindowScene,
    command_receiver: Receiver<WindowCommand>,
    event_sender: Sender<WindowEvent>,
    ready_sender: &Sender<Result<(), String>>,
) -> Result<(), String> {
    let class_id = NEXT_CLASS_ID.fetch_add(1, Ordering::Relaxed);
    let class_name = to_wide(&format!("FoxashWindowClass{}", class_id));
    let title = to_wide(&scene.title);

    unsafe {
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(format!(
                "Could not get the Windows application handle (error {})",
                windows_sys::Win32::Foundation::GetLastError()
            ));
        }

        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };

        if RegisterClassW(&window_class) == 0 {
            return Err(format!(
                "Could not register the Foxash window class (error {})",
                windows_sys::Win32::Foundation::GetLastError()
            ));
        }

        let mut gdiplus_token = 0usize;
        let gdiplus_input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        if GdiplusStartup(&mut gdiplus_token, &gdiplus_input, null_mut()) != 0 {
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err("Foxash could not start the native Windows image renderer".to_string());
        }

        let native_images = load_native_images(&scene.items);
        let minimum_height = layout(&scene, &native_images).button_y + 70;
        scene.height = scene.height.max(minimum_height);
        let mut state = Box::new(WindowState {
            submitted: !scene
                .items
                .iter()
                .any(|item| matches!(item, SceneItem::Input(_))),
            scene,
            edits: Vec::new(),
            button: null_mut(),
            error: None,
            submission_pending: false,
            command_receiver,
            event_sender,
            instance,
            hover_was_inside: false,
            button_override: None,
            native_images,
        });
        let state_pointer = (&mut *state as *mut WindowState).cast::<c_void>();

        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            100,
            100,
            state.scene.width,
            state.scene.height,
            null_mut(),
            null_mut(),
            instance,
            state_pointer,
        );

        if window.is_null() {
            GdiplusShutdown(gdiplus_token);
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err(format!(
                "Could not create window '{}' (error {})",
                state.scene.title,
                windows_sys::Win32::Foundation::GetLastError()
            ));
        }

        if let Some(error) = state.error.take() {
            DestroyWindow(window);
            dispose_native_images(&state.native_images);
            GdiplusShutdown(gdiplus_token);
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err(error);
        }
        if state.button.is_null() {
            DestroyWindow(window);
            dispose_native_images(&state.native_images);
            GdiplusShutdown(gdiplus_token);
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err("Foxash could not create the window button".to_string());
        }

        SetTimer(window, COMMAND_TIMER_ID, COMMAND_POLL_MILLISECONDS, None);
        ShowWindow(window, SW_SHOW);
        windows_sys::Win32::Graphics::Gdi::UpdateWindow(window);
        let _ = ready_sender.send(Ok(()));

        let mut message: MSG = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result == -1 {
                DestroyWindow(window);
                dispose_native_images(&state.native_images);
                GdiplusShutdown(gdiplus_token);
                UnregisterClassW(class_name.as_ptr(), instance);
                let _ = state.event_sender.send(WindowEvent::Closed);
                return Err(format!(
                    "The Foxash window message loop failed (error {})",
                    windows_sys::Win32::Foundation::GetLastError()
                ));
            }

            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        dispose_native_images(&state.native_images);
        GdiplusShutdown(gdiplus_token);
        UnregisterClassW(class_name.as_ptr(), instance);
    }

    Ok(())
}

struct Layout {
    item_y: Vec<i32>,
    prompt_y: Vec<i32>,
    edit_y: Vec<i32>,
    edit_width: Vec<i32>,
    edit_height: Vec<i32>,
    button_y: i32,
}

fn layout(scene: &WindowScene, native_images: &[Option<*mut GpImage>]) -> Layout {
    let mut y = 20;
    let mut item_y = Vec::with_capacity(scene.items.len());
    let mut prompt_y = Vec::new();
    let mut edit_y = Vec::new();
    let mut edit_width = Vec::new();
    let mut edit_height = Vec::new();
    let mut image_index = 0;
    for item in &scene.items {
        match item {
            SceneItem::Text(line) => {
                item_y.push(y);
                y += line.size.max(12) + 14;
            }
            SceneItem::Input(input) => {
                y += 8;
                item_y.push(y);
                prompt_y.push(y);
                y += 24;
                edit_y.push(y);
                let width = input.width.clamp(20, (scene.width - 40).max(20));
                let height = input.height.max(24);
                edit_width.push(width);
                edit_height.push(height);
                y += height + 12;
            }
            SceneItem::Image(image) => {
                item_y.push(y);
                let native_image = native_images.get(image_index).copied().flatten();
                let (_, height) = display_image_size(image, native_image, scene.width);
                y += height + 12;
                image_index += 1;
            }
        }
    }

    Layout {
        item_y,
        prompt_y,
        edit_y,
        edit_width,
        edit_height,
        button_y: y.max(24),
    }
}

fn display_image_size(
    image: &super::ImageAsset,
    native_image: Option<*mut GpImage>,
    window_width: i32,
) -> (i32, i32) {
    let mut natural_width = 240.0f32;
    let mut natural_height = 160.0f32;
    if let Some(native_image) = native_image {
        unsafe {
            GdipGetImageDimension(native_image, &mut natural_width, &mut natural_height);
        }
        if natural_width <= 0.0 || natural_height <= 0.0 {
            natural_width = 240.0;
            natural_height = 160.0;
        }
    }
    let maximum_width = (window_width - 40).max(40);
    match (image.width, image.height) {
        (Some(width), Some(height)) => (width.max(1), height.max(1)),
        (Some(width), None) => {
            let width = width.max(1).min(maximum_width);
            let height = ((width as f32 * natural_height / natural_width).round() as i32).max(1);
            (width, height)
        }
        (None, Some(height)) => {
            let height = height.max(1);
            let width = ((height as f32 * natural_width / natural_height).round() as i32)
                .max(1)
                .min(maximum_width);
            (width, height)
        }
        (None, None) => {
            let scale = (320.0 / natural_width)
                .min(220.0 / natural_height)
                .min(maximum_width as f32 / natural_width);
            (
                (natural_width * scale).round().max(1.0) as i32,
                (natural_height * scale).round().max(1.0) as i32,
            )
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        return 1;
    }

    let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut WindowState;
    if state_pointer.is_null() {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let state = &mut *state_pointer;

    match message {
        WM_CREATE => {
            let positions = layout(&state.scene, &state.native_images);
            for (index, _) in state
                .scene
                .items
                .iter()
                .filter(|item| matches!(item, SceneItem::Input(_)))
                .enumerate()
            {
                let edit_class = to_wide("EDIT");
                let empty = to_wide("");
                let edit = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    edit_class.as_ptr(),
                    empty.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    20,
                    positions.edit_y[index],
                    positions.edit_width[index],
                    positions.edit_height[index],
                    window,
                    (FIRST_INPUT_ID + index) as *mut c_void,
                    state.instance,
                    null(),
                );
                if edit.is_null() {
                    state.error = Some("Foxash could not create an input box".to_string());
                } else {
                    state.edits.push(edit);
                }
            }

            let button_class = to_wide("BUTTON");
            let button_text = to_wide(
                if state.scene.submit_button.name.is_none() && state.submitted {
                    "Close"
                } else {
                    &state.scene.submit_button.label
                },
            );
            state.button = CreateWindowExW(
                0,
                button_class.as_ptr(),
                button_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
                20,
                positions.button_y,
                110,
                32,
                window,
                SUBMIT_BUTTON_ID as *mut c_void,
                state.instance,
                null(),
            );
            0
        }
        WM_TIMER if wparam == COMMAND_TIMER_ID => {
            while let Ok(command) = state.command_receiver.try_recv() {
                match command {
                    WindowCommand::CompleteInput(lines) => {
                        state.native_images.extend(load_native_images(&lines));
                        state.scene.items.extend(lines);
                        state.submitted = true;
                        state.submission_pending = false;
                        state.error = None;
                        state.button_override = if state.scene.submit_button.name.is_none() {
                            Some("Close".to_string())
                        } else {
                            None
                        };
                        for edit in &state.edits {
                            ShowWindow(*edit, SW_HIDE);
                        }
                        if state.scene.submit_button.name.is_none() {
                            SetWindowTextW(state.button, to_wide("Close").as_ptr());
                        }
                        let positions = layout(&state.scene, &state.native_images);
                        state.scene.height = state.scene.height.max(positions.button_y + 70);
                        SetWindowPos(
                            window,
                            null_mut(),
                            0,
                            0,
                            state.scene.width,
                            state.scene.height,
                            SWP_NOMOVE | SWP_NOZORDER,
                        );
                        SetWindowPos(
                            state.button,
                            null_mut(),
                            20,
                            positions.button_y,
                            110,
                            32,
                            SWP_NOZORDER,
                        );
                        InvalidateRect(window, null(), 1);
                    }
                    WindowCommand::RejectInput(error) => {
                        state.error = Some(error);
                        state.submission_pending = false;
                        state.button_override = None;
                        InvalidateRect(window, null(), 1);
                    }
                    WindowCommand::UpdateButton(button) => {
                        state.scene.submit_button = button;
                        InvalidateRect(state.button, null(), 1);
                    }
                    WindowCommand::Close => {
                        DestroyWindow(window);
                    }
                }
            }
            if state.scene.submit_button.name.is_some() {
                let mut cursor = POINT { x: 0, y: 0 };
                let inside =
                    GetCursorPos(&mut cursor) != 0 && WindowFromPoint(cursor) == state.button;
                if inside && !state.hover_was_inside {
                    if let Some(name) = state.scene.submit_button.name.clone() {
                        let _ = state.event_sender.send(WindowEvent::ButtonHovered(name));
                    }
                }
                state.hover_was_inside = inside;
            }
            0
        }
        WM_DRAWITEM if wparam as usize == SUBMIT_BUTTON_ID => {
            let draw = &*(lparam as *const DRAWITEMSTRUCT);
            paint_button(draw, state);
            1
        }
        WM_ERASEBKGND => {
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(window, &mut client);
            let brush = CreateSolidBrush(state.scene.background);
            FillRect(hdc, &client, brush);
            DeleteObject(brush);
            1
        }
        WM_PAINT => {
            paint(window, state);
            0
        }
        WM_COMMAND
            if (wparam & 0xffff) as usize == SUBMIT_BUTTON_ID
                && ((wparam >> 16) & 0xffff) as u32 == BN_CLICKED =>
        {
            if state.submitted
                || !state
                    .scene
                    .items
                    .iter()
                    .any(|item| matches!(item, SceneItem::Input(_)))
            {
                if let Some(name) = state.scene.submit_button.name.clone() {
                    let _ = state.event_sender.send(WindowEvent::ButtonPressed(name));
                } else {
                    DestroyWindow(window);
                }
                return 0;
            }
            if state.submission_pending {
                return 0;
            }

            let inputs = state
                .edits
                .iter()
                .map(|edit| window_text(*edit))
                .collect::<Vec<_>>();
            if state
                .event_sender
                .send(WindowEvent::Input {
                    values: inputs,
                    button: state.scene.submit_button.name.clone(),
                })
                .is_ok()
            {
                state.submission_pending = true;
                state.error = None;
                state.button_override = Some("Checking...".to_string());
                InvalidateRect(state.button, null(), 1);
            } else {
                DestroyWindow(window);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            KillTimer(window, COMMAND_TIMER_ID);
            let _ = state.event_sender.send(WindowEvent::Closed);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

unsafe fn paint(window: HWND, state: &WindowState) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(window, &mut paint);
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let brush = CreateSolidBrush(state.scene.background);
    FillRect(hdc, &client, brush);
    DeleteObject(brush);
    SetBkMode(hdc, TRANSPARENT as i32);

    let positions = layout(&state.scene, &state.native_images);
    let mut graphics: *mut GpGraphics = null_mut();
    let has_images = state.scene.items.iter().any(
        |item| matches!(item, SceneItem::Image(image) if image.mime_type != "application/json"),
    );
    if has_images {
        GdipCreateFromHDC(hdc, &mut graphics);
    }
    let mut input_index = 0;
    let mut image_index = 0;
    for (index, item) in state.scene.items.iter().enumerate() {
        match item {
            SceneItem::Text(line) => draw_text(
                hdc,
                20,
                positions.item_y[index],
                &line.text,
                line.size,
                line.color,
            ),
            SceneItem::Input(input) => {
                if !state.submitted {
                    draw_text(
                        hdc,
                        20,
                        positions.prompt_y[input_index],
                        &input.prompt,
                        12,
                        opposite_color(state.scene.background),
                    );
                }
                input_index += 1;
            }
            SceneItem::Image(image) => {
                if image.mime_type == "application/json" {
                    let text = String::from_utf8_lossy(&image.contents);
                    for (line_index, line) in text.lines().take(32).enumerate() {
                        draw_text(
                            hdc,
                            20,
                            positions.item_y[index] + line_index as i32 * 16,
                            line,
                            12,
                            opposite_color(state.scene.background),
                        );
                    }
                } else {
                    let loaded = state.native_images.get(image_index).copied().flatten();
                    if let Some(bitmap) = loaded {
                        let (width, height) = display_image_size(image, loaded, state.scene.width);
                        if !graphics.is_null() {
                            GdipDrawImageRectI(
                                graphics,
                                bitmap,
                                20,
                                positions.item_y[index],
                                width,
                                height,
                            );
                        }
                    } else {
                        draw_text(
                            hdc,
                            20,
                            positions.item_y[index],
                            &format!("Could not display image: {}", image.name),
                            12,
                            rgb(190, 0, 0),
                        );
                    }
                }
                image_index += 1;
            }
        }
    }
    if !graphics.is_null() {
        GdipDeleteGraphics(graphics);
    }

    if let Some(error) = &state.error {
        draw_text(hdc, 20, positions.button_y - 22, error, 12, rgb(190, 0, 0));
    }

    EndPaint(window, &paint);
}

unsafe fn load_native_images(items: &[SceneItem]) -> Vec<Option<*mut GpImage>> {
    items
        .iter()
        .filter_map(|item| match item {
            SceneItem::Image(image) if image.mime_type != "application/json" => {
                let mut bitmap: *mut GpBitmap = null_mut();
                let path = to_wide(&image.name);
                let status = GdipCreateBitmapFromFile(path.as_ptr(), &mut bitmap);
                if status == 0 && !bitmap.is_null() {
                    Some(Some(bitmap.cast::<GpImage>()))
                } else {
                    Some(None)
                }
            }
            SceneItem::Image(_) => Some(None),
            _ => None,
        })
        .collect()
}

unsafe fn dispose_native_images(images: &[Option<*mut GpImage>]) {
    for image in images.iter().flatten() {
        GdipDisposeImage(*image);
    }
}

unsafe fn paint_button(draw: &DRAWITEMSTRUCT, state: &WindowState) {
    let mut color = state.scene.submit_button.button_color;
    if draw.itemState & ODS_SELECTED != 0 {
        color = rgb(
            (color & 0xff) * 4 / 5,
            ((color >> 8) & 0xff) * 4 / 5,
            ((color >> 16) & 0xff) * 4 / 5,
        );
    }
    let brush = CreateSolidBrush(color);
    FillRect(draw.hDC, &draw.rcItem, brush);
    DeleteObject(brush);
    SetBkMode(draw.hDC, TRANSPARENT as i32);
    SetTextColor(draw.hDC, state.scene.submit_button.text_color);

    let label = state
        .button_override
        .as_deref()
        .unwrap_or(&state.scene.submit_button.label);
    let wide = to_wide(label);
    let face = to_wide("Segoe UI");
    let font = CreateFontW(
        -state.scene.submit_button.text_size.max(8),
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        DEFAULT_QUALITY as u32,
        DEFAULT_PITCH as u32,
        face.as_ptr(),
    );
    let previous = SelectObject(draw.hDC, font);
    let mut bounds = draw.rcItem;
    DrawTextW(
        draw.hDC,
        wide.as_ptr(),
        wide.len().saturating_sub(1) as i32,
        &mut bounds,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    SelectObject(draw.hDC, previous);
    DeleteObject(font);
}

unsafe fn draw_text(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    text: &str,
    size: i32,
    color: u32,
) {
    let wide = to_wide(text);
    let face = to_wide("Segoe UI");
    let font = CreateFontW(
        -size.max(8),
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        DEFAULT_QUALITY as u32,
        DEFAULT_PITCH as u32,
        face.as_ptr(),
    );
    let previous = SelectObject(hdc, font);
    SetTextColor(hdc, color);
    TextOutW(
        hdc,
        x,
        y,
        wide.as_ptr(),
        wide.len().saturating_sub(1) as i32,
    );
    SelectObject(hdc, previous);
    DeleteObject(font);
}

unsafe fn window_text(window: HWND) -> String {
    let mut buffer = vec![0u16; 8192];
    let length = GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn opposite_color(color: u32) -> u32 {
    let red = color & 0xff;
    let green = (color >> 8) & 0xff;
    let blue = (color >> 16) & 0xff;
    let brightness = red * 299 + green * 587 + blue * 114;
    if brightness >= 128_000 {
        rgb(0, 0, 0)
    } else {
        rgb(255, 255, 255)
    }
}

fn rgb(red: u32, green: u32, blue: u32) -> u32 {
    red | (green << 8) | (blue << 16)
}
