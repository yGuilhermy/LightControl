use crate::{
    crypto,
    model::{Light, Preset, Routine, Scene},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
pub struct Credentials {
    pub id: String,
    pub ip: String,
    pub key: String,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        unsafe {
            crypto::wipe(self.key.as_bytes_mut());
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shortcut {
    pub action: String,
    pub modifiers: u32,
    pub key: u32,
}
pub const SHORTCUT_ACTIONS: [(&str, &str); 6] = [
    ("panel", "Abrir / recolher o painel"),
    ("toggle", "Ligar / desligar a luz"),
    ("on", "Ligar a luz"),
    ("off", "Desligar a luz"),
    ("brighter", "Aumentar brilho"),
    ("dimmer", "Diminuir brilho"),
];
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub name: String,
    pub start_hidden: bool,
    pub autostart: bool,
    pub pinned: bool,
    pub close_to_tray: bool,
    pub transition_ms: u32,
    pub hotkey_modifiers: u32,
    pub hotkey_key: u32,
    pub shortcuts: Option<Vec<Shortcut>>,
    pub presets: Vec<Preset>,
    pub favorites: Vec<String>,
    pub routines: Vec<Routine>,
    pub last_ip: String,
    pub scenes: Vec<Scene>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            name: "Quarto".into(),
            start_hidden: false,
            autostart: false,
            pinned: false,
            close_to_tray: true,
            transition_ms: 0,
            hotkey_modifiers: 3,
            hotkey_key: 0x4c,
            shortcuts: None,
            presets: vec![
                Preset {
                    name: "Leitura".into(),
                    light: Light {
                        brightness: 85,
                        temperature: 65,
                        ..Light::default()
                    },
                },
                Preset {
                    name: "Descanso".into(),
                    light: Light {
                        brightness: 25,
                        temperature: 0,
                        ..Light::default()
                    },
                },
                Preset {
                    name: "Concentrar".into(),
                    light: Light {
                        brightness: 100,
                        temperature: 100,
                        ..Light::default()
                    },
                },
                Preset {
                    name: "Cinema".into(),
                    light: Light {
                        mode: "colour".into(),
                        hue: 265,
                        saturation: 75,
                        value: 20,
                        ..Light::default()
                    },
                },
            ],
            favorites: vec![
                "#FFB86B".into(),
                "#FF6B82".into(),
                "#B497FF".into(),
                "#75D5CF".into(),
            ],
            routines: Vec::new(),
            last_ip: String::new(),
            scenes: vec![
                Scene {
                    name: "Aurora".into(),
                    effect: 2,
                    speed: 40,
                    brightness: 50,
                    colors: vec!["#FFB86B".into(), "#FF6B82".into(), "#B497FF".into()],
                },
                Scene {
                    name: "Oceano".into(),
                    effect: 2,
                    speed: 25,
                    brightness: 45,
                    colors: vec!["#75D5CF".into(), "#4679C9".into()],
                },
                Scene {
                    name: "Festa".into(),
                    effect: 1,
                    speed: 85,
                    brightness: 75,
                    colors: vec!["#FF4050".into(), "#4679FF".into(), "#66FF88".into()],
                },
            ],
        }
    }
}

impl Config {
    pub fn shortcuts(&self) -> Vec<Shortcut> {
        SHORTCUT_ACTIONS
            .iter()
            .map(|(action, _)| {
                self.shortcuts
                    .as_ref()
                    .and_then(|items| items.iter().find(|s| s.action == *action))
                    .cloned()
                    .unwrap_or_else(|| Shortcut {
                        action: (*action).into(),
                        modifiers: if *action == "panel" {
                            self.hotkey_modifiers
                        } else {
                            0
                        },
                        key: if *action == "panel" && self.hotkey_modifiers != 0 {
                            self.hotkey_key
                        } else {
                            0
                        },
                    })
            })
            .collect()
    }
}

pub fn default_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LightControl")
}
pub struct Store {
    pub root: PathBuf,
    pub config: Config,
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|_| "Não foi possível criar a pasta de dados.")?;
        let path = root.join("settings.json");
        let config = if path.exists() {
            serde_json::from_slice::<Config>(
                &fs::read(&path).map_err(|_| "Falha ao ler configurações.")?,
            )
            .map_err(|e| format!("Configuração inválida: {e}"))?
        } else {
            Config::default()
        };
        Ok(Self { root, config })
    }
    pub fn save(&self) -> Result<(), String> {
        atomic(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?,
        )
    }
    pub fn credentials(&self) -> Result<Option<Credentials>, String> {
        let path = self.root.join("device.dpapi");
        if !path.exists() {
            return Ok(None);
        }
        let encrypted = fs::read(path).map_err(|_| "Falha ao ler credencial protegida.")?;
        let mut raw = crypto::protect(&encrypted, true)?;
        let result = serde_json::from_slice::<Credentials>(&raw)
            .map_err(|_| "Arquivo de credenciais inválido.".into());
        crypto::wipe(&mut raw);
        result.map(Some)
    }
    pub fn set_credentials(&mut self, c: &Credentials) -> Result<(), String> {
        c.ip.parse::<std::net::Ipv4Addr>()
            .map_err(|_| "Endereço IPv4 inválido.")?;
        if c.key.len() != 16 || c.id.is_empty() {
            return Err("O ID é obrigatório e a local_key deve ter 16 bytes.".into());
        }
        let mut raw = serde_json::to_vec(c).map_err(|e| e.to_string())?;
        let result = crypto::protect(&raw, false);
        crypto::wipe(&mut raw);
        atomic(&self.root.join("device.dpapi"), &result?)?;
        self.config.last_ip = c.ip.clone();
        self.save()
    }
    pub fn import_tuya_manager(&mut self, root: &Path) -> Result<(), String> {
        let path = if root.is_dir() {
            root.join("core").join("tuya_manager.py")
        } else {
            root.to_path_buf()
        };
        let mut source = fs::read_to_string(path)
            .map_err(|_| "Não foi possível ler as credenciais do módulo Tuya informado.")?;
        let result = (|| {
            let id = literal(&source, "DEVICE_ID")?;
            let key = literal(&source, "LOCAL_KEY")?;
            let ip = literal(&source, "DEFAULT_IP")?;
            let c = Credentials { id, ip, key };
            self.set_credentials(&c)
        })();
        unsafe {
            crypto::wipe(source.as_bytes_mut());
        }
        result
    }
}
fn literal(source: &str, name: &str) -> Result<String, String> {
    for line in source.lines() {
        if let Some((lhs, rhs)) = line.split_once('=') {
            if lhs.trim() == name {
                let rhs = rhs.trim();
                let quote = rhs.chars().next().unwrap_or(' ');
                if quote == '\'' || quote == '"' {
                    if let Some(end) = rhs[1..].find(quote) {
                        return Ok(rhs[1..end + 1].to_string());
                    }
                }
            }
        }
    }
    Err(format!(
        "{name} não foi localizado como literal no módulo Tuya."
    ))
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("new");
    let mut file = fs::File::create(&tmp).map_err(|_| "Falha ao salvar dados.")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Falha ao gravar dados.")?;
    drop(file);
    let from: Vec<u16> = tmp
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let to: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 9) } == 0 {
        return Err("Falha ao substituir o arquivo de dados.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_is_data_not_execution() {
        assert_eq!(
            literal(
                "DEVICE_ID = 'abc'\nLOCAL_KEY = os.getenv('KEY')",
                "DEVICE_ID"
            )
            .unwrap(),
            "abc"
        );
        assert!(literal("LOCAL_KEY = os.getenv('KEY')", "LOCAL_KEY").is_err());
    }
}
