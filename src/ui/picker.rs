use super::*;

const PICKER_CLASS: &str = "LightControl.Color.1";
struct Picker {
    hwnd: HWND,
    dpi: f32,
    light: Light,
    favorites: Vec<String>,
    tracks: [HWND; 3],
    renderer: Renderer,
    widgets: Renderer,
    done: bool,
    result: Option<String>,
}
impl Picker {
    unsafe fn create(owner: HWND, current: &str, favorites: &[String]) -> Option<Box<Self>> {
        let dpi = GetDpiForWindow(owner).max(96) as f32 / 96.;
        let graphics = Graphics::new().ok()?;
        let mut p = Box::new(Self {
            hwnd: null_mut(),
            dpi,
            light: Light::default(),
            favorites: favorites.iter().take(8).cloned().collect(),
            tracks: [null_mut(); 3],
            renderer: Renderer::new(&graphics, 96. * dpi).ok()?,
            widgets: Renderer::new(&graphics, 96. * dpi).ok()?,
            done: false,
            result: None,
        });
        let _ = p.light.set_hex(current);
        let class = wide(PICKER_CLASS);
        RegisterClassExW(&WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: GetModuleHandleW(null()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        });
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
        let ex = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT | WS_EX_COMPOSITED;
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: (420. * dpi) as i32,
            bottom: (480. * dpi) as i32,
        };
        AdjustWindowRectExForDpi(&mut bounds, style, 0, ex, (96. * dpi) as u32);
        let mut parent: RECT = zeroed();
        GetWindowRect(owner, &mut parent);
        let mut monitor: MONITORINFO = zeroed();
        monitor.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        );
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        let x = ((parent.left + parent.right - width) / 2).clamp(
            monitor.rcWork.left,
            (monitor.rcWork.right - width).max(monitor.rcWork.left),
        );
        let y = ((parent.top + parent.bottom - height) / 2).clamp(
            monitor.rcWork.top,
            (monitor.rcWork.bottom - height).max(monitor.rcWork.top),
        );
        p.hwnd = CreateWindowExW(
            ex,
            class.as_ptr(),
            wide("Sua paleta · LightControl").as_ptr(),
            style,
            x,
            y,
            width,
            height,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if p.hwnd.is_null() {
            return None;
        }
        SetWindowLongPtrW(p.hwnd, GWLP_USERDATA, (&mut *p as *mut Self) as isize);
        let dark = 1i32;
        DwmSetWindowAttribute(
            p.hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark as *const _ as _,
            4,
        );
        for (i, label) in ["Tom", "Intensidade da cor", "Brilho da cor"]
            .iter()
            .enumerate()
        {
            let h = CreateWindowExW(
                0,
                wide("msctls_trackbar32").as_ptr(),
                wide(label).as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | TBS_NOTICKS,
                (28. * dpi) as i32,
                ((184. + 68. * i as f32) * dpi) as i32,
                (364. * dpi) as i32,
                (34. * dpi) as i32,
                p.hwnd,
                (i + 1) as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            p.tracks[i] = h;
            SendMessageW(h, TBM_SETRANGEMAX, 0, if i == 0 { 360 } else { 100 });
            SetWindowSubclass(h, Some(slider_proc), 2, 0);
        }
        for (id, label, x, y, w, h) in [
            (101, "Cancelar", 28., 426., 170., 36.),
            (102, "Usar cor", 222., 426., 170., 36.),
        ] {
            p.button(id, label, x, y, w, h);
        }
        for (i, c) in p.favorites.iter().enumerate() {
            p.button(
                200 + i,
                &format!("Escolher {}", color_name(c)),
                28. + i as f32 * 46.,
                378.,
                36.,
                28.,
            );
        }
        p.sync();
        Some(p)
    }
    #[allow(clippy::too_many_arguments)]
    unsafe fn button(&self, id: usize, label: &str, x: f32, y: f32, w: f32, h: f32) {
        CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            (x * self.dpi) as i32,
            (y * self.dpi) as i32,
            (w * self.dpi) as i32,
            (h * self.dpi) as i32,
            self.hwnd,
            id as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
    }
    unsafe fn sync(&self) {
        for (h, v) in
            self.tracks
                .iter()
                .zip([self.light.hue, self.light.saturation, self.light.value])
        {
            SendMessageW(*h, TBM_SETPOS, 1, v as isize);
        }
        RedrawWindow(
            self.hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        );
    }
    unsafe fn paint(&self) {
        let mut ps: PAINTSTRUCT = zeroed();
        let dc = BeginPaint(self.hwnd, &mut ps);
        let mut rc: RECT = zeroed();
        GetClientRect(self.hwnd, &mut rc);
        let r = &self.renderer;
        if r.begin(dc, rc.right, rc.bottom) {
            r.text("Escolha uma cor", 28., 22., 364., 3, TEXT);
            r.round(28., 68., 364., 70., 14., CARD);
            let hex = self.light.hex();
            r.round(42., 80., 46., 46., 14., hex_color(&hex));
            r.text(color_name(&hex), 104., 80., 270., 2, TEXT);
            r.text(
                "Ajuste e confirme quando estiver pronto.",
                104.,
                105.,
                270.,
                0,
                MUTED,
            );
            for (i, label) in ["Tom", "Intensidade da cor", "Brilho da cor"]
                .iter()
                .enumerate()
            {
                let y = 158. + 68. * i as f32;
                r.text(label, 32., y, 260., 1, MUTED);
                let value = match i {
                    0 => format!("{}°", self.light.hue),
                    1 => format!("{}%", self.light.saturation),
                    _ => format!("{}%", self.light.value),
                };
                r.text(&value, 342., y, 55., 1, TEXT);
            }
            r.text("SUA PALETA", 32., 356., 360., 0, MUTED);
            r.end();
        }
        EndPaint(self.hwnd, &ps);
    }
    unsafe fn slider(&self, item: &NMCUSTOMDRAW) {
        let mut rc: RECT = zeroed();
        GetClientRect(item.hdr.hwndFrom, &mut rc);
        let w = rc.right as f32 / self.dpi;
        let id = item.hdr.idFrom;
        let mut full = self.light.clone();
        full.saturation = 100;
        full.value = 100;
        let hue = hex_color(&full.hex());
        full.saturation = self.light.saturation;
        let colors = match id {
            1 => vec![
                0xff6666, 0xffde66, 0x70db79, 0x65d7db, 0x7184ed, 0xd078e6, 0xff6666,
            ],
            2 => vec![0xd9ded6, hue],
            _ => vec![0x101311, hex_color(&full.hex())],
        };
        let value = SendMessageW(item.hdr.hwndFrom, TBM_GETPOS, 0, 0) as f32;
        let (a, b, start, end) = slider_geometry(w);
        let x = start + (end - start) * value / if id == 1 { 360. } else { 100. };
        let r = &self.widgets;
        if r.begin(item.hdc, rc.right, rc.bottom) {
            r.gradient(a, 3., b - a, 28., 13., &colors);
            r.round(x - 9., 6., 18., 22., 6., TEXT);
            r.line(x, 12., x, 22., 0x67745c, 2.);
            r.end();
        }
    }
    unsafe fn button_draw(&self, item: &DRAWITEMSTRUCT) {
        let w = (item.rcItem.right - item.rcItem.left) as f32 / self.dpi;
        let h = (item.rcItem.bottom - item.rcItem.top) as f32 / self.dpi;
        let r = &self.widgets;
        if r.begin(
            item.hDC,
            item.rcItem.right - item.rcItem.left,
            item.rcItem.bottom - item.rcItem.top,
        ) {
            if item.CtlID >= 200 {
                if let Some(c) = self.favorites.get((item.CtlID - 200) as usize) {
                    r.round(1., 1., w - 2., h - 2., 9., hex_color(c));
                }
            } else {
                r.round(
                    1.,
                    1.,
                    w - 2.,
                    h - 2.,
                    10.,
                    if item.CtlID == 102 { 0x3b4b32 } else { TILE },
                );
                r.centered(&text(item.hwndItem), w, h, TEXT);
            }
            if item.itemState & ODS_FOCUS != 0 {
                r.stroke(1., 1., w - 2., h - 2., 9., ACCENT);
            }
            r.end();
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Picker;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let p = &mut *ptr;
    match msg {
        WM_PAINT => {
            p.paint();
            0
        }
        WM_ERASEBKGND => 1,
        WM_CLOSE => {
            p.done = true;
            0
        }
        WM_COMMAND => {
            match wp & 0xffff {
                101 => p.done = true,
                102 => {
                    p.result = Some(p.light.hex());
                    p.done = true;
                }
                id if id >= 200 => {
                    if let Some(c) = p.favorites.get(id - 200) {
                        let _ = p.light.set_hex(c);
                        p.sync();
                    }
                }
                _ => {}
            }
            0
        }
        WM_HSCROLL => {
            p.light.hue = SendMessageW(p.tracks[0], TBM_GETPOS, 0, 0) as u16;
            p.light.saturation = SendMessageW(p.tracks[1], TBM_GETPOS, 0, 0) as u16;
            p.light.value = SendMessageW(p.tracks[2], TBM_GETPOS, 0, 0) as u16;
            RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN);
            0
        }
        WM_NOTIFY => {
            let item = &*(lp as *const NMCUSTOMDRAW);
            if item.hdr.code == NM_CUSTOMDRAW && item.dwDrawStage == CDDS_PREPAINT {
                p.slider(item);
                CDRF_SKIPDEFAULT as isize
            } else {
                0
            }
        }
        WM_DRAWITEM => {
            p.button_draw(&*(lp as *const DRAWITEMSTRUCT));
            1
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub(super) unsafe fn show(owner: HWND, current: &str, favorites: &[String]) -> Option<String> {
    let mut p = Picker::create(owner, current, favorites)?;
    EnableWindow(owner, 0);
    ShowWindow(p.hwnd, SW_SHOW);
    SetFocus(p.tracks[0]);
    let mut msg: MSG = zeroed();
    while !p.done {
        let result = GetMessageW(&mut msg, null_mut(), 0, 0);
        if result <= 0 {
            if result == 0 {
                PostQuitMessage(msg.wParam as i32);
            }
            break;
        }
        if msg.message == WM_KEYDOWN {
            if msg.wParam == VK_ESCAPE as usize {
                break;
            }
            if msg.wParam == VK_RETURN as usize {
                p.result = Some(p.light.hex());
                break;
            }
        }
        if IsDialogMessageW(p.hwnd, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    EnableWindow(owner, 1);
    DestroyWindow(p.hwnd);
    SetForegroundWindow(owner);
    p.result.take()
}
pub(super) unsafe fn verify(owner: HWND, path: &std::path::Path) -> Result<(), String> {
    let mut p = Picker::create(owner, "#FF0000", &["#75D5CF".into(), "#B497FF".into()])
        .ok_or("Falha ao abrir paleta")?;
    ShowWindow(p.hwnd, SW_SHOW);
    SendMessageW(p.tracks[0], TBM_SETPOS, 1, 120);
    SendMessageW(
        p.hwnd,
        WM_HSCROLL,
        TB_THUMBPOSITION as usize,
        p.tracks[0] as isize,
    );
    RedrawWindow(
        p.hwnd,
        null(),
        null_mut(),
        RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
    );
    let captured = capture_client(p.hwnd, path);
    SendMessageW(p.hwnd, WM_COMMAND, 101, 0);
    let canceled = p.done && p.result.is_none();
    p.done = false;
    SendMessageW(p.hwnd, WM_COMMAND, 102, 0);
    let confirmed = p.result.as_deref() == Some("#00FF00");
    DestroyWindow(p.hwnd);
    captured?;
    if canceled && confirmed {
        Ok(())
    } else {
        Err("Paleta não preservou cancelamento ou confirmação".into())
    }
}
