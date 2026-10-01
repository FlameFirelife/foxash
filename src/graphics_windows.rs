use super::{TextLine, WindowScene};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, TextOutW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DEFAULT_QUALITY, FW_NORMAL, PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, GetWindowTextW, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassW,
    SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    UnregisterClassW, BN_CLICKED, BS_DEFPUSHBUTTON, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
    ES_AUTOHSCROLL, GWLP_USERDATA, IDC_ARROW, MSG, SWP_NOMOVE, SWP_NOZORDER, SW_HIDE, SW_SHOW,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_ERASEBKGND, WM_NCCREATE, WM_PAINT, WM_TIMER,
    WNDCLASSW, WS_CHILD, WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
};

const SUBMIT_BUTTON_ID: usize = 1;
const FIRST_INPUT_ID: usize = 100;
const COMMAND_TIMER_ID: usize = 1;
const COMMAND_POLL_MILLISECONDS: u32 = 35;
static NEXT_CLASS_ID: AtomicUsize = AtomicUsize::new(1);

enum WindowCommand {
    CompleteInput(Vec<TextLine>),
    RejectInput(String),
    Close,
}

enum WindowEvent {
    Input(Vec<String>),
    Closed,
}

pub struct WindowHandle {
    command_sender: Sender<WindowCommand>,
    event_receiver: Receiver<WindowEvent>,
    thread: Option<JoinHandle<()>>,
}

impl WindowHandle {
    pub fn wait_for_input(&self) -> Result<Vec<String>, String> {
        match self.event_receiver.recv() {
            Ok(WindowEvent::Input(values)) => Ok(values),
            Ok(WindowEvent::Closed) => {
                Err("The window was closed before its input was submitted".to_string())
            }
            Err(_) => Err("The Foxash window stopped unexpectedly".to_string()),
        }
    }

    pub fn complete_input(&self, lines: Vec<TextLine>) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::CompleteInput(lines))
            .map_err(|_| "The Foxash window has already closed".to_string())
    }

    pub fn reject_input(&self, error: String) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::RejectInput(error))
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
                Ok(WindowEvent::Input(_)) => {}
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

        let minimum_height = layout(&scene).button_y + 70;
        scene.height = scene.height.max(minimum_height);
        let mut state = Box::new(WindowState {
            submitted: scene.inputs.is_empty(),
            scene,
            edits: Vec::new(),
            button: null_mut(),
            error: None,
            submission_pending: false,
            command_receiver,
            event_sender,
            instance,
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
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err(format!(
                "Could not create window '{}' (error {})",
                state.scene.title,
                windows_sys::Win32::Foundation::GetLastError()
            ));
        }

        if let Some(error) = state.error.take() {
            DestroyWindow(window);
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err(error);
        }
        if state.button.is_null() {
            DestroyWindow(window);
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

        UnregisterClassW(class_name.as_ptr(), instance);
    }

    Ok(())
}

struct Layout {
    text_y: Vec<i32>,
    prompt_y: Vec<i32>,
    edit_y: Vec<i32>,
    edit_width: Vec<i32>,
    edit_height: Vec<i32>,
    button_y: i32,
}

fn layout(scene: &WindowScene) -> Layout {
    let mut y = 20;
    let mut text_y = Vec::with_capacity(scene.text.len());
    for line in &scene.text {
        text_y.push(y);
        y += line.size.max(12) + 14;
    }

    let mut prompt_y = Vec::with_capacity(scene.inputs.len());
    let mut edit_y = Vec::with_capacity(scene.inputs.len());
    let mut edit_width = Vec::with_capacity(scene.inputs.len());
    let mut edit_height = Vec::with_capacity(scene.inputs.len());
    for input in &scene.inputs {
        y += 8;
        prompt_y.push(y);
        y += 24;
        edit_y.push(y);
        let width = input.width.clamp(20, (scene.width - 40).max(20));
        let height = input.height.max(24);
        edit_width.push(width);
        edit_height.push(height);
        y += height + 12;
    }

    Layout {
        text_y,
        prompt_y,
        edit_y,
        edit_width,
        edit_height,
        button_y: y.max(24),
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
            let positions = layout(&state.scene);
            for (index, _) in state.scene.inputs.iter().enumerate() {
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
            let button_text = to_wide(if state.scene.inputs.is_empty() {
                "Close"
            } else {
                "Submit"
            });
            state.button = CreateWindowExW(
                0,
                button_class.as_ptr(),
                button_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
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
                        state.scene.text.extend(lines);
                        state.submitted = true;
                        state.submission_pending = false;
                        state.error = None;
                        for edit in &state.edits {
                            ShowWindow(*edit, SW_HIDE);
                        }
                        SetWindowTextW(state.button, to_wide("Close").as_ptr());
                        let positions = layout(&state.scene);
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
                        SetWindowTextW(state.button, to_wide("Submit").as_ptr());
                        InvalidateRect(window, null(), 1);
                    }
                    WindowCommand::Close => {
                        DestroyWindow(window);
                    }
                }
            }
            0
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
            if state.submitted || state.scene.inputs.is_empty() {
                DestroyWindow(window);
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
            if state.event_sender.send(WindowEvent::Input(inputs)).is_ok() {
                state.submission_pending = true;
                state.error = None;
                SetWindowTextW(state.button, to_wide("Checking...").as_ptr());
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

    let positions = layout(&state.scene);
    for (index, line) in state.scene.text.iter().enumerate() {
        draw_text(
            hdc,
            20,
            positions.text_y[index],
            &line.text,
            line.size,
            line.color,
        );
    }

    if !state.submitted {
        let color = opposite_color(state.scene.background);
        for (index, input) in state.scene.inputs.iter().enumerate() {
            draw_text(hdc, 20, positions.prompt_y[index], &input.prompt, 12, color);
        }
    }

    if let Some(error) = &state.error {
        draw_text(hdc, 20, positions.button_y - 22, error, 12, rgb(190, 0, 0));
    }

    EndPaint(window, &paint);
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
