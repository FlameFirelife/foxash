use super::{ButtonView, ImageAsset, SceneItem, TextLine, WindowEvent, WindowScene};
use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::null_mut;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;

type Widget = *mut c_void;
type Callback = Option<unsafe extern "C" fn()>;
type SourceCallback = Option<unsafe extern "C" fn(*mut c_void) -> c_int>;
type DestroyCallback = Option<unsafe extern "C" fn(*mut c_void)>;

#[cfg_attr(target_env = "gnu", link(name = "dl"))]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_NOW: c_int = 2;
const GTK_WINDOW_TOPLEVEL: c_int = 0;
const GTK_ORIENTATION_VERTICAL: c_int = 1;
const GDK_ENTER_NOTIFY_MASK: c_int = 1 << 10;
const GTK_PRIORITY_APPLICATION: u32 = 600;

#[derive(Clone, Copy)]
struct GtkApi {
    _gtk: *mut c_void,
    _gobject: *mut c_void,
    _glib: *mut c_void,
    init_check: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char) -> c_int,
    window_new: unsafe extern "C" fn(c_int) -> Widget,
    window_set_title: unsafe extern "C" fn(Widget, *const c_char),
    window_set_default_size: unsafe extern "C" fn(Widget, c_int, c_int),
    window_set_position: unsafe extern "C" fn(Widget, c_int),
    container_add: unsafe extern "C" fn(Widget, Widget),
    box_new: unsafe extern "C" fn(c_int, c_int) -> Widget,
    box_pack_start: unsafe extern "C" fn(Widget, Widget, c_int, c_int, u32),
    label_new: unsafe extern "C" fn(*const c_char) -> Widget,
    label_set_line_wrap: unsafe extern "C" fn(Widget, c_int),
    label_set_text: unsafe extern "C" fn(Widget, *const c_char),
    entry_new: unsafe extern "C" fn() -> Widget,
    entry_get_text: unsafe extern "C" fn(Widget) -> *const c_char,
    button_new_with_label: unsafe extern "C" fn(*const c_char) -> Widget,
    button_set_label: unsafe extern "C" fn(Widget, *const c_char),
    image_new_from_file: unsafe extern "C" fn(*const c_char) -> Widget,
    widget_set_size_request: unsafe extern "C" fn(Widget, c_int, c_int),
    widget_show_all: unsafe extern "C" fn(Widget),
    widget_hide: unsafe extern "C" fn(Widget),
    widget_destroy: unsafe extern "C" fn(Widget),
    widget_add_events: unsafe extern "C" fn(Widget, c_int),
    widget_get_style_context: unsafe extern "C" fn(Widget) -> *mut c_void,
    css_provider_new: unsafe extern "C" fn() -> *mut c_void,
    css_provider_load_from_data:
        unsafe extern "C" fn(*mut c_void, *const c_char, isize, *mut *mut c_void) -> c_int,
    style_context_add_provider: unsafe extern "C" fn(*mut c_void, *mut c_void, u32),
    signal_connect_data: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        Callback,
        *mut c_void,
        DestroyCallback,
        u32,
    ) -> u64,
    object_unref: unsafe extern "C" fn(*mut c_void),
    timeout_add: unsafe extern "C" fn(u32, SourceCallback, *mut c_void) -> u32,
    main: unsafe extern "C" fn(),
}

impl GtkApi {
    unsafe fn open_library(names: &[&CStr]) -> Result<*mut c_void, String> {
        for name in names {
            let handle = dlopen(name.as_ptr(), RTLD_NOW);
            if !handle.is_null() {
                return Ok(handle);
            }
        }
        Err("Foxash could not load the native GTK 3 graphics library".to_string())
    }

    unsafe fn symbol<T: Copy>(library: *mut c_void, name: &'static [u8]) -> Result<T, String> {
        let pointer = dlsym(library, name.as_ptr().cast());
        if pointer.is_null() {
            return Err(format!(
                "The installed GTK 3 library is missing {}",
                CStr::from_bytes_with_nul_unchecked(name).to_string_lossy()
            ));
        }
        Ok(std::mem::transmute_copy(&pointer))
    }

    unsafe fn load() -> Result<Self, String> {
        let gtk = Self::open_library(&[c"libgtk-3.so.0"])?;
        let gobject = Self::open_library(&[c"libgobject-2.0.so.0"])?;
        let glib = Self::open_library(&[c"libglib-2.0.so.0"])?;
        macro_rules! sym {
            ($library:ident, $name:literal, $ty:ty) => {
                Self::symbol::<$ty>($library, concat!($name, "\0").as_bytes())?
            };
        }
        Ok(Self {
            _gtk: gtk,
            _gobject: gobject,
            _glib: glib,
            init_check: sym!(
                gtk,
                "gtk_init_check",
                unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char) -> c_int
            ),
            window_new: sym!(gtk, "gtk_window_new", unsafe extern "C" fn(c_int) -> Widget),
            window_set_title: sym!(
                gtk,
                "gtk_window_set_title",
                unsafe extern "C" fn(Widget, *const c_char)
            ),
            window_set_default_size: sym!(
                gtk,
                "gtk_window_set_default_size",
                unsafe extern "C" fn(Widget, c_int, c_int)
            ),
            window_set_position: sym!(
                gtk,
                "gtk_window_set_position",
                unsafe extern "C" fn(Widget, c_int)
            ),
            container_add: sym!(
                gtk,
                "gtk_container_add",
                unsafe extern "C" fn(Widget, Widget)
            ),
            box_new: sym!(
                gtk,
                "gtk_box_new",
                unsafe extern "C" fn(c_int, c_int) -> Widget
            ),
            box_pack_start: sym!(
                gtk,
                "gtk_box_pack_start",
                unsafe extern "C" fn(Widget, Widget, c_int, c_int, u32)
            ),
            label_new: sym!(
                gtk,
                "gtk_label_new",
                unsafe extern "C" fn(*const c_char) -> Widget
            ),
            label_set_line_wrap: sym!(
                gtk,
                "gtk_label_set_line_wrap",
                unsafe extern "C" fn(Widget, c_int)
            ),
            label_set_text: sym!(
                gtk,
                "gtk_label_set_text",
                unsafe extern "C" fn(Widget, *const c_char)
            ),
            entry_new: sym!(gtk, "gtk_entry_new", unsafe extern "C" fn() -> Widget),
            entry_get_text: sym!(
                gtk,
                "gtk_entry_get_text",
                unsafe extern "C" fn(Widget) -> *const c_char
            ),
            button_new_with_label: sym!(
                gtk,
                "gtk_button_new_with_label",
                unsafe extern "C" fn(*const c_char) -> Widget
            ),
            button_set_label: sym!(
                gtk,
                "gtk_button_set_label",
                unsafe extern "C" fn(Widget, *const c_char)
            ),
            image_new_from_file: sym!(
                gtk,
                "gtk_image_new_from_file",
                unsafe extern "C" fn(*const c_char) -> Widget
            ),
            widget_set_size_request: sym!(
                gtk,
                "gtk_widget_set_size_request",
                unsafe extern "C" fn(Widget, c_int, c_int)
            ),
            widget_show_all: sym!(gtk, "gtk_widget_show_all", unsafe extern "C" fn(Widget)),
            widget_hide: sym!(gtk, "gtk_widget_hide", unsafe extern "C" fn(Widget)),
            widget_destroy: sym!(gtk, "gtk_widget_destroy", unsafe extern "C" fn(Widget)),
            widget_add_events: sym!(
                gtk,
                "gtk_widget_add_events",
                unsafe extern "C" fn(Widget, c_int)
            ),
            widget_get_style_context: sym!(
                gtk,
                "gtk_widget_get_style_context",
                unsafe extern "C" fn(Widget) -> *mut c_void
            ),
            css_provider_new: sym!(
                gtk,
                "gtk_css_provider_new",
                unsafe extern "C" fn() -> *mut c_void
            ),
            css_provider_load_from_data: sym!(
                gtk,
                "gtk_css_provider_load_from_data",
                unsafe extern "C" fn(*mut c_void, *const c_char, isize, *mut *mut c_void) -> c_int
            ),
            style_context_add_provider: sym!(
                gtk,
                "gtk_style_context_add_provider",
                unsafe extern "C" fn(*mut c_void, *mut c_void, u32)
            ),
            signal_connect_data: sym!(
                gobject,
                "g_signal_connect_data",
                unsafe extern "C" fn(
                    *mut c_void,
                    *const c_char,
                    Callback,
                    *mut c_void,
                    DestroyCallback,
                    u32,
                ) -> u64
            ),
            object_unref: sym!(gobject, "g_object_unref", unsafe extern "C" fn(*mut c_void)),
            timeout_add: sym!(
                glib,
                "g_timeout_add",
                unsafe extern "C" fn(u32, SourceCallback, *mut c_void) -> u32
            ),
            main: sym!(gtk, "gtk_main", unsafe extern "C" fn()),
        })
    }
}

enum WindowCommand {
    CompleteInput(Vec<SceneItem>),
    RejectInput(String),
    UpdateButton(ButtonView),
    Close,
}

enum UiCommand {
    Create {
        scene: WindowScene,
        command_receiver: Receiver<WindowCommand>,
        event_sender: Sender<WindowEvent>,
        ready_sender: Sender<Result<(), String>>,
    },
}

struct UiState {
    api: GtkApi,
    command_receiver: Receiver<UiCommand>,
    windows: Vec<Box<WindowState>>,
}

static GTK_UI: OnceLock<Result<Sender<UiCommand>, String>> = OnceLock::new();

pub struct WindowHandle {
    command_sender: Sender<WindowCommand>,
    event_receiver: Receiver<WindowEvent>,
    is_open: bool,
    pending_events: VecDeque<WindowEvent>,
}

impl WindowHandle {
    pub fn wait_for_event(&mut self) -> Result<WindowEvent, String> {
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(event);
        }
        self.event_receiver
            .recv()
            .map_err(|_| "The Foxash graphics window stopped unexpectedly".to_string())
    }

    pub fn wait_for_input(&mut self) -> Result<(Vec<String>, Option<String>), String> {
        loop {
            match self
                .event_receiver
                .recv()
                .map_err(|_| "The Foxash graphics window stopped unexpectedly".to_string())?
            {
                WindowEvent::Input { values, button } => return Ok((values, button)),
                WindowEvent::Closed => {
                    return Err("The graphics window was closed before input was submitted".into())
                }
                other => self.pending_events.push_back(other),
            }
        }
    }

    pub fn complete_input(&self, items: Vec<SceneItem>) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::CompleteInput(items))
            .map_err(|_| "The Foxash graphics window has already closed".to_string())
    }

    pub fn reject_input(&self, error: String) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::RejectInput(error))
            .map_err(|_| "The Foxash graphics window has already closed".to_string())
    }

    pub fn update_button(&self, button: ButtonView) -> Result<(), String> {
        self.command_sender
            .send(WindowCommand::UpdateButton(button))
            .map_err(|_| "The Foxash graphics window has already closed".to_string())
    }

    pub fn close(&mut self) {
        if self.is_open {
            let _ = self.command_sender.send(WindowCommand::Close);
            self.wait_until_closed();
        }
    }

    pub fn wait_until_closed(&mut self) {
        if !self.is_open {
            return;
        }
        loop {
            match self.event_receiver.recv() {
                Ok(WindowEvent::Closed) | Err(_) => break,
                Ok(_) => {}
            }
        }
        self.is_open = false;
    }
}

impl Drop for WindowHandle {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn launch_window(scene: WindowScene) -> Result<WindowHandle, String> {
    let service = GTK_UI
        .get_or_init(start_gtk_ui)
        .clone()
        .map_err(|message| message)?;
    let (command_sender, command_receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    let (ready_sender, ready_receiver) = mpsc::channel();
    service
        .send(UiCommand::Create {
            scene,
            command_receiver,
            event_sender,
            ready_sender,
        })
        .map_err(|_| "The Foxash Linux graphics service stopped unexpectedly".to_string())?;

    match ready_receiver.recv() {
        Ok(Ok(())) => Ok(WindowHandle {
            command_sender,
            event_receiver,
            is_open: true,
            pending_events: VecDeque::new(),
        }),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("The Foxash Linux window stopped during startup".to_string()),
    }
}

struct WindowState {
    api: GtkApi,
    scene: WindowScene,
    window: Widget,
    deferred_content: Widget,
    action: Widget,
    error: Widget,
    input_rows: Vec<Widget>,
    entries: Vec<Widget>,
    command_receiver: Receiver<WindowCommand>,
    event_sender: Sender<WindowEvent>,
    submitted: bool,
    submission_pending: bool,
    closed: bool,
}

fn start_gtk_ui() -> Result<Sender<UiCommand>, String> {
    let (command_sender, command_receiver) = mpsc::channel();
    let (ready_sender, ready_receiver) = mpsc::channel();
    thread::Builder::new()
        .name("foxash-native-ui".to_string())
        .spawn(move || unsafe {
            let api = match GtkApi::load() {
                Ok(api) => api,
                Err(error) => {
                    let _ = ready_sender.send(Err(error));
                    return;
                }
            };
            if (api.init_check)(null_mut(), null_mut()) == 0 {
                let _ = ready_sender.send(Err(
                    "Foxash needs a Linux desktop session with GTK 3 installed to open windows"
                        .to_string(),
                ));
                return;
            }
            let mut state = Box::new(UiState {
                api,
                command_receiver,
                windows: Vec::new(),
            });
            let state_pointer = (&mut *state as *mut UiState).cast::<c_void>();
            (api.timeout_add)(30, Some(ui_tick), state_pointer);
            let _ = ready_sender.send(Ok(()));
            (api.main)();
        })
        .map_err(|error| format!("Could not start the Foxash Linux UI thread: {}", error))?;

    match ready_receiver.recv() {
        Ok(Ok(())) => Ok(command_sender),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("The Foxash Linux UI thread stopped during startup".to_string()),
    }
}

unsafe extern "C" fn ui_tick(data: *mut c_void) -> c_int {
    let ui = &mut *(data as *mut UiState);
    while let Ok(command) = ui.command_receiver.try_recv() {
        match command {
            UiCommand::Create {
                scene,
                command_receiver,
                event_sender,
                ready_sender,
            } => match create_window(ui.api, scene, command_receiver, event_sender) {
                Ok(window) => {
                    ui.windows.push(window);
                    let _ = ready_sender.send(Ok(()));
                }
                Err(error) => {
                    let _ = ready_sender.send(Err(error));
                }
            },
        }
    }

    for window in &mut ui.windows {
        process_commands(window);
    }
    ui.windows.retain(|window| !window.closed);
    1
}

unsafe fn create_window(
    api: GtkApi,
    scene: WindowScene,
    command_receiver: Receiver<WindowCommand>,
    event_sender: Sender<WindowEvent>,
) -> Result<Box<WindowState>, String> {
    let title = c_string(&scene.title);
    let window = (api.window_new)(GTK_WINDOW_TOPLEVEL);
    if window.is_null() {
        return Err("GTK could not create a Foxash window".to_string());
    }
    (api.window_set_title)(window, title.as_ptr());
    (api.window_set_default_size)(window, scene.width, scene.height);
    (api.window_set_position)(window, 1);
    let content = (api.box_new)(GTK_ORIENTATION_VERTICAL, 10);
    (api.container_add)(window, content);
    apply_css(
        &api,
        window,
        &format!("* {{ background-color: {}; }}", css_color(scene.background)),
    );

    let mut input_rows = Vec::new();
    let mut entries = Vec::new();
    for item in &scene.items {
        match item {
            SceneItem::Text(line) => add_text(&api, content, line),
            SceneItem::Input(input) => {
                let row = (api.box_new)(GTK_ORIENTATION_VERTICAL, 4);
                let prompt = c_string(&input.prompt);
                let label = (api.label_new)(prompt.as_ptr());
                (api.label_set_line_wrap)(label, 1);
                let entry = (api.entry_new)();
                (api.widget_set_size_request)(entry, input.width, input.height.max(24));
                (api.box_pack_start)(row, label, 0, 0, 0);
                (api.box_pack_start)(row, entry, 0, 0, 0);
                (api.box_pack_start)(content, row, 0, 0, 0);
                input_rows.push(row);
                entries.push(entry);
            }
            SceneItem::Image(image) => {
                if let Err(error) = add_image(&api, content, image) {
                    (api.widget_destroy)(window);
                    return Err(error);
                }
            }
        }
    }

    let button_label = c_string(&scene.submit_button.label);
    let deferred_content = (api.box_new)(GTK_ORIENTATION_VERTICAL, 10);
    (api.box_pack_start)(content, deferred_content, 0, 0, 0);
    let action = (api.button_new_with_label)(button_label.as_ptr());
    (api.box_pack_start)(content, action, 0, 0, 0);
    let error = (api.label_new)(c"".as_ptr());
    (api.label_set_line_wrap)(error, 1);
    (api.box_pack_start)(content, error, 0, 0, 0);

    let mut state = Box::new(WindowState {
        api,
        submitted: entries.is_empty(),
        scene,
        window,
        deferred_content,
        action,
        error,
        input_rows,
        entries,
        command_receiver,
        event_sender,
        submission_pending: false,
        closed: false,
    });
    let state_pointer = (&mut *state as *mut WindowState).cast::<c_void>();
    connect(
        &state.api,
        action,
        c"clicked",
        callback(on_clicked),
        state_pointer,
    );
    if state.scene.submit_button.name.is_some() {
        (state.api.widget_add_events)(action, GDK_ENTER_NOTIFY_MASK);
        connect(
            &state.api,
            action,
            c"enter-notify-event",
            callback(on_hover),
            state_pointer,
        );
    }
    connect(
        &state.api,
        window,
        c"destroy",
        callback(on_destroy),
        state_pointer,
    );
    apply_button_style(&state.api, action, &state.scene.submit_button);
    (state.api.widget_show_all)(window);
    Ok(state)
}

fn add_text(api: &GtkApi, content: Widget, line: &TextLine) {
    unsafe {
        let value = c_string(&line.text);
        let label = (api.label_new)(value.as_ptr());
        (api.label_set_line_wrap)(label, 1);
        let css = format!(
            "* {{ color: {}; font-size: {}px; }}",
            css_color(line.color),
            line.size.max(8)
        );
        apply_css(api, label, &css);
        (api.box_pack_start)(content, label, 0, 0, 0);
    }
}

fn add_image(api: &GtkApi, content: Widget, image: &ImageAsset) -> Result<(), String> {
    unsafe {
        if image.mime_type == "application/json" {
            let text = String::from_utf8_lossy(&image.contents);
            let value = c_string(&text);
            let label = (api.label_new)(value.as_ptr());
            (api.label_set_line_wrap)(label, 1);
            apply_css(api, label, "* { font-family: monospace; }");
            (api.box_pack_start)(content, label, 0, 0, 0);
            return Ok(());
        }
        let path = c_string(&image.name);
        let widget = (api.image_new_from_file)(path.as_ptr());
        if widget.is_null() {
            return Err(format!("GTK could not display image '{}'", image.name));
        }
        if image.width.is_some() || image.height.is_some() {
            (api.widget_set_size_request)(
                widget,
                image.width.unwrap_or(-1),
                image.height.unwrap_or(-1),
            );
        }
        (api.box_pack_start)(content, widget, 0, 0, 0);
    }
    Ok(())
}

unsafe fn apply_css(api: &GtkApi, widget: Widget, css: &str) {
    let provider = (api.css_provider_new)();
    let css = c_string(css);
    (api.css_provider_load_from_data)(provider, css.as_ptr(), -1, null_mut());
    let context = (api.widget_get_style_context)(widget);
    (api.style_context_add_provider)(context, provider, GTK_PRIORITY_APPLICATION);
    (api.object_unref)(provider);
}

unsafe fn apply_button_style(api: &GtkApi, widget: Widget, button: &ButtonView) {
    let css = format!(
        "* {{ color: {}; background-color: {}; font-size: {}px; }}",
        css_color(button.text_color),
        css_color(button.button_color),
        button.text_size.max(8)
    );
    apply_css(api, widget, &css);
}

unsafe fn connect(
    api: &GtkApi,
    widget: Widget,
    signal: &CStr,
    handler: Callback,
    state: *mut c_void,
) {
    (api.signal_connect_data)(widget, signal.as_ptr(), handler, state, None, 0);
}

unsafe fn callback<T: Copy, U: Copy>(function: T) -> Option<U> {
    Some(std::mem::transmute_copy(&function))
}

unsafe extern "C" fn on_clicked(widget: Widget, data: *mut c_void) {
    let state = &mut *(data as *mut WindowState);
    if state.closed || state.submission_pending {
        return;
    }
    if !state.submitted && !state.entries.is_empty() {
        let values = state
            .entries
            .iter()
            .map(|entry| {
                let text = (state.api.entry_get_text)(*entry);
                if text.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(text).to_string_lossy().into_owned()
                }
            })
            .collect();
        let button = state.scene.submit_button.name.clone();
        if state
            .event_sender
            .send(WindowEvent::Input { values, button })
            .is_ok()
        {
            state.submission_pending = true;
        } else {
            (state.api.widget_destroy)(widget);
        }
    } else if let Some(name) = state.scene.submit_button.name.clone() {
        let _ = state.event_sender.send(WindowEvent::ButtonPressed(name));
    } else {
        (state.api.widget_destroy)(state.window);
    }
}

unsafe extern "C" fn on_hover(_widget: Widget, _event: *mut c_void, data: *mut c_void) -> c_int {
    let state = &mut *(data as *mut WindowState);
    if let Some(name) = state.scene.submit_button.name.clone() {
        let _ = state.event_sender.send(WindowEvent::ButtonHovered(name));
    }
    0
}

unsafe extern "C" fn on_destroy(_widget: Widget, data: *mut c_void) {
    let state = &mut *(data as *mut WindowState);
    if !state.closed {
        state.closed = true;
        let _ = state.event_sender.send(WindowEvent::Closed);
    }
}

unsafe fn process_commands(state: &mut WindowState) {
    while let Ok(command) = state.command_receiver.try_recv() {
        match command {
            WindowCommand::CompleteInput(items) => {
                state.submitted = true;
                state.submission_pending = false;
                for row in &state.input_rows {
                    (state.api.widget_hide)(*row);
                }
                for item in items {
                    match item {
                        SceneItem::Text(line) => {
                            add_text(&state.api, state.deferred_content, &line)
                        }
                        SceneItem::Image(image) => {
                            if let Err(error) =
                                add_image(&state.api, state.deferred_content, &image)
                            {
                                let message = c_string(&error);
                                (state.api.label_set_text)(state.error, message.as_ptr());
                            }
                        }
                        SceneItem::Input(_) => {}
                    }
                }
                if state.scene.submit_button.name.is_none() {
                    (state.api.button_set_label)(state.action, c"Close".as_ptr());
                }
                (state.api.widget_show_all)(state.window);
                for row in &state.input_rows {
                    (state.api.widget_hide)(*row);
                }
            }
            WindowCommand::RejectInput(error) => {
                state.submission_pending = false;
                let error = c_string(&error);
                (state.api.label_set_text)(state.error, error.as_ptr());
                (state.api.widget_show_all)(state.window);
            }
            WindowCommand::UpdateButton(button) => {
                state.scene.submit_button = button;
                let label = c_string(&state.scene.submit_button.label);
                (state.api.button_set_label)(state.action, label.as_ptr());
                apply_button_style(&state.api, state.action, &state.scene.submit_button);
            }
            WindowCommand::Close => {
                (state.api.widget_destroy)(state.window);
                return;
            }
        }
    }
}

fn c_string(value: &str) -> CString {
    CString::new(value.replace('\0', "�")).expect("NUL characters were removed")
}

fn css_color(color: u32) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        color & 0xff,
        (color >> 8) & 0xff,
        (color >> 16) & 0xff
    )
}
