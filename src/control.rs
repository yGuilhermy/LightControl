use crate::{
    model::{Dps, Light},
    protocol::{self, Client},
    store::{Credentials, Store},
};
use serde_json::{json, Value};
use std::{net::Ipv4Addr, time::Duration};

fn credential(store: &Store) -> Result<Credentials, String> {
    store.credentials()?.ok_or_else(|| {
        "Configure a credencial primeiro com lightcontrol config set ou config import-manager."
            .into()
    })
}

pub fn configuration(store: &Store) -> Result<Value, String> {
    let settings = store.credentials()?;
    Ok(json!({
        "name": store.config.name,
        "configured": settings.is_some(),
        "ip": settings.as_ref().map(|c| &c.ip),
    }))
}

pub fn configure(
    store: &mut Store,
    ip: Option<Ipv4Addr>,
    name: Option<&str>,
    new_credential: Option<Credentials>,
) -> Result<Value, String> {
    if let Some(name) = name {
        if name.trim().is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control)
        {
            return Err("Use um nome de 1 a 64 caracteres.".into());
        }
        store.config.name = name.trim().into();
    }
    if ip.is_some() || new_credential.is_some() {
        let mut creds = match new_credential {
            Some(c) => c,
            None => credential(store)?,
        };
        if let Some(ip) = ip {
            creds.ip = ip.to_string();
        }
        let mut client = Client::connect(&creds.ip, &creds.key)?;
        query(&mut client)?;
        store.set_credentials(&creds)?;
    } else if name.is_some() {
        store.save()?;
    }
    configuration(store)
}

pub fn discover(store: &mut Store) -> Result<Value, String> {
    let creds = credential(store)?;
    let ip = protocol::discover(&creds.id, Duration::from_secs(6))?;
    let ip = ip.parse().map_err(|_| "Endereço IPv4 inválido.")?;
    configure(store, Some(ip), None, None)
}

pub enum Action<'a> {
    Status,
    Power(bool),
    Brightness(u16),
    Temperature(u16),
    Color(&'a str),
    Timer(u16),
    Preset(&'a str),
    Scene(&'a str),
    DoNotDisturb(bool),
}

fn query(client: &mut Client) -> Result<Dps, String> {
    let status = client.query()?;
    if status.get("20").and_then(Value::as_bool).is_none() {
        return Err("A lâmpada não retornou o estado de energia.".into());
    }
    Ok(status)
}

pub fn execute(store: &Store, action: Action<'_>) -> Result<Value, String> {
    let creds = credential(store)?;
    let mut client = Client::connect(&creds.ip, &creds.key)?;
    let mut observed = query(&mut client)?;
    let mut light = Light::default();
    light.merge(&observed);
    let writes = match action {
        Action::Status => None,
        Action::Power(on) => {
            let mut dps = Dps::from_iter([("20".into(), json!(on))]);
            if !on && observed.get("26").and_then(Value::as_u64).unwrap_or(0) > 0 {
                dps.insert("26".into(), json!(0));
            }
            Some(dps)
        }
        Action::Brightness(value) => {
            if !(1..=100).contains(&value) {
                return Err("Brilho deve estar entre 1 e 100.".into());
            }
            let (dp, value) = match light.mode.as_str() {
                "colour" => {
                    light.value = value;
                    ("24", json!(light.hsv()))
                }
                "white" => ("22", json!(value * 10)),
                _ => return Err("Selecione branco ou cor antes de ajustar o brilho.".into()),
            };
            Some(Dps::from_iter([(dp.into(), value)]))
        }
        Action::Temperature(value) if value <= 100 => Some(Dps::from_iter([
            ("21".into(), json!("white")),
            ("23".into(), json!(value * 10)),
        ])),
        Action::Temperature(_) => return Err("Temperatura deve estar entre 0 e 100.".into()),
        Action::Color(color) => {
            light.set_hex(color)?;
            Some(Dps::from_iter([
                ("21".into(), json!("colour")),
                ("24".into(), json!(light.hsv())),
            ]))
        }
        Action::Timer(minutes) if minutes <= 1440 => {
            if !observed.contains_key("26") {
                return Err("A lâmpada não informou suporte ao timer.".into());
            }
            if minutes > 0 && !light.power {
                return Err("Ligue a lâmpada antes de ativar o timer.".into());
            }
            Some(Dps::from_iter([(
                "26".into(),
                json!(u32::from(minutes) * 60),
            )]))
        }
        Action::Timer(_) => return Err("Use um timer de 0 a 1440 minutos.".into()),
        Action::Preset(name) => {
            let preset = store
                .config
                .presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or("Preset não encontrado.")?;
            Some(preset.light.as_dps())
        }
        Action::Scene(name) => {
            let scene = store
                .config
                .scenes
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(name))
                .ok_or("Cena não encontrada.")?;
            Some(Dps::from_iter([
                ("20".into(), json!(true)),
                ("21".into(), json!("scene")),
                ("25".into(), json!(scene.payload()?)),
            ]))
        }
        Action::DoNotDisturb(enabled) => {
            if !observed.contains_key("34") {
                return Err("A lâmpada não informou suporte a Não perturbe.".into());
            }
            Some(Dps::from_iter([("34".into(), json!(enabled))]))
        }
    };

    if let Some(writes) = writes {
        observed.extend(client.set(&writes)?);
        light.merge(&observed);
    }
    Ok(json!({
        "name": store.config.name,
        "power": light.power,
        "mode": light.mode,
        "brightness": light.level(),
        "temperature": light.temperature,
        "color": light.hex(),
        "timer_seconds": observed.get("26"),
        "do_not_disturb": observed.get("34"),
    }))
}
