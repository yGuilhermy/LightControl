use crate::{
    model::{Dps, Light, Snapshot},
    protocol::{self, Client},
    store::{Config, Store},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime,
    UI::WindowsAndMessaging::PostMessageW,
};

pub const UPDATE: u32 = 0x8001;
pub enum Command {
    Set(Dps, Instant),
    Stage(Light),
    Power(Option<bool>, Instant),
    Adjust(i16, Instant),
    Preset(Light, u32, Instant),
    Refresh,
    Discover,
    Reconnect,
    Config(Config),
    Timer(Option<u64>),
    Stop,
}
pub struct Worker {
    pub tx: Sender<Command>,
    pub state: Arc<Mutex<Snapshot>>,
    join: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(root: PathBuf, hwnd: usize, preview: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let shared = state.clone();
        let join = thread::spawn(move || run(root, hwnd, preview, rx, shared));
        Self {
            tx,
            state,
            join: Some(join),
        }
    }
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
fn publish(hwnd: usize, shared: &Arc<Mutex<Snapshot>>, state: &Snapshot) {
    if let Ok(mut s) = shared.lock() {
        *s = state.clone();
    }
    unsafe {
        PostMessageW(hwnd as _, UPDATE, 0, 0);
    }
}
fn apply(state: &mut Snapshot, d: Dps) {
    state.light.merge(&d);
    if state.light.power {
        state.staged = None;
    }
    if let Some(s) = d.get("25").and_then(|v| v.as_str()) {
        state.scene = s.into();
    }
    if let Some(n) = d.get("26").and_then(|v| v.as_u64()) {
        if n == 0 {
            state.countdown_deadline = None;
        } else if n != state.countdown || state.countdown_deadline.is_none() {
            state.countdown_deadline = Some(protocol::epoch() + n);
        }
        state.countdown = n;
    }
    if let Some(b) = d.get("34").and_then(|v| v.as_bool()) {
        state.do_not_disturb = b;
    }
    state.known = true;
    state.online = true;
    state.pending = false;
    state.capabilities.extend(d.keys().cloned());
    state.message = "Conectada pela rede local".into();
}

fn power_dps(state: &Snapshot, power: bool) -> Dps {
    let mut dps = if power {
        state.staged.as_ref().map(Light::as_dps).unwrap_or_default()
    } else {
        Dps::new()
    };
    dps.insert("20".into(), serde_json::json!(power));
    if !power && state.countdown > 0 && state.capabilities.contains("26") {
        dps.insert("26".into(), serde_json::json!(0));
    }
    dps
}

fn run(
    root: PathBuf,
    hwnd: usize,
    preview: bool,
    rx: Receiver<Command>,
    shared: Arc<Mutex<Snapshot>>,
) {
    let mut state = Snapshot::default();
    let mut config = Store::open(root.clone())
        .map(|s| s.config)
        .unwrap_or_default();
    if preview {
        state.known = true;
        state.message = "Prévia visual · sem conexão com a lâmpada".into();
        publish(hwnd, &shared, &state);
        while let Ok(c) = rx.recv() {
            match c {
                Command::Stop => break,
                Command::Stage(mut light) => {
                    light.power = false;
                    state.staged = Some(light);
                    publish(hwnd, &shared, &state);
                }
                Command::Set(d, _) => {
                    state.light.merge(&d);
                    if state.light.power {
                        state.staged = None;
                    }
                    publish(hwnd, &shared, &state);
                }
                Command::Power(power, _) => {
                    let d = power_dps(&state, power.unwrap_or(!state.light.power));
                    state.light.merge(&d);
                    if state.light.power {
                        state.staged = None;
                    }
                    publish(hwnd, &shared, &state);
                }
                Command::Adjust(delta, _) => {
                    let mut light = state.staged.clone().unwrap_or_else(|| state.light.clone());
                    let level = (light.level() as i16 + delta).clamp(1, 100) as u16;
                    if light.mode == "colour" {
                        light.value = level;
                    } else {
                        light.brightness = level;
                    }
                    if state.light.power {
                        state.light = light;
                    } else {
                        light.power = false;
                        state.staged = Some(light);
                    }
                    publish(hwnd, &shared, &state);
                }
                Command::Preset(l, _, _) => {
                    state.light = l;
                    state.staged = None;
                    publish(hwnd, &shared, &state);
                }
                _ => {}
            }
        }
        return;
    }
    let mut client: Option<Client> = None;
    let mut next_connect = Instant::now();
    let mut backoff = 2u64;
    let mut heartbeat = Instant::now();
    let mut query = Instant::now();
    let mut timer: Option<u64> = None;
    let mut last_minute = protocol::epoch() / 60;
    let mut fired: HashMap<usize, u32> = HashMap::new();
    let mut fade: Option<(Light, Light, Instant, u32)> = None;
    let mut fade_tick = Instant::now();
    let mut connected_at = Instant::now();
    let mut next_discovery = Instant::now();
    publish(hwnd, &shared, &state);
    let mut queued = None;
    loop {
        let wait = if fade.is_some() {
            Duration::from_millis(120)
        } else {
            Duration::from_millis(500)
        };
        let command = match queued
            .take()
            .map(Ok)
            .unwrap_or_else(|| rx.recv_timeout(wait))
        {
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(c) => Some(c),
            Err(_) => None,
        };
        let mut write: Option<Dps> = None;
        let mut force_query = false;
        match command {
            Some(Command::Stage(mut light)) => {
                if !state.light.power {
                    light.power = false;
                    state.staged = Some(light);
                    publish(hwnd, &shared, &state);
                }
            }
            Some(Command::Config(c)) => {
                config = c;
                fired.clear();
            }
            Some(Command::Timer(t)) => {
                timer = t;
                state.message = if t.is_some() {
                    "Timer para desligar ativado".into()
                } else {
                    "Timer cancelado".into()
                };
                publish(hwnd, &shared, &state);
            }
            Some(Command::Reconnect) => {
                client = None;
                next_connect = Instant::now();
                backoff = 2;
                fade = None;
            }
            Some(Command::Discover) => {
                state.message = "Procurando anúncios na rede local…".into();
                publish(hwnd, &shared, &state);
                match Store::open(root.clone())
                    .and_then(|s| s.credentials())
                    .and_then(|c| c.ok_or("Importe as credenciais primeiro.".into()))
                {
                    Ok(mut c) => match protocol::discover(&c.id, Duration::from_secs(6)) {
                        Ok(ip) => {
                            c.ip = ip;
                            if let Ok(mut s) = Store::open(root.clone()) {
                                let _ = s.set_credentials(&c);
                            }
                            client = None;
                            next_connect = Instant::now();
                        }
                        Err(e) => state.message = e,
                    },
                    Err(e) => state.message = e,
                }
                publish(hwnd, &shared, &state);
            }
            Some(Command::Refresh) => {
                if client.is_none() {
                    next_connect = Instant::now();
                } else {
                    force_query = true;
                }
            }
            Some(Command::Set(mut d, issued)) => {
                if issued < connected_at || client.is_none() {
                    state.message = "Comando cancelado após mudança de conexão".into();
                    publish(hwnd, &shared, &state);
                    continue;
                }
                fade = None;
                while let Ok(next) = rx.try_recv() {
                    match next {
                        Command::Set(new, t) if t >= connected_at => d.extend(new),
                        Command::Stop => return,
                        other => {
                            queued = Some(other);
                            break;
                        }
                    }
                }
                write = Some(d);
            }
            Some(Command::Power(power, issued)) => {
                if issued < connected_at || client.is_none() || !state.online {
                    continue;
                }
                fade = None;
                write = Some(power_dps(&state, power.unwrap_or(!state.light.power)));
            }
            Some(Command::Adjust(delta, issued)) => {
                if issued < connected_at || client.is_none() || !state.online {
                    continue;
                }
                fade = None;
                let mut light = state.staged.clone().unwrap_or_else(|| state.light.clone());
                if light.mode == "scene" {
                    state.message = "Para ajustar brilho, selecione Branco ou Cor.".into();
                    publish(hwnd, &shared, &state);
                    continue;
                }
                let level = (light.level() as i16 + delta).clamp(1, 100) as u16;
                if light.mode == "colour" {
                    light.value = level;
                } else {
                    light.brightness = level;
                }
                if !state.light.power {
                    light.power = false;
                    state.staged = Some(light);
                    publish(hwnd, &shared, &state);
                } else {
                    write = Some(light.as_dps());
                }
            }
            Some(Command::Preset(l, ms, issued)) => {
                if issued < connected_at || client.is_none() {
                    continue;
                }
                if ms > 0 && state.online {
                    fade = Some((state.light.clone(), l, Instant::now(), ms));
                    fade_tick = Instant::now();
                } else {
                    fade = None;
                    write = Some(l.as_dps());
                }
            }
            _ => {}
        }
        let now = protocol::epoch();
        if timer.is_some_and(|t| now >= t) {
            timer = None;
            write = Some(serde_json::json!({"20":false}).as_object().unwrap().clone());
        }
        let minute = now / 60;
        if minute != last_minute {
            let elapsed = minute.saturating_sub(last_minute);
            last_minute = minute;
            if elapsed <= 2 {
                let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
                unsafe {
                    GetLocalTime(&mut local);
                }
                let date =
                    local.wYear as u32 * 10000 + local.wMonth as u32 * 100 + local.wDay as u32;
                for (i, r) in config.routines.iter().enumerate() {
                    if r.enabled
                        && r.hour == local.wHour
                        && r.minute == local.wMinute
                        && (r.days & (1 << local.wDayOfWeek)) != 0
                        && fired.get(&i) != Some(&date)
                    {
                        fired.insert(i, date);
                        if state.online {
                            write = Some(r.light.as_dps());
                        } else {
                            state.message = "Rotina ignorada: lâmpada offline".into();
                            publish(hwnd, &shared, &state);
                        }
                    }
                }
            } else {
                state.message = "Retomada detectada; rotinas perdidas não foram repetidas".into();
                publish(hwnd, &shared, &state);
            }
        }
        if let Some((start, target, began, ms)) = &fade {
            if Instant::now() >= fade_tick {
                let p = (began.elapsed().as_millis() as f32 / (*ms).max(1) as f32).min(1.);
                let mut l = target.clone();
                let blend = |a: u16, b: u16| (a as f32 + (b as f32 - a as f32) * p).round() as u16;
                l.brightness = blend(start.brightness, target.brightness);
                l.temperature = blend(start.temperature, target.temperature);
                l.hue = blend(start.hue, target.hue);
                l.saturation = blend(start.saturation, target.saturation);
                l.value = blend(start.value, target.value);
                write = Some(l.as_dps());
                fade_tick = Instant::now() + Duration::from_millis(200);
                if p >= 1. {
                    fade = None;
                }
            }
        }
        if client.is_none() && Instant::now() >= next_connect {
            state.online = false;
            state.pending = false;
            state.message = "Conectando à lâmpada…".into();
            publish(hwnd, &shared, &state);
            let result = Store::open(root.clone())
                .and_then(|s| s.credentials())
                .and_then(|c| c.ok_or("Importe as credenciais em Configurações.".into()))
                .and_then(|mut c| match Client::connect(&c.ip, &c.key) {
                    Ok(client) => Ok(client),
                    Err(error) if backoff >= 4 && Instant::now() >= next_discovery => {
                        next_discovery = Instant::now() + Duration::from_secs(60);
                        state.message = "Procurando a lâmpada após falha de conexão…".into();
                        publish(hwnd, &shared, &state);
                        let ip =
                            protocol::discover(&c.id, Duration::from_secs(5)).map_err(|_| error)?;
                        let mut client = Client::connect(&ip, &c.key)?;
                        client.query()?;
                        c.ip = ip;
                        if let Ok(mut s) = Store::open(root.clone()) {
                            let _ = s.set_credentials(&c);
                        }
                        Ok(client)
                    }
                    Err(error) => Err(error),
                })
                .and_then(|mut c| {
                    let d = c.query()?;
                    Ok((c, d))
                });
            match result {
                Ok((c, d)) => {
                    client = Some(c);
                    connected_at = Instant::now();
                    apply(&mut state, d);
                    backoff = 2;
                    heartbeat = Instant::now();
                    query = Instant::now();
                }
                Err(e) => {
                    state.message = e;
                    next_connect = Instant::now() + Duration::from_secs(backoff);
                    backoff = (backoff * 2).min(60);
                }
            }
            write = None;
            publish(hwnd, &shared, &state);
        }
        if let Some(c) = client.as_mut() {
            let started = Instant::now();
            let is_write = write.is_some();
            let resets_countdown = write.as_ref().is_some_and(|d| d.contains_key("26"));
            if is_write {
                state.pending = true;
                publish(hwnd, &shared, &state);
            }
            let result = if let Some(d) = write {
                c.set(&d).map(Some)
            } else if force_query || query.elapsed() >= Duration::from_secs(30) {
                query = Instant::now();
                c.query().map(Some)
            } else if heartbeat.elapsed() >= Duration::from_secs(10) {
                heartbeat = Instant::now();
                c.heartbeat().map(|_| None)
            } else {
                c.poll()
            };
            match result {
                Ok(Some(d)) => {
                    if is_write {
                        state.latency_ms = started.elapsed().as_millis();
                    }
                    if resets_countdown {
                        state.countdown_deadline = None;
                    }
                    apply(&mut state, d);
                    if is_write && state.light.power {
                        state.staged = None;
                    }
                    publish(hwnd, &shared, &state);
                }
                Ok(None) => {}
                Err(e) => {
                    client = None;
                    fade = None;
                    state.online = false;
                    state.pending = false;
                    state.message = e;
                    next_connect = Instant::now() + Duration::from_secs(2);
                    publish(hwnd, &shared, &state);
                }
            }
        } else if write.is_some() {
            state.message = "Comando não enviado: lâmpada offline".into();
            publish(hwnd, &shared, &state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turning_off_cancels_native_countdown_in_same_write() {
        let mut s = Snapshot {
            countdown: 60,
            ..Snapshot::default()
        };
        s.capabilities.insert("26".into());
        assert_eq!(
            power_dps(&s, false),
            serde_json::json!({"20":false,"26":0})
                .as_object()
                .unwrap()
                .clone()
        );
        assert!(!power_dps(&s, true).contains_key("26"));
        s.capabilities.clear();
        assert!(!power_dps(&s, false).contains_key("26"));
    }
    #[test]
    fn power_on_applies_prepared_values_together() {
        let mut s = Snapshot::default();
        s.light.power = false;
        let mut prepared = s.light.clone();
        prepared.set_hex("#FF0000").unwrap();
        prepared.value = 42;
        s.staged = Some(prepared.clone());
        let d = power_dps(&s, true);
        let mut actual = s.light.clone();
        actual.merge(&d);
        prepared.power = true;
        assert!(actual == prepared);
        assert_eq!(power_dps(&s, false).len(), 1);
        assert!(!s.light.power);
    }
    #[test]
    fn repeated_status_preserves_timer_deadline() {
        let mut s = Snapshot {
            countdown: 120,
            countdown_deadline: Some(12345),
            ..Snapshot::default()
        };
        apply(
            &mut s,
            serde_json::json!({"26":120}).as_object().unwrap().clone(),
        );
        assert_eq!(s.countdown_deadline, Some(12345));
        apply(
            &mut s,
            serde_json::json!({"26":0}).as_object().unwrap().clone(),
        );
        assert_eq!(s.countdown_deadline, None);
        apply(
            &mut s,
            serde_json::json!({"26":120}).as_object().unwrap().clone(),
        );
        assert!(s.countdown_deadline.unwrap() >= protocol::epoch() + 119);
    }
}
