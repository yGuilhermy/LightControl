use super::*;

pub(super) fn validate(items: &[Shortcut]) -> Result<(), String> {
    let mut used = std::collections::BTreeSet::new();
    for item in items.iter().filter(|s| s.key != 0) {
        if item.modifiers == 0 || item.modifiers & !15 != 0 || item.key > 255 {
            return Err("Combine a tecla com Ctrl, Alt, Shift ou Windows.".into());
        }
        if !used.insert((item.modifiers, item.key)) {
            return Err("Cada ação precisa de um atalho diferente.".into());
        }
    }
    Ok(())
}
pub(super) unsafe fn label(s: &Shortcut) -> String {
    if s.key == 0 {
        return "Definir atalho".into();
    }
    let mut parts = Vec::new();
    for (flag, name) in [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")] {
        if s.modifiers & flag != 0 {
            parts.push(name.to_string());
        }
    }
    let scan = MapVirtualKeyW(s.key, MAPVK_VK_TO_VSC);
    let extended = matches!(s.key, 0x21..=0x28 | 0x2d | 0x2e);
    let mut name = [0u16; 80];
    let n = GetKeyNameTextW(
        ((scan << 16) | if extended { 1 << 24 } else { 0 }) as i32,
        name.as_mut_ptr(),
        name.len() as i32,
    );
    parts.push(if n > 0 {
        String::from_utf16_lossy(&name[..n as usize])
    } else {
        format!("Tecla {}", s.key)
    });
    parts.join(" + ")
}
impl Ui {
    pub(super) unsafe fn shortcut_controls(&mut self, width: f32) {
        if self.shortcuts.is_empty() {
            self.shortcuts = self.store.config.shortcuts();
        }
        for i in 0..6 {
            self.button(
                820 + i as u16,
                &label(&self.shortcuts[i]),
                width - 352.,
                174. + i as f32 * 57.,
                300.,
                42.,
            );
        }
        self.button(830, "Salvar atalhos", 40., 543., 220., 40.);
        self.button(831, "Descartar alterações", 274., 543., 220., 40.);
    }
    pub(super) unsafe fn install_shortcuts(&self, items: &[Shortcut]) -> Result<(), String> {
        validate(items)?;
        if self.preview {
            return Ok(());
        }
        for id in 1..=6 {
            UnregisterHotKey(self.hwnd, id);
        }
        for (i, shortcut) in items.iter().enumerate().filter(|(_, s)| s.key != 0) {
            if RegisterHotKey(
                self.hwnd,
                i as i32 + 1,
                shortcut.modifiers | MOD_NOREPEAT,
                shortcut.key,
            ) == 0
            {
                for id in 1..=6 {
                    UnregisterHotKey(self.hwnd, id);
                }
                return Err(format!(
                    "{} está em uso ou reservado pelo Windows. Escolha outra combinação.",
                    label(shortcut)
                ));
            }
        }
        Ok(())
    }
    pub(super) unsafe fn begin_capture(&mut self, index: usize) {
        self.cancel_capture();
        for id in 1..=6 {
            UnregisterHotKey(self.hwnd, id);
        }
        self.capturing = Some(index);
        self.set(820 + index as u16, "Pressione a combinação…");
        self.notice = "Use Ctrl, Alt, Shift ou Win + uma tecla. Delete apaga; Esc cancela.".into();
        SetFocus(self.controls[&(820 + index as u16)]);
        self.invalidate();
    }
    pub(super) unsafe fn cancel_capture(&mut self) {
        if let Some(i) = self.capturing.take() {
            if let Some(shortcut) = self.shortcuts.get(i) {
                self.set(820 + i as u16, &label(shortcut));
            }
            self.register_hotkey();
        }
    }
    pub(super) unsafe fn capture_key(&mut self, key: u32) {
        let Some(i) = self.capturing else {
            return;
        };
        if key == VK_ESCAPE as u32 {
            self.cancel_capture();
            self.notice.clear();
            self.invalidate();
            return;
        }
        if matches!(key, 0x10..=0x12 | 0x5b | 0x5c | 0xa0..=0xa5) {
            return;
        }
        if key == VK_DELETE as u32 || key == VK_BACK as u32 {
            self.shortcuts[i].key = 0;
            self.shortcuts[i].modifiers = 0;
        } else {
            let mut modifiers = 0;
            for (vk, flag) in [
                (VK_CONTROL, 2),
                (VK_MENU, 1),
                (VK_SHIFT, 4),
                (VK_LWIN, 8),
                (VK_RWIN, 8),
            ] {
                if GetKeyState(vk as i32) < 0 {
                    modifiers |= flag;
                }
            }
            if modifiers == 0 {
                return;
            }
            self.shortcuts[i].key = key;
            self.shortcuts[i].modifiers = modifiers;
        }
        self.cancel_capture();
        self.notice = "Combinação escolhida. Salve para ativar.".into();
        self.invalidate();
    }
    pub(super) unsafe fn save_shortcuts(&mut self) {
        self.cancel_capture();
        let old = self.store.config.shortcuts();
        if let Err(e) = self.install_shortcuts(&self.shortcuts) {
            let _ = self.install_shortcuts(&old);
            self.notice = e;
            self.invalidate();
            return;
        }
        let previous = self.store.config.shortcuts.clone();
        self.store.config.shortcuts = Some(self.shortcuts.clone());
        match self.store.save() {
            Ok(()) => {
                self.notice = "Atalhos prontos. Funcionam também com o painel recolhido.".into()
            }
            Err(e) => {
                self.store.config.shortcuts = previous;
                let _ = self.install_shortcuts(&old);
                self.notice = e;
            }
        }
        self.invalidate();
    }
    pub(super) unsafe fn execute_shortcut(&mut self, index: usize) {
        if self.capturing.is_some() {
            return;
        }
        let items = self.store.config.shortcuts();
        let Some(shortcut) = items.get(index) else {
            return;
        };
        if shortcut.action == "panel" {
            if IsWindowVisible(self.hwnd) != 0 && self.mini {
                self.hide();
            } else {
                self.show(true);
            }
            return;
        }
        if !self.snapshot.online && !self.preview {
            self.notice = "Lâmpada offline. Verifique a conexão em Ajustes.".into();
            self.invalidate();
            return;
        }
        self.pending_send = None;
        let now = Instant::now();
        match shortcut.action.as_str() {
            "toggle" => self.send(Command::Power(None, now)),
            "on" => self.send(Command::Power(Some(true), now)),
            "off" => self.send(Command::Power(Some(false), now)),
            "brighter" => self.send(Command::Adjust(10, now)),
            "dimmer" => self.send(Command::Adjust(-10, now)),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_shortcut_migrates_without_resetting() {
        let config: crate::store::Config =
            serde_json::from_str(r#"{"hotkey_modifiers":6,"hotkey_key":80}"#).unwrap();
        let items = config.shortcuts();
        assert_eq!((items[0].modifiers, items[0].key), (6, 80));
        assert!(items[1..].iter().all(|s| s.key == 0));
    }
    #[test]
    fn duplicate_and_unmodified_shortcuts_are_rejected() {
        let mut items = crate::store::Config::default().shortcuts();
        items[1].key = items[0].key;
        items[1].modifiers = items[0].modifiers;
        assert!(validate(&items).is_err());
        items[1].modifiers = 0;
        assert!(validate(&items).is_err());
        items[1].key = 0;
        assert!(validate(&items).is_ok());
    }
}
