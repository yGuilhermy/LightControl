use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

pub type Dps = Map<String, Value>;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
    pub power: bool,
    pub mode: String,
    pub brightness: u16,
    pub temperature: u16,
    pub hue: u16,
    pub saturation: u16,
    pub value: u16,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            power: true,
            mode: "white".into(),
            brightness: 70,
            temperature: 40,
            hue: 35,
            saturation: 65,
            value: 70,
        }
    }
}

impl Light {
    pub fn merge(&mut self, d: &Dps) {
        if let Some(v) = d.get("20").and_then(Value::as_bool) {
            self.power = v;
        }
        if let Some(v) = d.get("21").and_then(Value::as_str) {
            self.mode = v.to_owned();
        }
        if let Some(v) = d.get("22").and_then(Value::as_u64) {
            self.brightness = (v.clamp(10, 1000) as f64 / 10.).round() as u16;
        }
        if let Some(v) = d.get("23").and_then(Value::as_u64) {
            self.temperature = (v.min(1000) / 10) as u16;
        }
        if let Some(v) = d.get("24").and_then(Value::as_str).and_then(parse_hsv) {
            self.hue = v.0;
            self.saturation = v.1;
            self.value = v.2;
        }
    }
    pub fn level(&self) -> u16 {
        if self.mode == "colour" {
            self.value
        } else {
            self.brightness
        }
    }
    pub fn hsv(&self) -> String {
        format!(
            "{:04x}{:04x}{:04x}",
            self.hue.min(360),
            self.saturation.min(100) * 10,
            self.value.min(100) * 10
        )
    }
    pub fn as_dps(&self) -> Dps {
        if self.mode == "colour" {
            json!({"20":self.power,"21":"colour","24":self.hsv()})
                .as_object()
                .unwrap()
                .clone()
        } else {
            json!({"20":self.power,"21":"white","22":raw_brightness(self.brightness),"23":self.temperature.min(100)*10}).as_object().unwrap().clone()
        }
    }
    pub fn rgb(&self) -> [u8; 3] {
        hsv_rgb(
            self.hue as f32,
            self.saturation as f32 / 100.,
            self.value as f32 / 100.,
        )
    }
    pub fn hex(&self) -> String {
        let c = self.rgb();
        format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
    }
    pub fn set_hex(&mut self, s: &str) -> Result<(), String> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 || !s.is_ascii() {
            return Err("Use uma cor hexadecimal com seis dígitos.".into());
        }
        let n = u32::from_str_radix(s, 16).map_err(|_| "Cor hexadecimal inválida.")?;
        let (h, s, v) = rgb_hsv([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
        self.hue = h;
        self.saturation = s;
        self.value = v;
        self.mode = "colour".into();
        Ok(())
    }
}

pub fn raw_brightness(percent: u16) -> u16 {
    percent.clamp(1, 100) * 10
}
pub fn parse_hsv(s: &str) -> Option<(u16, u16, u16)> {
    if s.len() != 12 || !s.is_ascii() {
        return None;
    }
    Some((
        u16::from_str_radix(&s[..4], 16).ok()?.min(360),
        (u16::from_str_radix(&s[4..8], 16).ok()?.min(1000) / 10),
        (u16::from_str_radix(&s[8..12], 16).ok()?.min(1000) / 10),
    ))
}
pub fn hsv_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1. - ((h / 60.) % 2. - 1.).abs());
    let m = v - c;
    let a = match (h % 360. / 60.) as u32 {
        0 => [c, x, 0.],
        1 => [x, c, 0.],
        2 => [0., c, x],
        3 => [0., x, c],
        4 => [x, 0., c],
        _ => [c, 0., x],
    };
    a.map(|n| ((n + m) * 255.).round().clamp(0., 255.) as u8)
}
pub fn rgb_hsv(c: [u8; 3]) -> (u16, u16, u16) {
    let [r, g, b] = c.map(|x| x as f32 / 255.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0. {
        0.
    } else if max == r {
        60. * ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / d + 2.)
    } else {
        60. * ((r - g) / d + 4.)
    };
    (
        h.round() as u16,
        if max == 0. {
            0
        } else {
            (d / max * 100.).round() as u16
        },
        (max * 100.).round() as u16,
    )
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub light: Light,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub effect: u8,
    pub speed: u8,
    pub brightness: u16,
    pub colors: Vec<String>,
}
impl Scene {
    pub fn payload(&self) -> Result<String, String> {
        if self.effect > 2
            || self.speed > 100
            || self.speed == 0
            || self.brightness == 0
            || self.brightness > 100
            || self.colors.is_empty()
            || self.colors.len() > 8
        {
            return Err("A cena precisa de 1 a 8 cores e ajustes entre 1 e 100.".into());
        }
        let mut s = "00".to_string();
        for color in &self.colors {
            let mut l = Light::default();
            l.set_hex(color)?;
            l.value = self.brightness;
            s.push_str(&format!(
                "{:02x}{:02x}{:02x}{}00000000",
                101 - self.speed,
                101 - self.speed,
                self.effect,
                l.hsv()
            ));
        }
        Ok(s)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Routine {
    pub name: String,
    pub hour: u16,
    pub minute: u16,
    pub days: u8,
    pub enabled: bool,
    pub light: Light,
}
impl Default for Routine {
    fn default() -> Self {
        Self {
            name: "Nova rotina".into(),
            hour: 22,
            minute: 0,
            days: 127,
            enabled: true,
            light: Light::default(),
        }
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub light: Light,
    pub staged: Option<Light>,
    pub countdown_deadline: Option<u64>,
    pub known: bool,
    pub online: bool,
    pub pending: bool,
    pub message: String,
    pub capabilities: BTreeSet<String>,
    pub latency_ms: u128,
    pub scene: String,
    pub countdown: u64,
    pub do_not_disturb: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn white_and_colour_are_independent() {
        let mut l = Light::default();
        l.merge(
            json!({"22":505,"24":"007803e801f4","21":"colour"})
                .as_object()
                .unwrap(),
        );
        assert_eq!(l.brightness, 51);
        assert_eq!(l.hue, 120);
        assert_eq!(l.level(), 50);
        l.mode = "white".into();
        assert_eq!(l.level(), 51);
        assert_eq!(l.rgb(), [0, 128, 0]);
    }
    #[test]
    fn malformed_color_is_rejected() {
        assert!(parse_hsv("éééééé").is_none());
        assert!(parse_hsv("garbage").is_none());
        let mut l = Light::default();
        assert!(l.set_hex("zzzzzz").is_err());
        assert!(l.set_hex("#FF0000").is_ok());
        assert_eq!(l.hue, 0);
        assert_eq!(l.hex(), "#FF0000");
        assert!(l.set_hex("#000000").is_ok());
        assert_eq!(l.value, 0);
        assert_eq!(l.hsv(), "000000000000");
    }
    #[test]
    fn brightness_ranges_are_bounded() {
        assert_eq!(raw_brightness(100), 1000);
        assert_eq!(raw_brightness(0), 10);
        let mut l = Light::default();
        l.merge(json!({"22":0,"23":99999}).as_object().unwrap());
        assert_eq!(l.brightness, 1);
        assert_eq!(l.temperature, 100);
    }
}
