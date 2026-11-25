use lightcontrol::{
    control::{self, Action},
    store::Store,
};
use serde_json::{json, Map, Value};
use std::{
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
};

const VERSION: &str = "2025-11-25";
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_LINE_BYTES: u64 = 64 * 1024;

pub fn serve(root: PathBuf) -> Result<(), String> {
    let tools = tools();
    let mut input = io::stdin().lock();
    let mut out = io::stdout().lock();
    let mut initialized = false;
    loop {
        let mut line = Vec::new();
        let size = (&mut input)
            .take(MAX_LINE_BYTES)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if size == 0 {
            return Ok(());
        }
        if !line.ends_with(b"\n") && size as u64 == MAX_LINE_BYTES {
            return Err("Mensagem MCP excede 64 KiB.".into());
        }
        let response = match serde_json::from_slice::<Value>(&line) {
            Ok(value) => dispatch(&root, &tools, &mut initialized, value),
            Err(_) => Some(error(Value::Null, -32700, "JSON inválido")),
        };
        if let Some(response) = response {
            writeln!(out, "{response}")
                .and_then(|_| out.flush())
                .map_err(|e| e.to_string())?;
        }
    }
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn dispatch(root: &Path, tools: &[Value], initialized: &mut bool, request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let valid_id = id
        .as_ref()
        .is_none_or(|id| id.is_string() || id.is_number() || id.is_null());
    let (Some(method), true) = (
        request["method"].as_str(),
        request["jsonrpc"] == "2.0" && valid_id,
    ) else {
        return Some(error(Value::Null, -32600, "Requisição inválida"));
    };
    let id = id?;
    let params = &request["params"];
    let result = match method {
        "initialize" => {
            let Some(version) = params["protocolVersion"].as_str() else {
                return Some(error(id, -32602, "Informe protocolVersion"));
            };
            *initialized = true;
            json!({"protocolVersion":if VERSIONS.contains(&version) { version } else { VERSION },
                "capabilities":{"tools":{}},
                "serverInfo":{"name":"lightcontrol","version":env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        _ if !*initialized => return Some(error(id, -32002, "Inicialize o servidor primeiro")),
        "tools/list" => json!({"tools": tools}),
        "tools/call" => {
            let Some(tool) = tools.iter().find(|tool| tool["name"] == params["name"]) else {
                return Some(error(id, -32602, "Ferramenta desconhecida"));
            };
            let empty = Map::new();
            let args = match params.get("arguments") {
                None => &empty,
                Some(Value::Object(args)) => args,
                _ => return Some(error(id, -32602, "arguments deve ser um objeto")),
            };
            if !valid_arguments(&tool["inputSchema"], args) {
                return Some(error(id, -32602, "Argumentos inválidos para a ferramenta"));
            }
            match call(root, tool["name"].as_str().unwrap(), args) {
                Ok(result) => {
                    json!({"content":[{"type":"text","text":result.to_string()}],"isError":false})
                }
                Err(message) => json!({"content":[{"type":"text","text":message}],"isError":true}),
            }
        }
        _ => return Some(error(id, -32601, "Método desconhecido")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn tools() -> Vec<Value> {
    let range = |min, max| json!({"type":"integer","minimum":min,"maximum":max});
    let tool = |name, description, properties: Value, required: &[&str], read_only| {
        json!({"name":name,"description":description,"inputSchema":{
            "type":"object","properties":properties,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint":read_only,"openWorldHint":false}})
    };
    vec![
        tool(
            "light_status",
            "Consulta o estado confirmado da lâmpada.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "light_power",
            "Liga ou desliga a lâmpada.",
            json!({"on":{"type":"boolean"}}),
            &["on"],
            false,
        ),
        tool(
            "light_brightness",
            "Ajusta o brilho no modo atual.",
            json!({"percent":range(1,100)}),
            &["percent"],
            false,
        ),
        tool(
            "light_temperature",
            "Seleciona branco, 0 quente a 100 frio.",
            json!({"percent":range(0,100)}),
            &["percent"],
            false,
        ),
        tool(
            "light_color",
            "Seleciona uma cor hexadecimal, RRGGBB.",
            json!({"hex":{"type":"string"}}),
            &["hex"],
            false,
        ),
        tool(
            "light_timer",
            "Configura o timer nativo em minutos. Zero cancela.",
            json!({"minutes":range(0,1440)}),
            &["minutes"],
            false,
        ),
        tool(
            "light_dnd",
            "Ativa ou desativa Não perturbe após queda de energia.",
            json!({"enabled":{"type":"boolean"}}),
            &["enabled"],
            false,
        ),
        tool(
            "preset_list",
            "Lista os presets do perfil local.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "preset_apply",
            "Aplica um preset pelo nome.",
            json!({"name":{"type":"string"}}),
            &["name"],
            false,
        ),
        tool(
            "scene_list",
            "Lista as cenas do perfil local.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "scene_apply",
            "Aplica uma cena pelo nome.",
            json!({"name":{"type":"string"}}),
            &["name"],
            false,
        ),
        tool(
            "config_show",
            "Consulta a configuração pública do perfil local.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "config_set",
            "Muda o nome ou valida e atualiza o IPv4 da lâmpada usando a credencial já cadastrada.",
            json!({"name":{"type":"string"},"ip":{"type":"string"}}),
            &[],
            false,
        ),
        tool(
            "config_discover",
            "Procura a lâmpada cadastrada na LAN, autentica e salva o IPv4.",
            json!({}),
            &[],
            false,
        ),
    ]
}

fn valid_arguments(schema: &Value, args: &Map<String, Value>) -> bool {
    schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .all(|key| args.contains_key(key.as_str().unwrap()))
        && args.iter().all(|(name, value)| {
            let property = &schema["properties"][name];
            match property["type"].as_str() {
                Some("boolean") => value.is_boolean(),
                Some("string") => value.as_str().is_some_and(|s| s.len() <= 256),
                Some("integer") => value.as_u64().is_some_and(|n| {
                    (property["minimum"].as_u64().unwrap()..=property["maximum"].as_u64().unwrap())
                        .contains(&n)
                }),
                _ => false,
            }
        })
}

fn call(root: &Path, name: &str, args: &Map<String, Value>) -> Result<Value, String> {
    let mut store = Store::open(root.into())?;
    let string = |key: &str| args[key].as_str().unwrap();
    let number = |key: &str| args[key].as_u64().unwrap() as u16;
    let action = match name {
        "light_status" => Action::Status,
        "light_power" => Action::Power(args["on"].as_bool().unwrap()),
        "light_brightness" => Action::Brightness(number("percent")),
        "light_temperature" => Action::Temperature(number("percent")),
        "light_color" => Action::Color(string("hex")),
        "light_timer" => Action::Timer(number("minutes")),
        "light_dnd" => Action::DoNotDisturb(args["enabled"].as_bool().unwrap()),
        "preset_list" => return Ok(json!(store.config.presets)),
        "preset_apply" => Action::Preset(string("name")),
        "scene_list" => return Ok(json!(store.config.scenes)),
        "scene_apply" => Action::Scene(string("name")),
        "config_show" => return control::configuration(&store),
        "config_discover" => return control::discover(&mut store),
        "config_set" => {
            if args.is_empty() {
                return Err("Informe name ou ip.".into());
            }
            let ip = args
                .get("ip")
                .map(|v| v.as_str().unwrap().parse())
                .transpose()
                .map_err(|_| "IPv4 inválido.")?;
            return control::configure(
                &mut store,
                ip,
                args.get("name").and_then(Value::as_str),
                None,
            );
        }
        _ => return Err("Ferramenta desconhecida.".into()),
    };
    control::execute(&store, action)
}
