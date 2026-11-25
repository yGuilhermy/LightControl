use lightcontrol::{
    control::{self, Action},
    store::{self, Credentials, Store},
};
use serde_json::{json, Value};
use std::{
    io::{self, IsTerminal, Read, Write},
    net::Ipv4Addr,
    path::PathBuf,
};

#[path = "mcp/mod.rs"]
mod mcp;

const HELP: &str = "LightControl CLI

Uso: lightcontrol [--data-dir DIRETÓRIO] [--json] <comando>

  status                          Consulta a lâmpada
  on | off                        Liga ou desliga
  brightness <1-100>              Ajusta o brilho no modo atual
  temperature <0-100>             Seleciona branco (0 quente, 100 frio)
  color <RRGGBB>                  Seleciona uma cor
  timer <0-1440>                  Timer da lâmpada em minutos (0 cancela)
  dnd <on|off>                    Não perturbe após queda de energia
  preset list | apply <nome>      Lista ou aplica presets
  scene list | apply <nome>       Lista ou aplica cenas
  config show                     Mostra configuração sem expor a chave
  config set [--ip IPv4] [--name nome]
  config set --ip IPv4 --id ID --key-stdin [--name nome]
  config import-manager <arquivo> Importa um módulo Tuya existente
  config discover                 Descobre e valida o novo IP na LAN
  mcp                             Inicia servidor MCP local via stdio

  -h, --help                      Mostra esta ajuda
  -V, --version                   Mostra a versão";

const CONFIG_HELP: &str = "Uso: lightcontrol config show
     lightcontrol config set [--ip IPv4] [--name nome]
     lightcontrol config set --ip IPv4 --id ID --key-stdin [--name nome]
     lightcontrol config import-manager <arquivo>
     lightcontrol config discover";

fn main() {
    if let Err(error) = run() {
        eprintln!("LightControl: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut root = store::default_dir();
    let mut json_output = false;
    let command = loop {
        match args.next().as_deref() {
            Some("--data-dir") => root = PathBuf::from(args.next().ok_or("Informe o diretório.")?),
            Some("--json") => json_output = true,
            Some("-V" | "--version") => {
                println!("LightControl {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            Some("-h" | "--help" | "help") | None => {
                println!("{HELP}");
                return Ok(());
            }
            Some(command) => break command.to_owned(),
        }
    };
    let remaining: Vec<_> = args.collect();
    let args: Vec<_> = remaining.iter().map(String::as_str).collect();
    if matches!(args.last(), Some(&"-h" | &"--help")) {
        println!(
            "{}",
            if command == "config" {
                CONFIG_HELP
            } else {
                HELP
            }
        );
        return Ok(());
    }
    if command == "mcp" && args.is_empty() {
        return mcp::serve(root);
    }
    let mut store = Store::open(root)?;
    let result = match (command.as_str(), args.as_slice()) {
        ("status", []) => control::execute(&store, Action::Status)?,
        ("on" | "off", []) => control::execute(&store, Action::Power(command == "on"))?,
        ("brightness", [n]) => control::execute(&store, Action::Brightness(number(n, 1, 100)?))?,
        ("temperature", [n]) => control::execute(&store, Action::Temperature(number(n, 0, 100)?))?,
        ("color", [hex]) => control::execute(&store, Action::Color(hex))?,
        ("timer", [n]) => control::execute(&store, Action::Timer(number(n, 0, 1440)?))?,
        ("dnd", [state @ ("on" | "off")]) => {
            control::execute(&store, Action::DoNotDisturb(*state == "on"))?
        }
        ("preset", ["list"]) => json!(store.config.presets),
        ("scene", ["list"]) => json!(store.config.scenes),
        ("preset", ["apply", name]) => control::execute(&store, Action::Preset(name))?,
        ("scene", ["apply", name]) => control::execute(&store, Action::Scene(name))?,
        ("config", ["show"]) => control::configuration(&store)?,
        ("config", ["set", options @ ..]) => configure(&mut store, options)?,
        ("config", ["discover"]) => control::discover(&mut store)?,
        ("config", ["import-manager", path]) => {
            store.import_tuya_manager(std::path::Path::new(path))?;
            control::configuration(&store)?
        }
        _ => return Err("Comando ou argumentos inválidos. Use lightcontrol -h.".into()),
    };
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json_output {
        writeln!(out, "{result}")
    } else if let Some(power) = result["power"].as_bool() {
        writeln!(
            out,
            "{}: {} · {}% · {}",
            result["name"].as_str().unwrap_or("Luz"),
            if power { "ligada" } else { "desligada" },
            result["brightness"],
            result["mode"].as_str().unwrap_or_default()
        )
    } else {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
        )
    }
    .map_err(|e| e.to_string())
}

fn number(value: &str, min: u16, max: u16) -> Result<u16, String> {
    value
        .parse()
        .ok()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| format!("Informe um número entre {min} e {max}."))
}

fn configure(store: &mut Store, mut args: &[&str]) -> Result<Value, String> {
    let (mut ip, mut name, mut id, mut read_key) = (None, None, None, false);
    while let [flag, rest @ ..] = args {
        args = rest;
        match (*flag, args) {
            ("--ip", [value, tail @ ..]) if ip.is_none() => {
                ip = Some(value.parse::<Ipv4Addr>().map_err(|_| "IPv4 inválido.")?);
                args = tail;
            }
            ("--name", [value, tail @ ..]) if name.is_none() => {
                name = Some(*value);
                args = tail;
            }
            ("--id", [value, tail @ ..]) if id.is_none() => {
                id = Some(*value);
                args = tail;
            }
            ("--key-stdin", _) if !read_key => read_key = true,
            _ => return Err("Opções inválidas. Use lightcontrol config -h.".into()),
        }
    }
    let new = match (read_key, id, ip) {
        (true, Some(id), Some(ip)) if !id.trim().is_empty() => {
            if io::stdin().is_terminal() {
                return Err("Forneça a chave por um pipe. Veja o exemplo seguro no README.".into());
            }
            let mut key = String::new();
            io::stdin()
                .take(64)
                .read_to_string(&mut key)
                .map_err(|_| "Não foi possível ler a chave de stdin.")?;
            key.truncate(key.trim_end_matches(['\r', '\n']).len());
            let credential = Credentials {
                id: id.into(),
                ip: ip.to_string(),
                key,
            };
            if credential.key.len() != 16 {
                return Err("A chave deve ter 16 bytes.".into());
            }
            Some(credential)
        }
        (false, None, _) if ip.is_some() || name.is_some() => None,
        _ => return Err("Para trocar a chave, informe --ip, --id e --key-stdin.".into()),
    };
    control::configure(store, ip, name, new)
}
