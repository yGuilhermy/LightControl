use super::*;

pub(super) fn hex_color(s: &str) -> u32 {
    u32::from_str_radix(s.trim().trim_start_matches('#'), 16).unwrap_or(ACCENT)
}
pub(super) fn color_name(hex: &str) -> &'static str {
    let mut l = Light::default();
    if l.set_hex(hex).is_err() {
        return "Cor personalizada";
    }
    if l.value < 14 {
        return "Noite";
    }
    if l.saturation < 12 {
        return "Neve";
    }
    match l.hue {
        0..=15 | 345..=360 => "Coral",
        16..=44 => "Pêssego",
        45..=69 => "Dourado",
        70..=154 => "Jardim",
        155..=194 => "Menta",
        195..=249 => "Oceano",
        250..=289 => "Lavanda",
        _ => "Rosa",
    }
}
impl Ui {
    pub(super) unsafe fn widget_renderer(&mut self) -> Option<&Renderer> {
        if self.graphics.is_none() {
            self.graphics = Graphics::new().ok();
        }
        if self.widgets.is_none() {
            self.widgets = self
                .graphics
                .as_ref()
                .and_then(|g| Renderer::new(g, 96. * self.dpi).ok());
        }
        self.widgets.as_ref()
    }
    pub(super) unsafe fn choose_color(&mut self, current: &str) -> Option<String> {
        self.modal = true;
        let result = super::picker::show(self.hwnd, current, &self.store.config.favorites);
        self.modal = false;
        self.opened_at = Instant::now();
        result
    }
    pub(super) unsafe fn scene_colors(&self) -> Vec<String> {
        self.get(705)
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }
    pub(super) unsafe fn invalidate_palette(&self) {
        for id in 760..=767 {
            if let Some(h) = self.controls.get(&id) {
                InvalidateRect(*h, null(), 0);
            }
        }
    }
    pub(super) unsafe fn sync_animation(&mut self) {
        if self.animations
            && self.page == Page::Home
            && IsWindowVisible(self.hwnd) != 0
            && IsIconic(self.hwnd) == 0
            && (self.target_glow() > 0. || self.glow > 0.01)
        {
            SetTimer(self.hwnd, ANIMATION_TIMER, 50, None);
        } else {
            KillTimer(self.hwnd, ANIMATION_TIMER);
        }
    }
    pub(super) fn target_glow(&self) -> f32 {
        if (self.snapshot.known || self.preview) && self.display.power {
            0.45 + self.display.level() as f32 / 100. * 0.55
        } else {
            0.
        }
    }
}

pub(super) unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let parent = data as HWND;
    let ptr = GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut Ui;
    if !ptr.is_null() {
        let ui = &mut *ptr;
        let id = GetDlgCtrlID(hwnd) as u16;
        match msg {
            WM_ERASEBKGND => return 1,
            WM_MOUSEMOVE => {
                if ui.hover != Some(id) {
                    let previous = ui.hover.replace(id);
                    if let Some(previous) = previous.and_then(|id| ui.controls.get(&id)) {
                        InvalidateRect(*previous, null(), 0);
                    }
                    InvalidateRect(hwnd, null(), 0);
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut tracking);
                }
            }
            WM_MOUSELEAVE => {
                if ui.hover == Some(id) {
                    ui.hover = None;
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(button_proc), 1);
            }
            _ => {}
        }
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

pub(super) unsafe extern "system" fn list_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(list_proc), 1);
    }
    let result = DefSubclassProc(hwnd, msg, wp, lp);
    if msg == WM_PAINT && SendMessageW(hwnd, LB_GETCOUNT, 0, 0) == 0 {
        let ptr = GetWindowLongPtrW(data as HWND, GWLP_USERDATA) as *mut Ui;
        if !ptr.is_null() {
            let ui = &mut *ptr;
            let mut bounds: RECT = zeroed();
            GetClientRect(hwnd, &mut bounds);
            let w = bounds.right as f32 / ui.dpi;
            let h = bounds.bottom as f32 / ui.dpi;
            let dc = GetDC(hwnd);
            if let Some(r) = ui.widget_renderer() {
                if r.begin(dc, bounds.right, bounds.bottom) {
                    r.round(0., 0., w, h, 12., CARD);
                    r.ellipse(w / 2., h / 2. - 45., 18., 18., 0x34422f);
                    r.line(
                        w / 2. - 5.,
                        h / 2. - 45.,
                        w / 2. + 5.,
                        h / 2. - 45.,
                        ACCENT,
                        1.6,
                    );
                    r.line(w / 2., h / 2. - 50., w / 2., h / 2. - 40., ACCENT, 1.6);
                    r.text(
                        "Tudo começa com um toque.",
                        18.,
                        h / 2. - 10.,
                        w - 36.,
                        1,
                        TEXT,
                    );
                    r.text(
                        "Crie seu primeiro item abaixo.",
                        18.,
                        h / 2. + 18.,
                        w - 36.,
                        0,
                        MUTED,
                    );
                    r.end();
                }
            }
            ReleaseDC(hwnd, dc);
        }
    }
    result
}

pub(super) unsafe fn draw_button(ui: &mut Ui, item: &DRAWITEMSTRUCT) {
    if item.CtlType == ODT_LISTBOX {
        draw_list_item(ui, item);
        return;
    }
    let id = item.CtlID as u16;
    let active = match id {
        10 => ui.page == Page::Home,
        11 => ui.page == Page::Presets,
        12 => ui.page == Page::Routines,
        13 => matches!(ui.page, Page::Settings | Page::Shortcuts),
        15 => ui.page == Page::Scenes,
        21 => ui.display.mode == "white",
        22 => ui.display.mode == "colour",
        410..=416 => ui.editing_days & (1 << (id - 410)) != 0,
        _ => false,
    };
    let hover = ui.hover == Some(id);
    let down = item.itemState & ODS_SELECTED != 0;
    let w = (item.rcItem.right - item.rcItem.left) as f32 / ui.dpi;
    let h = (item.rcItem.bottom - item.rcItem.top) as f32 / ui.dpi;
    let label = text(item.hwndItem);
    let picker_color = if id == 317 {
        Some(hex_color(&ui.get(311)))
    } else {
        None
    };
    let picker_name = if id == 317 {
        Some(color_name(&ui.get(311)))
    } else {
        None
    };
    let swatch = if (50..=55).contains(&id) {
        ui.store
            .config
            .favorites
            .get((id - 50) as usize)
            .map(|s| hex_color(s))
    } else if (760..=767).contains(&id) {
        ui.scene_colors()
            .get((id - 760) as usize)
            .map(|s| hex_color(s))
    } else {
        None
    };
    let quick = if (30..=33).contains(&id) {
        ui.store.config.presets.get((id - 30) as usize).cloned()
    } else {
        None
    };
    let scene_chip = (760..=767).contains(&id);
    let on = ui.display.power;
    let surface = if (10..=15).contains(&id) && id != 14 {
        0x151b16
    } else if (820..=825).contains(&id)
        || (ui.page == Page::Home
            && ([20, 21, 22, 23, 24, 25, 26].contains(&id) || (50..=55).contains(&id)))
    {
        CARD
    } else {
        BG
    };
    let r = match ui.widget_renderer() {
        Some(r) => r,
        None => return,
    };
    if !r.begin(item.hDC, item.rcItem.right, item.rcItem.bottom) {
        return;
    }
    r.round(0., 0., w, h, 0., surface);
    let fill = if let Some(c) = swatch {
        c
    } else if down {
        0x44543e
    } else if active {
        0x3b4935
    } else if hover {
        0x313d32
    } else if id == 20 && on {
        0x3b4935
    } else {
        TILE
    };
    if !scene_chip || swatch.is_some() {
        r.round(
            1.,
            1.,
            w - 2.,
            h - 2.,
            if id == 20 { 15. } else { 9. },
            fill,
        );
    }
    if active {
        r.stroke(1., 1., w - 2., h - 2., 9., 0x627456);
    }
    if scene_chip && swatch.is_none() {
        r.stroke(1., 1., w - 2., h - 2., 9., 0x303a30);
    }
    if let Some(preset) = quick {
        let c = if preset.light.mode == "colour" {
            hex_color(&preset.light.hex())
        } else {
            mix(0xffb96e, 0xc5e2ff, preset.light.temperature as f32 / 100.)
        };
        r.ellipse(20., h / 2., 5., 5., c);
        r.text(&label, 35., (h - 18.) / 2. - 1., w - 40., 1, TEXT);
    } else if let Some(c) = picker_color {
        r.round(12., h / 2. - 7., 14., 14., 5., c);
        r.text(
            picker_name.unwrap_or("Escolher cor"),
            37.,
            (h - 18.) / 2. - 1.,
            w - 42.,
            1,
            TEXT,
        );
    } else if swatch.is_none() && !scene_chip {
        r.centered(&label, w, h, if active { ACCENT } else { TEXT });
    }
    if hover || item.itemState & ODS_FOCUS != 0 {
        r.stroke(
            2.,
            2.,
            w - 4.,
            h - 4.,
            8.,
            if item.itemState & ODS_FOCUS != 0 {
                ACCENT
            } else {
                0x65765a
            },
        );
    }
    r.end();
}

unsafe fn draw_list_item(ui: &mut Ui, item: &DRAWITEMSTRUCT) {
    let id = item.CtlID as u16;
    let i = item.itemID as usize;
    let w = (item.rcItem.right - item.rcItem.left) as f32 / ui.dpi;
    let h = (item.rcItem.bottom - item.rcItem.top) as f32 / ui.dpi;
    let selected = item.itemState & ODS_SELECTED != 0;
    let mut title = String::new();
    let mut subtitle = String::new();
    let mut colors = Vec::new();
    match id {
        300 => {
            if let Some(p) = ui.store.config.presets.get(i) {
                title = p.name.clone();
                subtitle = format!(
                    "{}% · {}",
                    p.light.level(),
                    if p.light.mode == "colour" {
                        "Luz colorida"
                    } else {
                        "Luz branca"
                    }
                );
                colors.push(if p.light.mode == "colour" {
                    hex_color(&p.light.hex())
                } else {
                    mix(0xffb96e, 0xc5e2ff, p.light.temperature as f32 / 100.)
                });
            }
        }
        310 => {
            if let Some(c) = ui.store.config.favorites.get(i) {
                title = color_name(c).into();
                subtitle = "Cor favorita".into();
                colors.push(hex_color(c));
            }
        }
        700 => {
            if let Some(s) = ui.store.config.scenes.get(i) {
                title = s.name.clone();
                subtitle = format!(
                    "{} · {}%",
                    ["Luz constante", "Cores alternadas", "Transição suave"]
                        [s.effect.min(2) as usize],
                    s.brightness
                );
                colors.extend(s.colors.iter().map(|s| hex_color(s)));
            }
        }
        400 => {
            if let Some(r) = ui.store.config.routines.get(i) {
                title = format!("{:02}:{:02}  {}", r.hour, r.minute, r.name);
                subtitle = format!(
                    "{} · {}",
                    if r.enabled { "Ativa" } else { "Pausada" },
                    if r.light.power {
                        "Ligar a luz"
                    } else {
                        "Desligar a luz"
                    }
                );
            }
        }
        _ => {}
    }
    let r = match ui.widget_renderer() {
        Some(r) => r,
        None => return,
    };
    if !r.begin_at(
        item.hDC,
        item.rcItem.left,
        item.rcItem.top,
        item.rcItem.right - item.rcItem.left,
        item.rcItem.bottom - item.rcItem.top,
    ) {
        return;
    }
    r.round(0., 0., w, h, 0., BG);
    if !title.is_empty() {
        r.round(
            0.,
            2.,
            w - 4.,
            h - 6.,
            11.,
            if selected { 0x34422f } else { CARD },
        );
        if selected {
            r.stroke(0.5, 2.5, w - 5., h - 7., 11., 0x596e4c);
        }
        let x = if colors.is_empty() { 15. } else { 54. };
        if let Some(c) = colors.first() {
            r.round(13., 16., 29., 29., 10., *c);
        }
        r.text(&title, x, 10., w - x - 10., 1, TEXT);
        r.text(&subtitle, x, 32., w - x - 10., 0, MUTED);
        for (n, c) in colors.iter().skip(1).take(6).enumerate() {
            r.ellipse(w - 17. - n as f32 * 9., 45., 3., 3., *c);
        }
    }
    r.end();
}

pub(super) unsafe fn draw_slider(ui: &mut Ui, item: &NMCUSTOMDRAW) {
    let mut bounds: RECT = zeroed();
    GetClientRect(item.hdr.hwndFrom, &mut bounds);
    let w = bounds.right as f32 / ui.dpi;
    let h = bounds.bottom as f32 / ui.dpi;
    let id = item.hdr.idFrom as u16;
    let colour = ui.display.mode == "colour";
    let max = if id == 101 && colour { 360. } else { 100. };
    let value = SendMessageW(item.hdr.hwndFrom, TBM_GETPOS, 0, 0) as f32;
    let (rail_start, rail_end, start, end) = slider_geometry(w);
    let minimum = SendMessageW(item.hdr.hwndFrom, TBM_GETRANGEMIN, 0, 0) as f32;
    let x = start + (end - start) * (value - minimum) / (max - minimum);
    let tint = hex_color(&ui.display.hex());
    let r = match ui.widget_renderer() {
        Some(r) => r,
        None => return,
    };
    if !r.begin(item.hdc, bounds.right, bounds.bottom) {
        return;
    }
    r.round(0., 0., w, h, 0., CARD);
    let y = h / 2. - 14.;
    if id == 101 && colour {
        r.gradient(
            rail_start,
            y,
            rail_end - rail_start,
            28.,
            13.,
            &[
                0xed7373, 0xe4c46e, 0x8cd48b, 0x70c5c8, 0x798fda, 0xb58ed2, 0xed7373,
            ],
        );
    } else if id == 101 {
        r.gradient(
            rail_start,
            y,
            rail_end - rail_start,
            28.,
            13.,
            &[0xd49a56, 0xd8c89d, 0xb6d5e6],
        );
    } else if id == 102 {
        r.gradient(
            rail_start,
            y,
            rail_end - rail_start,
            28.,
            13.,
            &[0xdde1d6, tint],
        );
    } else {
        r.round(rail_start, y, rail_end - rail_start, 28., 13., 0x101710);
        r.gradient(
            rail_start,
            y,
            (x + 10. - rail_start).max(22.),
            28.,
            13.,
            &[0x56674c, ACCENT],
        );
        for i in 1..10 {
            let tick = start + (end - start) * i as f32 / 10.;
            r.line(
                tick,
                y + 10.,
                tick,
                y + 18.,
                mix(0x101710, ACCENT, 0.17),
                1.,
            );
        }
    }
    r.round(x - 9., y + 4., 18., 22., 6., 0x0d120d);
    r.gradient(x - 9., y + 3., 18., 22., 6., &[0xf2f3e9, 0xbac7ad]);
    r.line(x - 2., y + 10., x - 2., y + 18., 0x6b795e, 1.);
    r.line(x + 2., y + 10., x + 2., y + 18., 0x6b795e, 1.);
    r.end();
}

pub(super) fn slider_geometry(width: f32) -> (f32, f32, f32, f32) {
    (4., width - 4., 15., (width - 15.).max(16.))
}
pub(super) unsafe extern "system" fn slider_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
            if msg == WM_LBUTTONDOWN {
                SetFocus(hwnd);
                SetCapture(hwnd);
            }
            if GetCapture() == hwnd {
                let mut bounds: RECT = zeroed();
                GetClientRect(hwnd, &mut bounds);
                let dpi = GetDpiForWindow(hwnd).max(96) as f32 / 96.;
                let (_, _, start, end) = slider_geometry(bounds.right as f32 / dpi);
                let x = (lp as i16) as f32 / dpi;
                let min = SendMessageW(hwnd, TBM_GETRANGEMIN, 0, 0) as f32;
                let max = SendMessageW(hwnd, TBM_GETRANGEMAX, 0, 0) as f32;
                let value = (min + (max - min) * ((x - start) / (end - start)).clamp(0., 1.))
                    .round() as isize;
                SendMessageW(hwnd, TBM_SETPOS, 1, value);
                let code = if msg == WM_LBUTTONUP {
                    TB_THUMBPOSITION
                } else {
                    TB_THUMBTRACK
                };
                SendMessageW(GetParent(hwnd), WM_HSCROLL, code as usize, hwnd as isize);
                if msg == WM_LBUTTONUP {
                    ReleaseCapture();
                }
                return 0;
            }
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(slider_proc), 2);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}
