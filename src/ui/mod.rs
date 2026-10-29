mod draw;
mod picker;
mod qa;
mod shortcuts;
mod visual;
use crate::{
    model::{Light, Preset, Routine, Scene, Snapshot},
    protocol,
    store::{Credentials, Shortcut, Store, SHORTCUT_ACTIONS},
    worker::{Command, Worker, UPDATE},
};
use draw::*;
use std::{
    collections::BTreeMap,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use visual::*;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{LibraryLoader::GetModuleHandleW, Registry::*, Threading::CreateMutexW},
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

const CLASS: &str = "LightControl.Native.1";
const TRAY: u32 = WM_APP + 2;
const OPEN: u32 = WM_APP + 3;
const QUIT: u32 = WM_APP + 4;
const HIDE: u32 = WM_APP + 5;
const STATS: u32 = WM_APP + 6;
const ANIMATION_TIMER: usize = 4;
const TRAY_OPEN: u32 = WM_APP + 7;
const SLIDER_TIMER: usize = 1;
const CLOCK_TIMER: usize = 2;
const TBM_GETPOS: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Settings,
    Shortcuts,
    Presets,
    Routines,
    Timer,
    Scenes,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn rgb(c: u32) -> u32 {
    ((c & 255) << 16) | (c & 0xff00) | ((c >> 16) & 255)
}
unsafe fn text(hwnd: HWND) -> String {
    let n = GetWindowTextLengthW(hwnd);
    let mut s = vec![0; n as usize + 1];
    GetWindowTextW(hwnd, s.as_mut_ptr(), s.len() as i32);
    String::from_utf16_lossy(&s[..n as usize])
}
pub fn error_box(s: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(s).as_ptr(),
            wide("LightControl").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
pub fn request_quit() {
    unsafe {
        let hwnd = FindWindowW(wide(CLASS).as_ptr(), null());
        if !hwnd.is_null() {
            PostMessageW(hwnd, QUIT, 0, 0);
        }
    }
}
pub fn request_hide() {
    unsafe {
        let hwnd = FindWindowW(wide(CLASS).as_ptr(), null());
        if !hwnd.is_null() {
            PostMessageW(hwnd, HIDE, 0, 0);
        }
    }
}
pub fn request_stats() {
    unsafe {
        let hwnd = FindWindowW(wide(CLASS).as_ptr(), null());
        if !hwnd.is_null() {
            PostMessageW(hwnd, STATS, 0, 0);
        }
    }
}

pub fn run(
    store: Store,
    preview: bool,
    start_hidden: bool,
    capture: Option<std::path::PathBuf>,
    mini: bool,
    page: Option<String>,
    ui_test: bool,
) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let class = wide(CLASS);
        let mutex_name = wide(if preview {
            "Local\\LightControl.Preview.1"
        } else {
            "Local\\LightControl.Native.1"
        });
        let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if mutex.is_null() {
            return Err("Falha ao iniciar instância.".into());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let h = FindWindowW(class.as_ptr(), null());
            if !h.is_null() && !start_hidden {
                PostMessageW(h, OPEN, mini as usize, 0);
            }
            CloseHandle(mutex);
            return Ok(());
        }
        let instance = GetModuleHandleW(null());
        let icon = make_icon();
        let class_def = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hIcon: icon,
            hIconSm: icon,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassExW(&class_def) == 0 {
            CloseHandle(mutex);
            return Err("Falha ao registrar janela.".into());
        }
        let common = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        InitCommonControlsEx(&common);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT | WS_EX_COMPOSITED,
            class.as_ptr(),
            wide("LightControl · Quarto").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            900,
            690,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            CloseHandle(mutex);
            return Err("Falha ao criar janela nativa.".into());
        }
        let dpi = GetDpiForWindow(hwnd).max(96) as f32 / 96.;
        let font = CreateFontW(
            -(14. * dpi) as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        let bg = CreateSolidBrush(rgb(BG));
        let tile = CreateSolidBrush(rgb(TILE));
        let card = CreateSolidBrush(rgb(CARD));
        let worker = Worker::start(store.root.clone(), hwnd as usize, preview);
        let taskbar = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        let mut ui = Box::new(Ui {
            hwnd,
            store,
            worker,
            snapshot: Snapshot::default(),
            display: Light::default(),
            pending_stage: None,
            preview,
            mini: false,
            page: Page::Home,
            controls: BTreeMap::new(),
            renderer: None,
            graphics: None,
            paints: 0,
            rebuilds: 0,
            updates: 0,
            dpi,
            font,
            bg,
            tile,
            card,
            icon,
            taskbar,
            dragging: false,
            last_slider: None,
            pending_send: None,
            timer: None,
            timer_native: false,
            timer_minutes: "30".into(),
            notice: String::new(),
            editing_days: 127,
            capture,
            layout_size: (0, 0),
            widgets: None,
            hover: None,
            connection_advanced: false,
            color_advanced: false,
            shortcuts: Vec::new(),
            capturing: None,
            opened_at: Instant::now(),
            tray_event_at: None,
            phase_start: Instant::now(),
            glow: 0.,
            modal: false,
            animations: {
                let mut enabled = 1i32;
                SystemParametersInfoW(
                    SPI_GETCLIENTAREAANIMATION,
                    0,
                    &mut enabled as *mut _ as _,
                    0,
                );
                enabled != 0
            },
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *ui as *mut Ui as isize);
        let dark = 1i32;
        DwmSetWindowAttribute(hwnd, 20, &dark as *const _ as _, 4);
        let corners = 2i32;
        DwmSetWindowAttribute(hwnd, 33, &corners as *const _ as _, 4);
        ui.add_tray();
        ui.register_hotkey();
        ui.resize_mode(mini);
        ui.page = match page.as_deref() {
            Some("settings") => Page::Settings,
            Some("shortcuts") => Page::Shortcuts,
            Some("presets") => Page::Presets,
            Some("routines") => Page::Routines,
            Some("scenes") => Page::Scenes,
            Some("timer") => Page::Timer,
            _ => Page::Home,
        };
        if preview && ui.capture.is_some() {
            ui.store.config.pinned = true;
        }
        ui.rebuild();
        let test_result = if ui_test {
            let result = qa::run(&mut ui);
            DestroyWindow(hwnd);
            result
        } else {
            Ok(())
        };
        if !start_hidden && !ui_test {
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);
        }
        if ui.capture.is_some() {
            SetTimer(hwnd, 3, 1000, None);
        }
        let mut message: MSG = zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result <= 0 {
                break;
            }
            if matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) && ui.capturing.is_some() {
                ui.capture_key(message.wParam as u32);
                continue;
            }
            if message.message == WM_KEYDOWN {
                if GetKeyState(VK_CONTROL as i32) < 0 {
                    let page = match message.wParam {
                        0x31 => Some(Page::Home),
                        0x32 => Some(Page::Presets),
                        0x33 => Some(Page::Scenes),
                        0x34 => Some(Page::Routines),
                        0x35 => Some(Page::Settings),
                        0x54 => Some(Page::Timer),
                        _ => None,
                    };
                    if let Some(page) = page {
                        ui.page(page);
                        continue;
                    }
                }
                if message.wParam == VK_ESCAPE as usize {
                    if ui.page != Page::Home {
                        ui.page(Page::Home);
                    } else if ui.mini {
                        ui.hide();
                    }
                    continue;
                }
                if message.wParam == VK_F5 as usize {
                    ui.send(Command::Refresh);
                    continue;
                }
                if message.wParam == VK_F12 as usize {
                    let _ = capture_client(hwnd, &ui.store.root.join("ui-snapshot.bmp"));
                    continue;
                }
            }
            if IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        ui.worker.shutdown();
        ui.remove_tray();
        for id in 1..=6 {
            UnregisterHotKey(hwnd, id);
        }
        CloseHandle(mutex);
        DeleteObject(ui.font);
        DeleteObject(ui.bg);
        DeleteObject(ui.tile);
        DeleteObject(ui.card);
        DestroyIcon(icon);
        test_result
    }
}

struct Ui {
    hwnd: HWND,
    store: Store,
    worker: Worker,
    snapshot: Snapshot,
    display: Light,
    pending_stage: Option<Light>,
    preview: bool,
    mini: bool,
    page: Page,
    controls: BTreeMap<u16, HWND>,
    renderer: Option<Renderer>,
    graphics: Option<Graphics>,
    paints: u64,
    rebuilds: u64,
    updates: u64,
    dpi: f32,
    font: HFONT,
    bg: HBRUSH,
    tile: HBRUSH,
    card: HBRUSH,
    icon: HICON,
    taskbar: u32,
    dragging: bool,
    last_slider: Option<Instant>,
    pending_send: Option<crate::model::Dps>,
    timer: Option<u64>,
    timer_native: bool,
    timer_minutes: String,
    notice: String,
    editing_days: u8,
    capture: Option<std::path::PathBuf>,
    layout_size: (i32, i32),
    widgets: Option<Renderer>,
    hover: Option<u16>,
    connection_advanced: bool,
    color_advanced: bool,
    shortcuts: Vec<Shortcut>,
    capturing: Option<usize>,
    opened_at: Instant,
    tray_event_at: Option<Instant>,
    phase_start: Instant,
    glow: f32,
    animations: bool,
    modal: bool,
}

impl Ui {
    fn send(&self, c: Command) {
        let _ = self.worker.tx.send(c);
    }
    unsafe fn invalidate(&self) {
        InvalidateRect(self.hwnd, null(), 0);
    }
    fn size(&self) -> (f32, f32) {
        unsafe {
            let mut r: RECT = zeroed();
            GetClientRect(self.hwnd, &mut r);
            (r.right as f32 / self.dpi, r.bottom as f32 / self.dpi)
        }
    }
    #[allow(clippy::too_many_arguments)]
    unsafe fn control(
        &mut self,
        id: u16,
        class: &str,
        label: &str,
        style: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> HWND {
        let child = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD | style,
            (x * self.dpi) as i32,
            (y * self.dpi) as i32,
            (w * self.dpi) as i32,
            (h * self.dpi) as i32,
            self.hwnd,
            id as usize as _,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(child, WM_SETFONT, self.font as usize, 0);
        if class == "LISTBOX" {
            SetWindowSubclass(child, Some(list_proc), 1, self.hwnd as usize);
        }
        if class == "BUTTON" {
            SetWindowSubclass(child, Some(button_proc), 1, self.hwnd as usize);
        }
        self.controls.insert(id, child);
        child
    }
    unsafe fn button(&mut self, id: u16, label: &str, x: f32, y: f32, w: f32, h: f32) {
        self.control(
            id,
            "BUTTON",
            label,
            WS_TABSTOP | BS_OWNERDRAW as u32,
            x,
            y,
            w,
            h,
        );
    }
    unsafe fn label(&mut self, id: u16, label: &str, x: f32, y: f32, w: f32) {
        self.control(id, "STATIC", label, 0, x, y, w, 22.);
    }
    unsafe fn edit(&mut self, id: u16, value: &str, x: f32, y: f32, w: f32, password: bool) {
        self.control(
            id,
            "EDIT",
            value,
            WS_TABSTOP
                | WS_BORDER
                | ES_AUTOHSCROLL as u32
                | if password { ES_PASSWORD as u32 } else { 0 },
            x,
            y,
            w,
            30.,
        );
        SendMessageW(self.controls[&id], EM_SETLIMITTEXT, 2048, 0);
    }
    #[allow(clippy::too_many_arguments)]
    unsafe fn slider(
        &mut self,
        id: u16,
        label: &str,
        value: u16,
        max: u16,
        x: f32,
        y: f32,
        w: f32,
    ) {
        self.label(
            800 + id,
            label,
            if id == 100 { x + w - 70. } else { x },
            y - 25.,
            if id == 100 {
                70.
            } else if id == 101 && self.display.mode == "colour" {
                w - 110.
            } else {
                w
            },
        );
        let h = self.control(
            id,
            "msctls_trackbar32",
            label,
            WS_TABSTOP | TBS_NOTICKS,
            x,
            y,
            w,
            if self.display.mode == "colour" && [101, 102].contains(&id) {
                28.
            } else {
                38.
            },
        );
        SendMessageW(
            h,
            TBM_SETRANGEMIN,
            0,
            if id == 100 && self.display.mode != "colour" {
                1
            } else {
                0
            },
        );
        SetWindowSubclass(h, Some(slider_proc), 2, 0);
        SendMessageW(h, TBM_SETRANGEMAX, 0, max as isize);
        SendMessageW(h, TBM_SETPOS, 0, value as isize);
        InvalidateRect(h, null(), 0);
        SendMessageW(h, TBM_SETPAGESIZE, 0, 5);
    }
    unsafe fn combo(&mut self, id: u16, items: &[String], selected: usize, x: f32, y: f32, w: f32) {
        let h = self.control(
            id,
            "COMBOBOX",
            "",
            WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
            x,
            y,
            w,
            200.,
        );
        for s in items {
            SendMessageW(h, CB_ADDSTRING, 0, wide(s).as_ptr() as isize);
        }
        SendMessageW(h, CB_SETCURSEL, selected, 0);
    }
    unsafe fn list(&mut self, id: u16, items: &[String], x: f32, y: f32, w: f32, h: f32) {
        let list = self.control(
            id,
            "LISTBOX",
            "",
            WS_TABSTOP
                | WS_VSCROLL
                | LBS_NOTIFY as u32
                | LBS_NOINTEGRALHEIGHT as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32,
            x,
            y,
            w,
            h,
        );
        SendMessageW(list, LB_SETITEMHEIGHT, 0, (54. * self.dpi) as isize);
        for s in items {
            SendMessageW(list, LB_ADDSTRING, 0, wide(s).as_ptr() as isize);
        }
    }
    unsafe fn get(&self, id: u16) -> String {
        self.controls.get(&id).map(|h| text(*h)).unwrap_or_default()
    }
    unsafe fn set(&self, id: u16, s: &str) {
        if let Some(h) = self.controls.get(&id) {
            SetWindowTextW(*h, wide(s).as_ptr());
        }
    }
    unsafe fn selection(&self, id: u16) -> Option<usize> {
        let s = SendMessageW(*self.controls.get(&id)?, LB_GETCURSEL, 0, 0);
        if s < 0 {
            None
        } else {
            Some(s as usize)
        }
    }
    unsafe fn rebuild(&mut self) {
        let visible = IsWindowVisible(self.hwnd) != 0;
        if visible {
            SendMessageW(self.hwnd, WM_SETREDRAW, 0, 0);
        }
        self.rebuilds += 1;
        self.hover = None;
        let mut area: RECT = zeroed();
        GetClientRect(self.hwnd, &mut area);
        self.layout_size = (area.right, area.bottom);
        for (_, h) in std::mem::take(&mut self.controls) {
            DestroyWindow(h);
        }
        self.renderer = None;
        let (width, _) = self.size();
        let right = (width - 458.).max(260.);
        if !self.mini {
            for (id, label, x) in [
                (10, "Luz", 32.),
                (11, "Presets", 134.),
                (15, "Cenas", 236.),
                (12, "Rotinas", 338.),
                (13, "Ajustes", 440.),
            ] {
                self.button(id, label, x, 62., 94., 36.);
            }
            self.button(14, "Recolher", width - 129., 27., 97., 30.);
        }
        match self.page {
            Page::Home => {
                let x = if self.mini { 24. } else { 48. };
                let top = if self.mini { 20. } else { 125. };
                let cw = if self.mini { width - 48. } else { 330. };
                self.button(
                    20,
                    if self.display.power {
                        "Desligar"
                    } else {
                        "Ligar"
                    },
                    x + cw - 90.,
                    top + 8.,
                    80.,
                    32.,
                );
                self.slider(
                    100,
                    "Brilho",
                    self.display.level(),
                    100,
                    x + 8.,
                    top + 204.,
                    cw - 16.,
                );
                self.button(21, "Branco", x + 4., top + 250., (cw - 12.) / 2., 32.);
                self.button(
                    22,
                    "Cor",
                    x + 8. + (cw - 12.) / 2.,
                    top + 250.,
                    (cw - 12.) / 2.,
                    32.,
                );
                if self.display.mode == "colour" {
                    self.slider(
                        101,
                        "Tonalidade",
                        self.display.hue,
                        360,
                        x + 8.,
                        top + 315.,
                        cw - 16.,
                    );
                    self.slider(
                        102,
                        "Vibração da cor",
                        self.display.saturation,
                        100,
                        x + 8.,
                        top + 372.,
                        cw - 16.,
                    );
                    self.button(23, "Escolher cor", x + cw - 100., top + 285., 95., 25.);
                } else {
                    self.slider(
                        101,
                        "Temperatura do branco",
                        self.display.temperature,
                        100,
                        x + 8.,
                        top + 325.,
                        cw - 16.,
                    );
                }
                let count = self.store.config.favorites.len().min(6);
                for i in 0..count {
                    self.button(
                        50 + i as u16,
                        &format!("Cor {}", i + 1),
                        x + 5. + i as f32 * 38.,
                        top + 424.,
                        30.,
                        25.,
                    );
                }
                self.button(24, "+", x + cw - 35., top + 423., 30., 28.);
                if self.mini {
                    self.button(25, "Timer", x, top + 465., 70., 30.);
                    self.button(26, "Abrir app", x + cw - 112., top + 465., 112., 30.);
                } else {
                    let rx = 414.;
                    for (i, p) in self.store.config.presets.clone().iter().take(4).enumerate() {
                        let col = (i % 2) as f32;
                        let row = (i / 2) as f32;
                        self.button(
                            30 + i as u16,
                            &p.name,
                            rx + col * (right / 2. + 4.),
                            177. + row * 57.,
                            right / 2. - 8.,
                            46.,
                        );
                    }
                    self.button(28, "Editar presets", rx, 299., right, 33.);
                    self.button(25, "Configurar timer", rx, 407., right, 34.);
                    self.button(29, "Gerenciar rotinas", rx, 512., right, 34.);
                    self.button(27, "Atualizar estado", rx, 558., right, 30.);
                }
            }
            Page::Settings => self.settings_controls(width),
            Page::Shortcuts => self.shortcut_controls(width),
            Page::Presets => {
                self.list(
                    300,
                    &self
                        .store
                        .config
                        .presets
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>(),
                    40.,
                    145.,
                    300.,
                    220.,
                );
                self.edit(306, "Meu preset", 40., 380., 300., false);
                self.button(301, "Aplicar", 40., 423., 140., 34.);
                self.button(302, "Salvar luz atual", 194., 423., 146., 34.);
                self.button(303, "Atualizar selecionado", 40., 469., 300., 34.);
                self.button(304, "Excluir selecionado", 40., 515., 300., 34.);
                self.list(
                    310,
                    &self.store.config.favorites.clone(),
                    390.,
                    145.,
                    width - 430.,
                    220.,
                );
                self.edit(311, &self.display.hex(), 390., 380., width - 430., false);
                self.button(317, "Escolher uma cor", 390., 380., width - 565., 32.);
                self.button(318, "Código…", width - 165., 380., 125., 32.);
                self.button(312, "Adicionar cor", 390., 423., width - 430., 34.);
                self.button(313, "Alterar selecionada", 390., 469., width - 430., 34.);
                self.button(314, "Excluir cor", 390., 515., width - 430., 34.);
                self.button(316, "Aplicar cor", 390., 560., width - 430., 30.);
            }
            Page::Routines => {
                self.list(
                    400,
                    &self
                        .store
                        .config
                        .routines
                        .iter()
                        .map(|r| {
                            format!(
                                "{}  {:02}:{:02}  {}",
                                if r.enabled { "●" } else { "○" },
                                r.hour,
                                r.minute,
                                r.name
                            )
                        })
                        .collect::<Vec<_>>(),
                    40.,
                    145.,
                    290.,
                    350.,
                );
                self.button(420, "Nova rotina", 40., 515., 138., 34.);
                self.button(422, "Excluir", 190., 515., 140., 34.);
                let x = 374.;
                self.label(450, "Nome", x, 145., 250.);
                self.edit(401, "Minha rotina", x, 170., width - x - 40., false);
                self.label(451, "Horário local · hora e minuto", x, 215., 300.);
                self.edit(402, "22", x, 240., 80., false);
                self.edit(403, "00", x + 95., 240., 80., false);
                self.label(452, "Dias da semana", x, 285., 300.);
                for (i, name) in ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"]
                    .iter()
                    .enumerate()
                {
                    self.button(410 + i as u16, name, x + i as f32 * 42., 310., 35., 31.);
                }
                self.label(453, "Ação", x, 360., 200.);
                let mut actions = vec!["Desligar".into(), "Usar ajustes atuais da luz".into()];
                actions.extend(self.store.config.presets.iter().map(|p| p.name.clone()));
                self.combo(405, &actions, 0, x, 386., width - x - 40.);
                self.button(417, "Ativada: sim", x, 438., width - x - 40., 32.);
                self.button(421, "Salvar rotina", x, 515., width - x - 40., 34.);
            }
            Page::Timer => {
                self.label(550, "Desligar depois de quantos minutos?", 48., 170., 500.);
                self.edit(500, &self.timer_minutes.clone(), 48., 204., 180., false);
                self.button(501, "Iniciar timer", 48., 258., 180., 38.);
                self.button(502, "Cancelar timer", 246., 258., 180., 38.);
                for (i, n) in [15, 30, 60, 120].iter().enumerate() {
                    self.button(
                        510 + i as u16,
                        &format!("{n} min"),
                        48. + i as f32 * 100.,
                        325.,
                        88.,
                        34.,
                    );
                }
            }
            Page::Scenes => {
                self.list(
                    700,
                    &self
                        .store
                        .config
                        .scenes
                        .iter()
                        .map(|s| s.name.clone())
                        .collect::<Vec<_>>(),
                    40.,
                    145.,
                    280.,
                    330.,
                );
                self.button(710, "Nova", 40., 495., 130., 34.);
                self.button(712, "Excluir", 185., 495., 135., 34.);
                self.button(714, "Retomar cena da lâmpada", 40., 548., 280., 34.);
                let x = 360.;
                let cw = width - x - 40.;
                self.label(750, "Nome da cena", x, 145., cw);
                self.edit(701, "Minha cena", x, 170., cw, false);
                self.label(751, "Efeito", x, 213., cw);
                self.combo(
                    702,
                    &[
                        "Estática".into(),
                        "Alternar cores".into(),
                        "Transição suave".into(),
                    ],
                    2,
                    x,
                    238.,
                    cw,
                );
                self.label(752, "Ritmo da transição", x, 283., cw / 2.);
                self.edit(703, "40", x, 308., cw / 2. - 10., false);
                self.label(753, "Intensidade (%)", x + cw / 2. + 10., 283., cw / 2.);
                self.edit(704, "50", x + cw / 2. + 10., 308., cw / 2. - 10., false);
                self.label(754, "Sua paleta · toque nas cores para editar", x, 358., cw);
                self.edit(705, "#FFB86B, #FF6B82, #B497FF", x, 383., cw, false);
                for i in 0..8 {
                    self.button(
                        760 + i,
                        &format!("Editar cor {}", i + 1),
                        x + i as f32 * 44.,
                        385.,
                        36.,
                        34.,
                    );
                }
                self.button(768, "+ Cor", x, 431., 90., 32.);
                self.button(769, "Remover última", x + 102., 431., 146., 32.);
                self.label(
                    755,
                    "A cena continua na lâmpada após fechar o app.",
                    x,
                    548.,
                    cw,
                );
                self.button(711, "Salvar cena", x, 495., cw / 2. - 10., 34.);
                self.button(
                    713,
                    "Aplicar cena",
                    x + cw / 2. + 10.,
                    495.,
                    cw / 2. - 10.,
                    34.,
                );
            }
        }
        for (&id, &child) in &self.controls {
            let hidden = (id == 311 && !self.color_advanced)
                || (id == 317 && self.color_advanced)
                || id == 705
                || (!self.connection_advanced
                    && self.page == Page::Settings
                    && [201, 202, 203, 204, 210, 601, 602, 603, 604].contains(&id));
            ShowWindow(child, if hidden { SW_HIDE } else { SW_SHOWNA });
        }
        if visible {
            SendMessageW(self.hwnd, WM_SETREDRAW, 1, 0);
        }
        RedrawWindow(
            self.hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_FRAME,
        );
        self.sync_animation();
    }
    unsafe fn reflow(&mut self) {
        let focus = GetDlgCtrlID(GetFocus());
        let fields: Vec<(u16, String)> = self
            .controls
            .iter()
            .filter(|(id, _)| {
                [
                    200, 201, 202, 203, 204, 205, 306, 311, 401, 402, 403, 500, 701, 703, 704, 705,
                ]
                .contains(id)
            })
            .map(|(id, h)| (*id, text(*h)))
            .collect();
        let selections: Vec<(u16, u32, isize)> = [
            (207, CB_GETCURSEL),
            (405, CB_GETCURSEL),
            (702, CB_GETCURSEL),
            (300, LB_GETCURSEL),
            (310, LB_GETCURSEL),
            (400, LB_GETCURSEL),
            (700, LB_GETCURSEL),
        ]
        .iter()
        .filter_map(|(id, msg)| {
            self.controls
                .get(id)
                .map(|h| (*id, *msg, SendMessageW(*h, *msg, 0, 0)))
        })
        .collect();
        self.rebuild();
        for (id, value) in fields {
            self.set(id, &value);
        }
        for (id, msg, selection) in selections {
            if let Some(h) = self.controls.get(&id) {
                SendMessageW(
                    *h,
                    if msg == CB_GETCURSEL {
                        CB_SETCURSEL
                    } else {
                        LB_SETCURSEL
                    },
                    selection as usize,
                    0,
                );
            }
        }
        if let Some(h) = self.controls.get(&(focus as u16)) {
            SetFocus(*h);
        }
    }
    unsafe fn settings_controls(&mut self, width: f32) {
        let x = 40.;
        let rx = width / 2. + 20.;
        let col = width / 2. - 60.;
        for (id, label, y) in [
            (600, "Nome da lâmpada", 139.),
            (601, "IP local (IPv4)", 213.),
            (602, "ID do dispositivo", 287.),
            (603, "Nova local_key (vazio mantém a atual)", 361.),
        ] {
            self.label(id, label, x, y, col);
        }
        self.edit(200, &self.store.config.name.clone(), x, 164., col, false);
        let c = self.store.credentials().ok().flatten();
        self.edit(
            201,
            c.as_ref().map(|c| c.ip.as_str()).unwrap_or(""),
            x,
            238.,
            col,
            false,
        );
        self.edit(
            202,
            c.as_ref().map(|c| c.id.as_str()).unwrap_or(""),
            x,
            312.,
            col,
            false,
        );
        self.edit(203, "", x, 386., col, true);
        self.label(
            604,
            "Módulo tuya_manager.py ou pasta",
            x,
            435.,
            col,
        );
        self.edit(
            204,
            "",
            x,
            460.,
            col,
            false,
        );
        self.button(210, "Importar controle Tuya", x, 507., col, 34.);
        for (id, label, enabled, y) in [
            (
                212,
                "Iniciar recolhido",
                self.store.config.start_hidden,
                139.,
            ),
            (
                213,
                "Iniciar com o Windows",
                self.store.config.autostart,
                182.,
            ),
            (
                214,
                "Fixar painel da bandeja",
                self.store.config.pinned,
                225.,
            ),
            (
                215,
                "Fechar recolhe para bandeja",
                self.store.config.close_to_tray,
                268.,
            ),
        ] {
            self.button(
                id,
                &format!("{label}: {}", if enabled { "sim" } else { "não" }),
                rx,
                y,
                col,
                32.,
            );
        }
        self.label(605, "Transição suave (segundos)", rx, 318., col);
        self.edit(
            205,
            &(self.store.config.transition_ms as f64 / 1000.).to_string(),
            rx,
            343.,
            col,
            false,
        );
        self.button(220, "Personalizar atalhos…", rx, 397., col, 44.);
        self.button(
            218,
            if self.connection_advanced {
                "Ocultar conexão avançada"
            } else {
                "Conexão da lâmpada…"
            },
            x,
            if self.connection_advanced { 557. } else { 218. },
            col,
            32.,
        );
        self.button(216, "Reconectar", rx, 465., col / 2. - 5., 32.);
        self.button(
            217,
            "Buscar na LAN",
            rx + col / 2. + 5.,
            465.,
            col / 2. - 5.,
            32.,
        );
        self.button(211, "Salvar configurações", rx, 507., col, 34.);
        if self.snapshot.capabilities.contains("34") {
            self.button(
                219,
                if self.snapshot.do_not_disturb {
                    "Não perturbe após falta de energia: sim"
                } else {
                    "Não perturbe após falta de energia: não"
                },
                rx,
                557.,
                col,
                30.,
            );
        }
    }
    unsafe fn resize_mode(&mut self, mini: bool) {
        if self.page == Page::Timer {
            self.timer_minutes = self.get(500);
        }
        self.mini = mini;
        self.page = Page::Home;
        self.renderer = None;
        let style = if mini {
            WS_POPUP | WS_BORDER | WS_CLIPCHILDREN
        } else {
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN
        };
        SetWindowLongPtrW(self.hwnd, GWL_STYLE, style as isize);
        SetWindowLongPtrW(
            self.hwnd,
            GWL_EXSTYLE,
            (WS_EX_CONTROLPARENT | WS_EX_COMPOSITED | if mini { WS_EX_TOOLWINDOW } else { 0 })
                as isize,
        );
        let mut r = RECT {
            left: 0,
            top: 0,
            right: (if mini { 366. } else { 836. } * self.dpi) as i32,
            bottom: (if mini { 565. } else { 654. } * self.dpi) as i32,
        };
        AdjustWindowRectExForDpi(
            &mut r,
            style,
            0,
            if mini { WS_EX_TOOLWINDOW } else { 0 },
            (96. * self.dpi) as u32,
        );
        let (x, y);
        if mini {
            let id = NOTIFYICONIDENTIFIER {
                cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..zeroed()
            };
            let mut tray: RECT = zeroed();
            let mut p = POINT { x: 0, y: 0 };
            if Shell_NotifyIconGetRect(&id, &mut tray) >= 0 {
                p.x = tray.right;
                p.y = tray.top;
            } else {
                GetCursorPos(&mut p);
            }
            let monitor = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..zeroed()
            };
            GetMonitorInfoW(monitor, &mut info);
            x = (p.x - (r.right - r.left))
                .max(info.rcWork.left)
                .min((info.rcWork.right - (r.right - r.left)).max(info.rcWork.left));
            y = (p.y - (r.bottom - r.top) - 10)
                .max(info.rcWork.top)
                .min((info.rcWork.bottom - (r.bottom - r.top)).max(info.rcWork.top));
        } else {
            let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..zeroed()
            };
            GetMonitorInfoW(monitor, &mut info);
            x = info.rcWork.left + (info.rcWork.right - info.rcWork.left - (r.right - r.left)) / 2;
            y = info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - (r.bottom - r.top)) / 2;
        }
        SetWindowPos(
            self.hwnd,
            if mini { HWND_TOPMOST } else { HWND_NOTOPMOST },
            x,
            y,
            r.right - r.left,
            r.bottom - r.top,
            SWP_FRAMECHANGED | SWP_NOACTIVATE,
        );
    }
    unsafe fn show(&mut self, mini: bool) {
        self.opened_at = Instant::now();
        self.resize_mode(mini);
        self.rebuild();
        ShowWindow(self.hwnd, SW_SHOW);
        SetForegroundWindow(self.hwnd);
        self.sync_animation();
        if mini {
            SetTimer(self.hwnd, 5, 450, None);
        }
    }
    unsafe fn hide(&mut self) {
        KillTimer(self.hwnd, ANIMATION_TIMER);
        ShowWindow(self.hwnd, SW_HIDE);
        self.renderer = None;
        self.widgets = None;
        self.graphics = None;
    }
    unsafe fn page(&mut self, page: Page) {
        if self.mini {
            self.resize_mode(false);
        }
        if self.controls.contains_key(&500) {
            self.timer_minutes = self.get(500);
        }
        self.cancel_capture();
        self.page = page;
        if page == Page::Shortcuts {
            self.shortcuts = self.store.config.shortcuts();
        }
        self.notice.clear();
        self.rebuild();
        ShowWindow(self.hwnd, SW_SHOW);
        self.sync_animation();
    }
    unsafe fn add_tray(&self) {
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY;
        data.hIcon = self.icon;
        let tip = wide("LightControl · lâmpada do quarto");
        data.szTip[..tip.len()].copy_from_slice(&tip);
        Shell_NotifyIconW(NIM_ADD, &data);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &data);
    }
    unsafe fn remove_tray(&self) {
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            ..zeroed()
        };
        Shell_NotifyIconW(NIM_DELETE, &data);
    }
    unsafe fn register_hotkey(&mut self) {
        if let Err(e) = self.install_shortcuts(&self.store.config.shortcuts()) {
            self.notice = e;
        }
    }
    unsafe fn tray_menu(&mut self) {
        let menu = CreatePopupMenu();
        for (id, label) in [
            (26, "Abrir LightControl"),
            (
                20,
                if self.display.power {
                    "Desligar luz"
                } else {
                    "Ligar luz"
                },
            ),
            (25, "Timer para desligar"),
            (13, "Configurações"),
        ] {
            AppendMenuW(menu, MF_STRING, id, wide(label).as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        for (i, p) in self.store.config.presets.iter().take(20).enumerate() {
            AppendMenuW(menu, MF_STRING, 1000 + i, wide(&p.name).as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, 99, wide("Sair").as_ptr());
        let mut p: POINT = zeroed();
        GetCursorPos(&mut p);
        SetForegroundWindow(self.hwnd);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            p.x,
            p.y,
            0,
            self.hwnd,
            null(),
        );
        DestroyMenu(menu);
        if id != 0 {
            self.action(id as u16);
        }
        PostMessageW(self.hwnd, WM_NULL, 0, 0);
    }
    unsafe fn update(&mut self) {
        self.updates += 1;
        if let Ok(s) = self.worker.state.lock() {
            self.snapshot = s.clone();
        }
        if self.snapshot.light.power || self.pending_stage == self.snapshot.staged {
            self.pending_stage = None;
        }
        if !self.dragging && self.pending_send.is_none() && !self.snapshot.pending {
            self.display = if !self.snapshot.light.power {
                self.pending_stage
                    .clone()
                    .or_else(|| self.snapshot.staged.clone())
                    .unwrap_or_else(|| self.snapshot.light.clone())
            } else {
                self.snapshot.light.clone()
            };
        }
        if self.snapshot.online && !self.snapshot.pending {
            if self.snapshot.countdown > 0 && self.snapshot.light.power {
                self.timer_native = true;
                self.timer = self.snapshot.countdown_deadline;
                SetTimer(self.hwnd, CLOCK_TIMER, 1000, None);
            } else if self.timer_native {
                self.timer_native = false;
                self.timer = None;
                KillTimer(self.hwnd, CLOCK_TIMER);
                if self.snapshot.countdown > 0 {
                    self.send(Command::Set(
                        serde_json::json!({"26":0}).as_object().unwrap().clone(),
                        Instant::now(),
                    ));
                }
            }
        }
        if self.page == Page::Home {
            let color_control = self.controls.contains_key(&102);
            if color_control != (self.display.mode == "colour") {
                self.rebuild();
            } else {
                self.set(
                    20,
                    if self.display.power {
                        "Desligar"
                    } else {
                        "Ligar"
                    },
                );
                for (id, value) in [
                    (100, self.display.level()),
                    (
                        101,
                        if color_control {
                            self.display.hue
                        } else {
                            self.display.temperature
                        },
                    ),
                    (102, self.display.saturation),
                ] {
                    if let Some(h) = self.controls.get(&id) {
                        if !self.dragging {
                            SendMessageW(*h, TBM_SETPOS, 0, value as isize);
                            InvalidateRect(*h, null(), 0);
                        }
                    }
                }
                self.set(23, "Escolher cor");
                self.invalidate();
            }
        } else {
            if self.controls.contains_key(&219) {
                self.set(
                    219,
                    if self.snapshot.do_not_disturb {
                        "Não perturbe após falta de energia: sim"
                    } else {
                        "Não perturbe após falta de energia: não"
                    },
                );
            }
            self.invalidate();
        }
        self.sync_animation();
    }
    unsafe fn commit(&mut self, mut d: crate::model::Dps) {
        self.notice.clear();
        if !self.snapshot.online && !self.preview {
            self.notice = "Lâmpada offline. Verifique a conexão em Ajustes.".into();
            self.display = self.snapshot.light.clone();
            self.invalidate();
            return;
        }
        if d.get("20").and_then(|v| v.as_bool()) == Some(false) && self.timer_native {
            d.insert("26".into(), serde_json::json!(0));
            self.timer_native = false;
            self.timer = None;
            KillTimer(self.hwnd, CLOCK_TIMER);
        }
        self.display.merge(&d);
        self.send(Command::Set(d, Instant::now()));
        self.invalidate();
    }
    unsafe fn change_light(&mut self) {
        if !self.display.power {
            self.pending_send = None;
            self.pending_stage = Some(self.display.clone());
            self.send(Command::Stage(self.display.clone()));
            self.notice = "Ajustes preparados para quando você ligar.".into();
            self.invalidate();
        } else {
            self.commit(self.display.as_dps());
        }
    }
    unsafe fn slider_changed(&mut self, id: u16, code: u16) {
        let Some(h) = self.controls.get(&id) else {
            return;
        };
        let value = SendMessageW(*h, TBM_GETPOS, 0, 0) as u16;
        InvalidateRect(*h, null(), 0);
        if id == 100 {
            if self.display.mode == "colour" {
                self.display.value = value;
            } else {
                self.display.brightness = value;
            }
        } else if id == 101 {
            if self.display.mode == "colour" {
                self.display.hue = value;
            } else {
                self.display.temperature = value;
            }
        } else if id == 102 {
            self.display.saturation = value;
        }
        if !self.display.power {
            self.change_light();
            return;
        }
        self.pending_send = Some(self.display.as_dps());
        self.dragging = code != TB_ENDTRACK as u16 && code != TB_THUMBPOSITION as u16;
        if !self.dragging
            || self
                .last_slider
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(120))
        {
            if let Some(d) = self.pending_send.take() {
                self.commit(d);
            }
            self.last_slider = Some(Instant::now());
        }
        SetTimer(self.hwnd, SLIDER_TIMER, 140, None);
        self.set(23, "Escolher cor");
        self.invalidate();
    }
    unsafe fn persist(&mut self) {
        match self.store.save() {
            Ok(()) => {
                self.send(Command::Config(self.store.config.clone()));
                self.notice = "Salvo neste computador".into();
            }
            Err(e) => self.notice = e,
        }
        self.invalidate();
    }
    unsafe fn apply_preset(&mut self, index: usize) {
        if let Some(p) = self.store.config.presets.get(index) {
            if self.snapshot.online || self.preview {
                self.send(Command::Preset(
                    p.light.clone(),
                    self.store.config.transition_ms,
                    Instant::now(),
                ));
            } else {
                self.notice = "Lâmpada offline; preset não enviado.".into();
                self.invalidate();
            }
        }
    }
    unsafe fn action(&mut self, id: u16) {
        match id {
            10 => self.page(Page::Home),
            11 | 28 => self.page(Page::Presets),
            12 | 29 => self.page(Page::Routines),
            13 => self.page(Page::Settings),
            14 => self.hide(),
            15 => self.page(Page::Scenes),
            20 => {
                if !self.snapshot.online && !self.preview {
                    self.notice = "Lâmpada offline. Verifique a conexão em Ajustes.".into();
                    self.invalidate();
                    return;
                }
                self.notice.clear();
                self.pending_send = None;
                self.send(Command::Power(Some(!self.display.power), Instant::now()));
            }
            21 | 22 => {
                self.display.mode = if id == 21 {
                    "white".into()
                } else {
                    "colour".into()
                };
                self.change_light();
                self.rebuild();
            }
            23 => {
                if let Some(hex) = self.choose_color(&self.display.hex()) {
                    if self.display.set_hex(&hex).is_ok() {
                        self.change_light();
                        self.rebuild();
                    }
                }
            }
            24 => {
                let hex = self.display.hex();
                if !self.store.config.favorites.contains(&hex) {
                    self.store.config.favorites.push(hex);
                    self.persist();
                    self.rebuild();
                }
            }
            25 => self.page(Page::Timer),
            26 => self.show(false),
            27 => self.send(Command::Refresh),
            30..=33 => self.apply_preset((id - 30) as usize),
            50..=55 => {
                if let Some(hex) = self.store.config.favorites.get((id - 50) as usize).cloned() {
                    if self.display.set_hex(&hex).is_ok() {
                        self.change_light();
                        self.rebuild();
                    }
                }
            }
            99 => {
                DestroyWindow(self.hwnd);
            }
            210 => {
                let path = self.get(204);
                match self.store.import_tuya_manager(std::path::Path::new(&path)) {
                    Ok(()) => {
                        self.send(Command::Reconnect);
                        self.rebuild();
                        self.notice = "Controle Tuya importado e protegido pelo Windows.".into();
                    }
                    Err(e) => self.notice = e,
                }
                self.invalidate();
            }
            211 => {
                if let Err(e) = self.save_settings() {
                    self.notice = e;
                }
                self.invalidate();
            }
            212..=215 => {
                let (label, value) = match id {
                    212 => {
                        self.store.config.start_hidden = !self.store.config.start_hidden;
                        ("Iniciar recolhido", self.store.config.start_hidden)
                    }
                    213 => {
                        self.store.config.autostart = !self.store.config.autostart;
                        ("Iniciar com o Windows", self.store.config.autostart)
                    }
                    214 => {
                        self.store.config.pinned = !self.store.config.pinned;
                        ("Fixar painel da bandeja", self.store.config.pinned)
                    }
                    _ => {
                        self.store.config.close_to_tray = !self.store.config.close_to_tray;
                        (
                            "Fechar recolhe para bandeja",
                            self.store.config.close_to_tray,
                        )
                    }
                };
                self.set(
                    id,
                    &format!("{label}: {}", if value { "sim" } else { "não" }),
                );
                self.notice = "Clique em Salvar configurações para concluir.".into();
                self.invalidate();
            }
            218 => {
                self.connection_advanced = !self.connection_advanced;
                self.reflow();
            }
            220 => self.page(Page::Shortcuts),
            820..=825 => self.begin_capture((id - 820) as usize),
            830 => self.save_shortcuts(),
            831 => {
                self.cancel_capture();
                self.shortcuts = self.store.config.shortcuts();
                self.rebuild();
            }
            216 => self.send(Command::Reconnect),
            217 => self.send(Command::Discover),
            219 => self.commit(
                serde_json::json!({"34":!self.snapshot.do_not_disturb})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            301 => {
                if let Some(i) = self.selection(300) {
                    self.apply_preset(i);
                }
            }
            302 => {
                let name = self.get(306).trim().to_string();
                if name.is_empty() {
                    self.notice = "Dê um nome ao preset.".into();
                } else {
                    self.store.config.presets.push(Preset {
                        name,
                        light: self.display.clone(),
                    });
                    self.persist();
                    self.rebuild();
                }
            }
            303 => {
                if let Some(i) = self.selection(300) {
                    let name = self.get(306).trim().to_string();
                    if !name.is_empty() {
                        self.store.config.presets[i] = Preset {
                            name,
                            light: self.display.clone(),
                        };
                        self.persist();
                        self.rebuild();
                    }
                }
            }
            304 => {
                if let Some(i) = self.selection(300) {
                    self.store.config.presets.remove(i);
                    self.persist();
                    self.rebuild();
                }
            }
            312 | 313 => {
                let hex = self.get(311);
                let mut light = Light::default();
                match light.set_hex(&hex) {
                    Ok(()) => {
                        let exact_hex = format!(
                            "#{}",
                            hex.trim().trim_start_matches('#').to_ascii_uppercase()
                        );
                        if id == 312 {
                            self.store.config.favorites.push(exact_hex);
                        } else if let Some(i) = self.selection(310) {
                            self.store.config.favorites[i] = exact_hex;
                        }
                        self.persist();
                        self.rebuild();
                    }
                    Err(e) => self.notice = e,
                }
            }
            317 => {
                if let Some(hex) = self.choose_color(&self.get(311)) {
                    self.set(311, &hex);
                    if let Some(h) = self.controls.get(&317) {
                        InvalidateRect(*h, null(), 0);
                    }
                }
            }
            318 => {
                self.color_advanced = !self.color_advanced;
                self.reflow();
            }
            314 => {
                if let Some(i) = self.selection(310) {
                    self.store.config.favorites.remove(i);
                    self.persist();
                    self.rebuild();
                }
            }
            316 => {
                let hex = self.get(311);
                match self.display.set_hex(&hex) {
                    Ok(()) => {
                        self.change_light();
                    }
                    Err(e) => self.notice = e,
                }
                self.invalidate();
            }
            410..=416 => {
                self.editing_days ^= 1 << (id - 410);
                InvalidateRect(self.controls[&id], null(), 0);
            }
            417 => {
                let enabled = !self.get(417).ends_with("sim");
                self.set(
                    417,
                    if enabled {
                        "Ativada: sim"
                    } else {
                        "Ativada: não"
                    },
                );
            }
            420 => {
                SendMessageW(self.controls[&400], LB_SETCURSEL, usize::MAX, 0);
                self.editing_days = 127;
                self.set(401, "Minha rotina");
                self.set(402, "22");
                self.set(403, "00");
                self.set(417, "Ativada: sim");
                self.invalidate();
            }
            421 => {
                if let Err(e) = self.save_routine() {
                    self.notice = e;
                }
                self.invalidate();
            }
            422 => {
                if let Some(i) = self.selection(400) {
                    self.store.config.routines.remove(i);
                    self.persist();
                    self.rebuild();
                }
            }
            501 => {
                match self.get(500).trim().parse::<u64>() {
                    Ok(minutes) if (1..=1440).contains(&minutes) => {
                        if !self.snapshot.online && !self.preview {
                            self.notice = "Conecte a lâmpada antes de iniciar o timer.".into();
                        } else {
                            self.timer_minutes = minutes.to_string();
                            let deadline = protocol::epoch() + minutes * 60;
                            self.timer = Some(deadline);
                            if self.snapshot.capabilities.contains("26") && self.display.power {
                                self.timer_native = true;
                                self.commit(
                                    serde_json::json!({"26":minutes*60})
                                        .as_object()
                                        .unwrap()
                                        .clone(),
                                );
                                self.send(Command::Timer(None));
                            } else {
                                self.timer_native = false;
                                self.send(Command::Timer(Some(deadline)));
                            }
                            SetTimer(self.hwnd, CLOCK_TIMER, 1000, None);
                            self.notice = "Timer ativado".into();
                        }
                    }
                    _ => self.notice = "Use entre 1 e 1440 minutos.".into(),
                }
                self.invalidate();
            }
            502 => {
                self.timer = None;
                self.timer_native = false;
                self.send(Command::Timer(None));
                if self.snapshot.capabilities.contains("26") {
                    self.commit(serde_json::json!({"26":0}).as_object().unwrap().clone());
                }
                KillTimer(self.hwnd, CLOCK_TIMER);
                self.notice = "Timer cancelado".into();
                self.invalidate();
            }
            510..=513 => {
                self.set(500, &[15, 30, 60, 120][(id - 510) as usize].to_string());
                self.action(501);
            }
            710 => {
                SendMessageW(self.controls[&700], LB_SETCURSEL, usize::MAX, 0);
                self.set(701, "Minha cena");
                self.set(705, &self.store.config.favorites.join(", "));
                self.invalidate_palette();
            }
            711 | 713 => {
                match self.scene_from_fields() {
                    Ok(scene) => {
                        if id == 711 {
                            if let Some(i) = self.selection(700) {
                                self.store.config.scenes[i] = scene;
                            } else {
                                self.store.config.scenes.push(scene);
                            }
                            self.persist();
                            self.rebuild();
                        } else if self.snapshot.capabilities.contains("25") || self.preview {
                            if let Ok(payload) = scene.payload() {
                                self.commit(
                                    serde_json::json!({"20":true,"21":"scene","25":payload})
                                        .as_object()
                                        .unwrap()
                                        .clone(),
                                );
                            }
                        } else {
                            self.notice = "A lâmpada ainda não reportou suporte a cenas.".into();
                        }
                    }
                    Err(e) => self.notice = e,
                }
                self.invalidate();
            }
            712 => {
                if let Some(i) = self.selection(700) {
                    self.store.config.scenes.remove(i);
                    self.persist();
                    self.rebuild();
                }
            }
            760..=767 => {
                let mut colors = self.scene_colors();
                let i = (id - 760) as usize;
                if let Some(current) = colors.get(i).cloned() {
                    if let Some(hex) = self.choose_color(&current) {
                        colors[i] = hex;
                        self.set(705, &colors.join(", "));
                        self.invalidate_palette();
                    }
                }
            }
            768 => {
                let mut colors = self.scene_colors();
                if colors.len() < 8 {
                    if let Some(hex) = self.choose_color("#D4E4B8") {
                        colors.push(hex);
                        self.set(705, &colors.join(", "));
                        self.invalidate_palette();
                    }
                } else {
                    self.notice = "A paleta já tem oito cores.".into();
                    self.invalidate();
                }
            }
            769 => {
                let mut colors = self.scene_colors();
                if colors.len() > 1 {
                    colors.pop();
                    self.set(705, &colors.join(", "));
                    self.invalidate_palette();
                }
            }
            714 => {
                if !self.snapshot.scene.is_empty() {
                    self.commit(
                        serde_json::json!({"20":true,"21":"scene","25":self.snapshot.scene})
                            .as_object()
                            .unwrap()
                            .clone(),
                    );
                }
            }
            1000..=1019 => self.apply_preset((id - 1000) as usize),
            _ => {}
        }
    }
    unsafe fn save_settings(&mut self) -> Result<(), String> {
        let mut config = self.store.config.clone();
        config.name = self.get(200).trim().to_string();
        if config.name.is_empty() {
            return Err("Escolha um nome para a lâmpada.".into());
        }
        let seconds: f64 = self
            .get(205)
            .trim()
            .replace(',', ".")
            .parse()
            .map_err(|_| "Informe a duração em segundos.")?;
        if !seconds.is_finite() || !(0.0..=30.0).contains(&seconds) {
            return Err("Escolha uma transição entre 0 e 30 segundos.".into());
        }
        config.transition_ms = (seconds * 1000.).round() as u32;
        let existing = self.store.credentials()?;
        let key = if self.get(203).is_empty() {
            existing.as_ref().map(|c| c.key.clone()).unwrap_or_default()
        } else {
            self.get(203)
        };
        let c = Credentials {
            id: self.get(202).trim().into(),
            ip: self.get(201).trim().into(),
            key,
        };
        self.store.set_credentials(&c)?;
        config.last_ip = c.ip.clone();
        self.store.config = config;
        autostart(self.store.config.autostart, &self.store.root)?;
        self.store.save()?;
        self.register_hotkey();
        self.send(Command::Config(self.store.config.clone()));
        self.send(Command::Reconnect);
        self.set(203, "");
        SetWindowTextW(
            self.hwnd,
            wide(&format!("LightControl · {}", self.store.config.name)).as_ptr(),
        );
        self.notice = "Configurações salvas. Reconectando…".into();
        Ok(())
    }
    unsafe fn save_routine(&mut self) -> Result<(), String> {
        let name = self.get(401).trim().to_owned();
        let hour: u16 = self.get(402).trim().parse().map_err(|_| "Hora inválida.")?;
        let minute: u16 = self
            .get(403)
            .trim()
            .parse()
            .map_err(|_| "Minuto inválido.")?;
        if name.is_empty() || hour > 23 || minute > 59 || self.editing_days == 0 {
            return Err("Informe nome, horário válido e pelo menos um dia.".into());
        }
        let index = self.selection(400);
        let action = SendMessageW(self.controls[&405], CB_GETCURSEL, 0, 0);
        let light = if action == 0 {
            Light {
                power: false,
                ..self.display.clone()
            }
        } else if action == 1 {
            self.display.clone()
        } else if action >= 2 {
            self.store
                .config
                .presets
                .get(action as usize - 2)
                .map(|p| p.light.clone())
                .unwrap_or_else(|| self.display.clone())
        } else {
            index
                .and_then(|i| self.store.config.routines.get(i))
                .map(|r| r.light.clone())
                .unwrap_or_else(|| self.display.clone())
        };
        let routine = Routine {
            name,
            hour,
            minute,
            days: self.editing_days,
            enabled: self.get(417).ends_with("sim"),
            light,
        };
        if let Some(i) = index {
            self.store.config.routines[i] = routine;
        } else {
            self.store.config.routines.push(routine);
        }
        self.persist();
        self.rebuild();
        Ok(())
    }
    unsafe fn selected(&mut self, id: u16) {
        match id {
            300 => {
                if let Some(i) = self.selection(300) {
                    if let Some(p) = self.store.config.presets.get(i) {
                        self.set(306, &p.name);
                    }
                }
            }
            310 => {
                if let Some(i) = self.selection(310) {
                    if let Some(s) = self.store.config.favorites.get(i) {
                        self.set(311, s);
                        if let Some(h) = self.controls.get(&317) {
                            InvalidateRect(*h, null(), 0);
                        }
                    }
                }
            }
            400 => {
                if let Some(i) = self.selection(400) {
                    if let Some(r) = self.store.config.routines.get(i) {
                        self.set(401, &r.name);
                        self.set(402, &format!("{:02}", r.hour));
                        self.set(403, &format!("{:02}", r.minute));
                        self.set(
                            417,
                            if r.enabled {
                                "Ativada: sim"
                            } else {
                                "Ativada: não"
                            },
                        );
                        self.editing_days = r.days;
                        SendMessageW(self.controls[&405], CB_SETCURSEL, usize::MAX, 0);
                        self.invalidate();
                    }
                }
            }
            _ => {}
        }
    }
    unsafe fn scene_from_fields(&self) -> Result<Scene, String> {
        let scene = Scene {
            name: self.get(701).trim().into(),
            effect: SendMessageW(self.controls[&702], CB_GETCURSEL, 0, 0) as u8,
            speed: self
                .get(703)
                .trim()
                .parse()
                .map_err(|_| "Velocidade inválida.")?,
            brightness: self
                .get(704)
                .trim()
                .parse()
                .map_err(|_| "Brilho inválido.")?,
            colors: self
                .get(705)
                .split(',')
                .map(|s| s.trim().to_owned())
                .collect(),
        };
        if scene.name.is_empty() {
            return Err("Dê um nome à cena.".into());
        }
        scene.payload()?;
        Ok(scene)
    }
    unsafe fn paint(&mut self) {
        self.paints += 1;
        if !self.animations {
            self.glow = self.target_glow();
        }
        let mut ps: PAINTSTRUCT = zeroed();
        BeginPaint(self.hwnd, &mut ps);
        let (w, h) = self.size();
        if self.renderer.is_none() {
            if self.graphics.is_none() {
                self.graphics = Graphics::new().ok();
            }
            self.renderer = self
                .graphics
                .as_ref()
                .and_then(|g| Renderer::new(g, 96. * self.dpi).ok());
        }
        if let Some(r) = &self.renderer {
            if !r.begin(ps.hdc, self.layout_size.0, self.layout_size.1) {
                EndPaint(self.hwnd, &ps);
                self.renderer = None;
                return;
            }
            if !self.mini {
                r.text("lightcontrol", 32., 20., 250., 3, TEXT);
                r.text("Seu espaço em outra luz.", 224., 30., 300., 0, MUTED);
                r.round(27., 57., 512., 46., 13., 0x151b16);
                r.line(32., 110., w - 32., 110., 0x293529, 1.);
            }
            match self.page {
                Page::Home => {
                    let x = if self.mini { 24. } else { 48. };
                    let top = if self.mini { 20. } else { 125. };
                    let cw = if self.mini { w - 48. } else { 330. };
                    r.round(
                        x - 16.,
                        top - 7.,
                        cw + 32.,
                        if self.mini { 515. } else { 484. },
                        18.,
                        CARD,
                    );
                    r.text(
                        &self.store.config.name,
                        x + 5.,
                        top + 12.,
                        cw - 100.,
                        1,
                        TEXT,
                    );
                    r.text(
                        if !self.snapshot.known {
                            "Preparando seu ambiente…"
                        } else if !self.display.power {
                            "Luz apagada"
                        } else if self.display.mode == "colour" {
                            "Um toque de cor"
                        } else if self.display.mode == "scene" {
                            "Cores em movimento"
                        } else if self.display.temperature < 35 {
                            "Branco aconchegante"
                        } else if self.display.temperature > 70 {
                            "Branco para focar"
                        } else {
                            "Branco equilibrado"
                        },
                        x + 5.,
                        top + 36.,
                        cw - 20.,
                        0,
                        MUTED,
                    );
                    let tint = if self.display.mode == "colour" {
                        let c = self.display.rgb();
                        (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
                    } else {
                        mix(0xffbb79, 0xd1e6f3, self.display.temperature as f32 / 100.)
                    };
                    r.bulb(
                        x + cw / 2.,
                        top + 104.,
                        self.glow,
                        tint,
                        if self.animations {
                            self.phase_start.elapsed().as_secs_f32() * 1.4
                        } else {
                            0.
                        },
                    );
                    let level = if self.snapshot.known {
                        format!("{}", self.display.level())
                    } else {
                        "—".into()
                    };
                    r.text(&level, x + 8., top + 130., 150., 4, TEXT);
                    r.text(
                        "%",
                        x + if level.len() > 2 { 115. } else { 85. },
                        top + 160.,
                        60.,
                        2,
                        MUTED,
                    );
                    if self.display.mode != "colour" {
                        r.text("Quente", x + 8., top + 365., 100., 0, MUTED);
                        r.text("Frio", x + cw - 38., top + 365., 60., 0, MUTED);
                    }
                    r.text("SUA PALETA", x + 8., top + 402., 200., 0, MUTED);
                    if !self.mini {
                        let rx = 414.;
                        r.text("Qual é o clima de agora?", rx, 128., w - rx - 30., 3, TEXT);
                        r.text("UM TOQUE PARA MUDAR", rx, 157., w - rx - 30., 0, MUTED);
                        r.round(rx - 12., 356., w - rx - 20., 97., 12., CARD);
                        r.text("Timer para desligar", rx, 366., w - rx - 30., 2, TEXT);
                        r.text(&self.timer_label(), rx, 390., w - rx - 30., 0, MUTED);
                        r.text("Rotinas", rx, 465., w - rx - 30., 2, TEXT);
                        r.text(
                            &format!(
                                "{} rotinas ativas neste computador",
                                self.store
                                    .config
                                    .routines
                                    .iter()
                                    .filter(|r| r.enabled)
                                    .count()
                            ),
                            rx,
                            486.,
                            w - rx - 30.,
                            0,
                            MUTED,
                        );
                    }
                }
                Page::Settings => {
                    r.text("Do seu jeito", 40., 116., w - 80., 1, MUTED);
                    if !self.connection_advanced {
                        r.round(40., 275., w / 2. - 60., 270., 16., CARD);
                        r.text("Tudo acontece aqui.", 60., 299., w / 2. - 90., 3, TEXT);
                        r.text(
                            "Controle direto pela sua rede.",
                            60.,
                            342.,
                            w / 2. - 90.,
                            1,
                            MUTED,
                        );
                        r.text(
                            "Sem precisar de uma conta aberta.",
                            60.,
                            368.,
                            w / 2. - 90.,
                            1,
                            MUTED,
                        );
                        r.text(
                            "Os dados da lâmpada ficam",
                            60.,
                            423.,
                            w / 2. - 90.,
                            1,
                            MUTED,
                        );
                        r.text(
                            "protegidos neste computador.",
                            60.,
                            449.,
                            w / 2. - 90.,
                            1,
                            MUTED,
                        );
                    }
                }
                Page::Presets => {
                    r.text("Seus momentos", 40., 114., 340., 0, MUTED);
                    r.text("Cores que você gosta", 390., 114., w - 430., 0, MUTED);
                }
                Page::Routines => {
                    r.text("Uma luz no seu ritmo", 40., 114., w - 80., 0, MUTED);
                    r.text(
                        "Exigem o app aberto e o PC acordado. Horários perdidos não são repetidos.",
                        40.,
                        568.,
                        w - 80.,
                        0,
                        MUTED,
                    );
                }
                Page::Shortcuts => {
                    r.text("A luz na ponta dos dedos", 40., 119., w - 80., 3, TEXT);
                    r.text(
                        "Os comandos continuam ativos com o app na bandeja.",
                        40.,
                        149.,
                        w - 80.,
                        0,
                        MUTED,
                    );
                    for (i, (_, name)) in SHORTCUT_ACTIONS.iter().enumerate() {
                        r.round(40., 167. + i as f32 * 57., w - 80., 51., 11., CARD);
                        r.text(name, 58., 185. + i as f32 * 57., w - 430., 1, TEXT);
                    }
                    r.text(
                        "Clique em uma combinação e pressione as teclas. Delete remove o atalho.",
                        40.,
                        518.,
                        w - 80.,
                        0,
                        MUTED,
                    );
                }
                Page::Timer => {
                    r.text("TIMER PARA DESLIGAR", 48., 129., w - 96., 3, TEXT);
                    r.text(&self.timer_label(), 48., 402., w - 96., 2, ACCENT);
                    r.text(if self.timer_native||self.timer.is_none()&&self.snapshot.capabilities.contains("26")&&self.display.power{"Timer executado pela lâmpada, mesmo após fechar o aplicativo."}else{"Timer executado por este PC. Se ele suspender, a ação ocorre ao retomar."},48.,448.,w-96.,1,MUTED);
                }
                Page::Scenes => {
                    r.text("Cores em movimento", 40., 114., w - 80., 0, MUTED);
                }
            }
            if !self.mini {
                r.line(32., h - 48., w - 32., h - 48., 0x272922, 1.);
            }
            let msg = if !self.notice.is_empty() {
                self.notice.as_str()
            } else if self.preview {
                "Prévia visual"
            } else if self.snapshot.online {
                if self.snapshot.pending {
                    "Ajustando a luz…"
                } else if self.snapshot.message == "Conectada pela rede local" {
                    "●  Conectada pela rede local"
                } else {
                    self.snapshot.message.as_str()
                }
            } else {
                self.snapshot.message.as_str()
            };
            r.text(
                msg,
                if self.mini { 24. } else { 36. },
                h - 31.,
                w - if self.mini { 48. } else { 72. },
                0,
                if !self.notice.is_empty() {
                    ACCENT
                } else {
                    MUTED
                },
            );
            if !r.end() {
                self.renderer = None;
            }
        }
        EndPaint(self.hwnd, &ps);
    }
    fn timer_label(&self) -> String {
        match self.timer {
            Some(t) if t > protocol::epoch() => {
                let n = t - protocol::epoch();
                format!("Desliga em {:02}:{:02}", n / 60, n % 60)
            }
            Some(_) => "Timer concluído".into(),
            None => "Nenhum timer ativo".into(),
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, message, wp, lp);
    }
    let ui = &mut *ptr;
    if message == ui.taskbar {
        ui.add_tray();
        return 0;
    }
    match message {
        WM_PAINT => {
            ui.paint();
            0
        }
        WM_ERASEBKGND => 1,
        UPDATE => {
            ui.update();
            0
        }
        OPEN => {
            ui.show(wp != 0);
            0
        }
        QUIT => {
            DestroyWindow(hwnd);
            0
        }
        HIDE => {
            ui.hide();
            0
        }
        STATS => {
            let stats = serde_json::json!({"epoch":protocol::epoch(),"visible":IsWindowVisible(hwnd)!=0,"mini":ui.mini,"online":ui.snapshot.online,"message":ui.snapshot.message,"paints":ui.paints,"rebuilds":ui.rebuilds,"updates":ui.updates,"latency_ms":ui.snapshot.latency_ms});
            let _ = std::fs::write(
                ui.store.root.join("runtime-stats.json"),
                serde_json::to_vec_pretty(&stats).unwrap(),
            );
            0
        }
        WM_SIZE => {
            if wp != SIZE_MINIMIZED as usize {
                let size = (
                    (lp as u32 & 0xffff) as i32,
                    ((lp as u32 >> 16) & 0xffff) as i32,
                );
                if size != ui.layout_size {
                    ui.renderer = None;
                    if !ui.controls.is_empty() {
                        ui.reflow();
                    } else {
                        ui.layout_size = size;
                    }
                }
            }
            0
        }
        WM_GETMINMAXINFO => {
            if !ui.mini {
                let info = &mut *(lp as *mut MINMAXINFO);
                info.ptMinTrackSize.x = (780. * ui.dpi) as i32;
                info.ptMinTrackSize.y = (700. * ui.dpi) as i32;
            }
            0
        }
        WM_DPICHANGED => {
            ui.dpi = (wp & 0xffff) as f32 / 96.;
            ui.widgets = None;
            ui.renderer = None;
            let r = &*(lp as *const RECT);
            DeleteObject(ui.font);
            ui.font = CreateFontW(
                -(14. * ui.dpi) as i32,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide("Segoe UI").as_ptr(),
            );
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            ui.rebuild();
            0
        }
        WM_CLOSE => {
            if ui.store.config.close_to_tray {
                ui.hide();
            } else {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            ui.remove_tray();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            PostQuitMessage(0);
            0
        }
        WM_ACTIVATE => {
            if (wp & 0xffff) == WA_INACTIVE as usize && ui.capturing.is_some() {
                ui.cancel_capture();
            }
            if (wp & 0xffff) == WA_INACTIVE as usize
                && ui.mini
                && !ui.store.config.pinned
                && !ui.modal
                && ui.opened_at.elapsed() > Duration::from_millis(400)
            {
                ui.hide();
            }
            0
        }
        WM_HOTKEY => {
            ui.execute_shortcut(wp.saturating_sub(1));
            0
        }
        TRAY_OPEN => {
            ui.show(true);
            0
        }
        TRAY => {
            match lp as u32 & 0xffff {
                NIN_SELECT | NIN_KEYSELECT => {
                    if ui
                        .tray_event_at
                        .is_none_or(|t| t.elapsed() > Duration::from_millis(350))
                    {
                        ui.tray_event_at = Some(Instant::now());
                        PostMessageW(hwnd, TRAY_OPEN, 0, 0);
                    }
                }
                WM_CONTEXTMENU => ui.tray_menu(),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            let id = (wp & 0xffff) as u16;
            let notify = ((wp >> 16) & 0xffff) as u16;
            if notify == LBN_SELCHANGE as u16 && [300, 310, 400, 700].contains(&id) {
                if id == 700 {
                    if let Some(i) = ui.selection(700) {
                        if let Some(s) = ui.store.config.scenes.get(i) {
                            ui.set(701, &s.name);
                            ui.set(703, &s.speed.to_string());
                            ui.set(704, &s.brightness.to_string());
                            ui.set(705, &s.colors.join(", "));
                            SendMessageW(ui.controls[&702], CB_SETCURSEL, s.effect as usize, 0);
                            ui.invalidate_palette();
                        }
                    }
                } else {
                    ui.selected(id);
                }
            } else if notify == LBN_DBLCLK as u16 && [300, 310, 700].contains(&id) {
                ui.action(match id {
                    300 => 301,
                    310 => 316,
                    _ => 713,
                });
            } else if notify == BN_CLICKED as u16 {
                ui.action(id);
            }
            0
        }
        WM_HSCROLL => {
            let h = lp as HWND;
            let id = GetDlgCtrlID(h) as u16;
            ui.slider_changed(id, (wp & 0xffff) as u16);
            0
        }
        WM_TIMER => {
            if wp == SLIDER_TIMER {
                KillTimer(hwnd, SLIDER_TIMER);
                ui.dragging = false;
                if let Some(d) = ui.pending_send.take() {
                    ui.commit(d);
                }
            } else if wp == CLOCK_TIMER {
                if ui.timer.is_some_and(|t| protocol::epoch() >= t) {
                    ui.timer = None;
                    KillTimer(hwnd, CLOCK_TIMER);
                }
                if IsWindowVisible(hwnd) != 0 {
                    ui.invalidate();
                }
            } else if wp == ANIMATION_TIMER {
                let target = ui.target_glow();
                ui.glow += (target - ui.glow) * 0.18;
                if target == 0. && ui.glow < 0.01 {
                    ui.glow = 0.;
                }
                let x = if ui.mini { 32. } else { 56. };
                let y = if ui.mini { 46. } else { 151. };
                let area = RECT {
                    left: (x * ui.dpi) as i32,
                    top: (y * ui.dpi) as i32,
                    right: ((x + 300.) * ui.dpi) as i32,
                    bottom: ((y + 176.) * ui.dpi) as i32,
                };
                InvalidateRect(hwnd, &area, 0);
                ui.sync_animation();
            } else if wp == 5 {
                KillTimer(hwnd, 5);
                let foreground = GetForegroundWindow();
                if ui.mini
                    && !ui.modal
                    && !ui.store.config.pinned
                    && foreground != hwnd
                    && IsChild(hwnd, foreground) == 0
                {
                    ui.hide();
                }
            } else if wp == 3 {
                KillTimer(hwnd, 3);
                if let Some(path) = ui.capture.take() {
                    let _ = capture_client(hwnd, &path);
                }
            }
            0
        }
        WM_MEASUREITEM => {
            let item = &mut *(lp as *mut MEASUREITEMSTRUCT);
            if item.CtlType == ODT_LISTBOX {
                item.itemHeight = (54. * ui.dpi) as u32;
                return 1;
            }
            0
        }
        WM_DRAWITEM => {
            draw_button(ui, &*(lp as *const DRAWITEMSTRUCT));
            1
        }
        WM_NOTIFY => {
            let header = &*(lp as *const NMHDR);
            if header.code == NM_CUSTOMDRAW && [100, 101, 102].contains(&(header.idFrom as u16)) {
                let d = &*(lp as *const NMCUSTOMDRAW);
                if d.dwDrawStage == CDDS_PREPAINT {
                    draw_slider(ui, d);
                    return CDRF_SKIPDEFAULT as isize;
                }
            }
            DefWindowProcW(hwnd, message, wp, lp)
        }
        WM_CTLCOLORSTATIC => {
            SetTextColor(wp as HDC, rgb(MUTED));
            let card = (900..=902).contains(&GetDlgCtrlID(lp as HWND));
            SetBkColor(wp as HDC, rgb(if card { CARD } else { BG }));
            if card {
                ui.card as isize
            } else {
                ui.bg as isize
            }
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            SetTextColor(wp as HDC, rgb(TEXT));
            let is_card_list = [300, 310, 400, 700].contains(&GetDlgCtrlID(lp as HWND));
            SetBkColor(wp as HDC, rgb(if is_card_list { BG } else { TILE }));
            if is_card_list {
                ui.bg as isize
            } else {
                ui.tile as isize
            }
        }
        _ => DefWindowProcW(hwnd, message, wp, lp),
    }
}

unsafe fn make_icon() -> HICON {
    let dc = GetDC(null_mut());
    let mut bitmap: BITMAPV5HEADER = zeroed();
    bitmap.bV5Size = size_of::<BITMAPV5HEADER>() as u32;
    bitmap.bV5Width = 32;
    bitmap.bV5Height = 32;
    bitmap.bV5Planes = 1;
    bitmap.bV5BitCount = 32;
    bitmap.bV5Compression = BI_BITFIELDS;
    bitmap.bV5RedMask = 0x00ff0000;
    bitmap.bV5GreenMask = 0x0000ff00;
    bitmap.bV5BlueMask = 0x000000ff;
    bitmap.bV5AlphaMask = 0xff000000;
    let mut bits = null_mut();
    let color = CreateDIBSection(
        dc,
        &bitmap as *const _ as *const BITMAPINFO,
        DIB_RGB_COLORS,
        &mut bits,
        null_mut(),
        0,
    );
    ReleaseDC(null_mut(), dc);
    if color.is_null() {
        return LoadIconW(null_mut(), IDI_APPLICATION);
    }
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, 1024);
    for y in 0..32 {
        for x in 0..32 {
            let dx = x as i32 - 16;
            let dy = y as i32 - 20;
            pixels[y * 32 + x] =
                if dx * dx + dy * dy <= 86 || ((12..=20).contains(&x) && (5..=19).contains(&y)) {
                    0xffd7dbb8
                } else {
                    0
                };
        }
    }
    let mask = CreateBitmap(32, 32, 1, 1, null());
    let info = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = CreateIconIndirect(&info);
    DeleteObject(mask);
    DeleteObject(color);
    icon
}

unsafe fn capture_client(hwnd: HWND, path: &std::path::Path) -> Result<(), String> {
    let mut r: RECT = zeroed();
    GetClientRect(hwnd, &mut r);
    let w = r.right;
    let h = r.bottom;
    let dc = GetDC(hwnd);
    let memory = CreateCompatibleDC(dc);
    let mut bits = null_mut();
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        },
        ..zeroed()
    };
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() {
        DeleteDC(memory);
        ReleaseDC(hwnd, dc);
        return Err("Falha ao exportar a prévia.".into());
    }
    let old = SelectObject(memory, bitmap);
    BitBlt(memory, 0, 0, w, h, dc, 0, 0, SRCCOPY);
    GdiFlush();
    let count = (w * h * 4) as usize;
    let mut bmp = Vec::with_capacity(count + 54);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((count + 54) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&w.to_le_bytes());
    bmp.extend_from_slice(&h.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&32u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    bmp.extend_from_slice(std::slice::from_raw_parts(bits as *const u8, count));
    let result = std::fs::write(path, bmp).map_err(|e| e.to_string());
    SelectObject(memory, old);
    DeleteObject(bitmap);
    DeleteDC(memory);
    ReleaseDC(hwnd, dc);
    result
}

unsafe fn autostart(enabled: bool, root: &std::path::Path) -> Result<(), String> {
    let mut key = null_mut();
    let status = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
        0,
        null(),
        0,
        KEY_SET_VALUE,
        null(),
        &mut key,
        null_mut(),
    );
    if status != 0 {
        return Err("Não foi possível atualizar o início com o Windows.".into());
    }
    let value = wide("LightControl");
    let result = if enabled {
        let exe = std::env::current_exe().map_err(|_| "Executável não localizado.")?;
        let command = wide(&format!(
            "\"{}\" --tray --data-dir \"{}\"",
            exe.display(),
            root.display()
        ));
        RegSetValueExW(
            key,
            value.as_ptr(),
            0,
            REG_SZ,
            command.as_ptr() as _,
            (command.len() * 2) as u32,
        )
    } else {
        let r = RegDeleteValueW(key, value.as_ptr());
        if r == ERROR_FILE_NOT_FOUND {
            0
        } else {
            r
        }
    };
    RegCloseKey(key);
    if result == 0 {
        Ok(())
    } else {
        Err("Não foi possível salvar o início automático.".into())
    }
}
