use super::*;
use serde_json::{json, Value};

pub(super) unsafe fn run(ui: &mut Ui) -> Result<(), String> {
    let baseline = ui.store.config.clone();
    let mut results = Vec::<Value>::new();
    let test = (|| -> Result<(), String> {
        macro_rules! check{($name:expr,$condition:expr)=>{let passed=$condition;results.push(json!({"test":$name,"passed":passed}));if !passed{return Err(format!("Falha no teste da interface: {}",$name));}};}
        let click = |ui: &mut Ui, id: u16| {
            let hwnd = ui.controls[&id];
            SendMessageW(ui.hwnd, WM_COMMAND, id as usize, hwnd as isize);
        };
        ui.update();
        ShowWindow(ui.hwnd, SW_SHOW);
        for (page, name) in [
            (Page::Home, "home"),
            (Page::Presets, "presets"),
            (Page::Scenes, "scenes"),
            (Page::Routines, "routines"),
            (Page::Settings, "settings"),
            (Page::Shortcuts, "shortcuts"),
        ] {
            ui.page(page);
            RedrawWindow(
                ui.hwnd,
                null(),
                null_mut(),
                RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
            capture_client(ui.hwnd, &ui.store.root.join(format!("{name}.bmp")))?;
        }
        ui.page(Page::Home);
        click(ui, 22);
        check!(
            "troca para cor cria os controles HSV",
            ui.controls.contains_key(&102) && ui.display.mode == "colour"
        );
        click(ui, 21);
        check!(
            "troca para branco remove saturação",
            !ui.controls.contains_key(&102) && ui.display.mode == "white"
        );
        SendMessageW(ui.controls[&100], TBM_SETPOS, 1, 33);
        SendMessageW(
            ui.hwnd,
            WM_HSCROLL,
            TB_THUMBPOSITION as usize,
            ui.controls[&100] as isize,
        );
        check!("slider envia o valor final", ui.display.brightness == 33);
        ui.page(Page::Presets);
        ui.set(306, "QA Preset");
        click(ui, 302);
        check!(
            "preset é persistido",
            Store::open(ui.store.root.clone())?.config.presets.len() == baseline.presets.len() + 1
        );
        SendMessageW(ui.controls[&300], LB_SETCURSEL, baseline.presets.len(), 0);
        click(ui, 304);
        check!(
            "exclusão de preset",
            ui.store.config.presets.len() == baseline.presets.len()
        );
        ui.set(311, "cor inválida");
        click(ui, 312);
        check!(
            "cor inválida não é salva",
            ui.store.config.favorites.len() == baseline.favorites.len()
        );
        ui.set(311, "#1234AB");
        click(ui, 312);
        check!(
            "favorita hexadecimal é persistida",
            ui.store
                .config
                .favorites
                .last()
                .is_some_and(|s| s == "#1234AB")
        );
        SendMessageW(ui.controls[&310], LB_SETCURSEL, baseline.favorites.len(), 0);
        click(ui, 314);
        ui.page(Page::Scenes);
        ui.set(701, "QA Scene");
        ui.set(703, "50");
        ui.set(704, "40");
        ui.set(705, "#FF0000, #0000FF");
        click(ui, 711);
        check!(
            "cena é persistida e gera payload válido",
            ui.store
                .config
                .scenes
                .last()
                .is_some_and(|s| s.name == "QA Scene" && s.payload().is_ok())
        );
        SendMessageW(ui.controls[&700], LB_SETCURSEL, baseline.scenes.len(), 0);
        click(ui, 712);
        check!(
            "exclusão de cena",
            ui.store.config.scenes.len() == baseline.scenes.len()
        );
        ui.page(Page::Routines);
        ui.set(401, "QA Routine");
        ui.set(402, "23");
        ui.set(403, "59");
        click(ui, 421);
        check!(
            "rotina é persistida com dias e horário",
            ui.store
                .config
                .routines
                .last()
                .is_some_and(|r| r.name == "QA Routine"
                    && r.hour == 23
                    && r.minute == 59
                    && r.days == 127)
        );
        SendMessageW(ui.controls[&400], LB_SETCURSEL, baseline.routines.len(), 0);
        ui.selected(400);
        click(ui, 410);
        click(ui, 421);
        check!(
            "edição de dias da rotina",
            ui.store
                .config
                .routines
                .last()
                .is_some_and(|r| r.days == 126)
        );
        SendMessageW(ui.controls[&400], LB_SETCURSEL, baseline.routines.len(), 0);
        click(ui, 422);
        check!(
            "exclusão de rotina",
            ui.store.config.routines.len() == baseline.routines.len()
        );
        ui.page(Page::Timer);
        ui.set(500, "0");
        click(ui, 501);
        check!("timer rejeita intervalo inválido", ui.timer.is_none());
        ui.set(500, "1");
        click(ui, 501);
        check!("timer local é criado", ui.timer.is_some());
        let deadline = ui.timer;
        ui.page(Page::Home);
        ui.page(Page::Timer);
        check!(
            "timer mantém prazo e duração ao navegar",
            ui.timer == deadline && ui.get(500) == "1"
        );
        RedrawWindow(
            ui.hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
        capture_client(ui.hwnd, &ui.store.root.join("timer.bmp"))?;
        click(ui, 502);
        check!("timer pode ser cancelado", ui.timer.is_none());
        ui.page(Page::Settings);
        ui.set(200, "Nome durante resize");
        ui.set(205, "0,5");
        let (w, h) = ui.size();
        SetWindowPos(
            ui.hwnd,
            null_mut(),
            0,
            0,
            ((w + 60.) * ui.dpi) as i32,
            ((h + 70.) * ui.dpi) as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        check!(
            "redimensionar preserva campos",
            ui.get(200) == "Nome durante resize" && ui.get(205) == "0,5"
        );
        ui.resize_mode(true);
        ui.rebuild();
        check!(
            "mini painel possui tamanho e controles próprios",
            ui.size().0 < 400. && ui.controls.contains_key(&26) && !ui.controls.contains_key(&10)
        );
        let controls = ui.controls.len();
        for _ in 0..20 {
            ui.resize_mode(false);
            ui.rebuild();
            ui.resize_mode(true);
            ui.rebuild();
        }
        let mut native_children = 0;
        let mut child = GetWindow(ui.hwnd, GW_CHILD);
        while !child.is_null() {
            native_children += 1;
            child = GetWindow(child, GW_HWNDNEXT);
        }
        check!(
            "vinte alternâncias não acumulam janelas de controles",
            native_children == controls
        );
        ui.hide();
        ui.tray_event_at = None;
        SendMessageW(ui.hwnd, TRAY, 0, WM_LBUTTONUP as isize);
        SendMessageW(ui.hwnd, TRAY, 0, NIN_SELECT as isize);
        SendMessageW(ui.hwnd, TRAY, 0, NIN_KEYSELECT as isize);
        let mut event: MSG = zeroed();
        let mut openings = 0;
        while PeekMessageW(&mut event, ui.hwnd, TRAY_OPEN, TRAY_OPEN, PM_REMOVE) != 0 {
            DispatchMessageW(&event);
            openings += 1;
        }
        SendMessageW(ui.hwnd, WM_ACTIVATE, WA_INACTIVE as usize, 0);
        check!(
            "clique da bandeja abre uma vez e resiste à desativação inicial",
            openings == 1 && IsWindowVisible(ui.hwnd) != 0 && ui.mini
        );
        ui.opened_at = Instant::now() - Duration::from_secs(1);
        ui.store.config.pinned = false;
        SendMessageW(ui.hwnd, WM_ACTIVATE, WA_INACTIVE as usize, 0);
        check!(
            "painel recolhe ao perder foco após a abertura",
            IsWindowVisible(ui.hwnd) == 0
        );
        let wait_for = |ui: &mut Ui, power: bool| {
            for _ in 0..50 {
                if ui
                    .worker
                    .state
                    .lock()
                    .map(|s| s.light.power == power)
                    .unwrap_or(false)
                {
                    ui.update();
                    return true;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            false
        };
        SendMessageW(ui.hwnd, WM_HOTKEY, 4, 0);
        check!(
            "atalho desliga com a janela oculta",
            wait_for(ui, false) && IsWindowVisible(ui.hwnd) == 0
        );
        SendMessageW(ui.hwnd, WM_HOTKEY, 3, 0);
        check!(
            "atalho liga com a janela oculta",
            wait_for(ui, true) && IsWindowVisible(ui.hwnd) == 0
        );
        SendMessageW(ui.hwnd, WM_HOTKEY, 2, 0);
        check!(
            "atalho alterna a energia sem abrir o painel",
            wait_for(ui, false) && IsWindowVisible(ui.hwnd) == 0
        );
        ui.resize_mode(false);
        ui.page(Page::Home);
        let old_level = ui.snapshot.light.brightness;
        let slider = ui.controls[&100];
        let mut rc: RECT = zeroed();
        GetClientRect(slider, &mut rc);
        let at_end = ((15. * ui.dpi) as isize) << 16 | ((rc.right - 2) as isize);
        SendMessageW(slider, WM_LBUTTONDOWN, 1, at_end);
        check!(
            "clique no trilho salta imediatamente ao destino",
            SendMessageW(slider, TBM_GETPOS, 0, 0) == 100
        );
        SendMessageW(slider, WM_LBUTTONUP, 0, at_end);
        ui.page(Page::Presets);
        ui.page(Page::Home);
        check!(
            "ajuste apagado persiste ao navegar",
            !ui.display.power && ui.display.brightness == 100
        );
        let mut staged = false;
        for _ in 0..100 {
            staged = ui
                .worker
                .state
                .lock()
                .map(|s| {
                    !s.light.power
                        && s.light.brightness == old_level
                        && s.staged.as_ref().is_some_and(|l| l.brightness == 100)
                })
                .unwrap_or(false);
            if staged {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        check!("preparar brilho não altera energia nem estado real", staged);
        ui.update();
        RedrawWindow(
            ui.hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
        capture_client(ui.hwnd, &ui.store.root.join("home-prepared.bmp"))?;
        SendMessageW(ui.hwnd, WM_HOTKEY, 3, 0);
        check!(
            "atalho liga aplicando o brilho preparado",
            wait_for(ui, true) && ui.display.brightness == 100 && ui.snapshot.staged.is_none()
        );
        check!(
            "seletor próprio cancela sem resultado e confirma a cor",
            picker::verify(ui.hwnd, &ui.store.root.join("color-picker.bmp")).is_ok()
        );
        ui.page(Page::Shortcuts);
        let original = ui.store.config.shortcuts();
        ui.shortcuts[1].key = original[0].key;
        ui.shortcuts[1].modifiers = original[0].modifiers;
        ui.save_shortcuts();
        check!(
            "atalhos duplicados não substituem a configuração ativa",
            ui.store.config.shortcuts() == original
        );
        ui.shortcuts = original;
        ui.shortcuts[1].key = 0x50;
        ui.shortcuts[1].modifiers = 6;
        ui.save_shortcuts();
        check!(
            "atalhos personalizados são persistidos",
            Store::open(ui.store.root.clone())?.config.shortcuts()[1].key == 0x50
        );
        ui.page(Page::Home);
        ui.animations = true;
        ui.display.power = true;
        ui.sync_animation();
        SendMessageW(ui.hwnd, WM_TIMER, ANIMATION_TIMER, 0);
        check!("animação evolui com a luz acesa", ui.glow > 0.);
        ui.hide();
        check!(
            "janela oculta libera os recursos gráficos",
            ui.renderer.is_none() && ui.widgets.is_none() && ui.graphics.is_none()
        );
        Ok(())
    })();
    ui.store.config = baseline;
    let _ = ui.store.save();
    let report = json!({"passed":test.is_ok(),"tests":results,"error":test.as_ref().err()});
    std::fs::write(
        ui.store.root.join("ui-tests.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    test
}
