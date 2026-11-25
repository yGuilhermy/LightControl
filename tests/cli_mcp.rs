use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn cli_help_and_mcp_stdio_contract() {
    let executable = env!("CARGO_BIN_EXE_lightcontrol");
    let help = Command::new(executable).arg("-h").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("config discover"));

    let root = std::env::temp_dir().join(format!("lightcontrol-mcp-test-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let mut child = Command::new(executable)
        .arg("--data-dir")
        .arg(&root)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let requests = [
        json!({"id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"method":"notifications/initialized"}),
        json!({"id":2,"method":"tools/list"}),
        json!({"id":3,"method":"tools/call","params":{"name":"light_brightness","arguments":{"percent":101}}}),
        json!({"id":4,"method":"tools/call","params":{"name":"config_set","arguments":{"name":"Luz de teste"}}}),
        json!({"id":5,"method":"tools/call","params":{"name":"light_status"}}),
    ];
    for mut request in requests {
        request["jsonrpc"] = json!("2.0");
        writeln!(input, "{request}").unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(output.status.success());
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 5);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    assert!(responses[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "light_power"));
    assert_eq!(responses[2]["error"]["code"], -32602);
    let config: Value = serde_json::from_str(
        responses[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        config,
        json!({"name":"Luz de teste","configured":false,"ip":null})
    );
    assert_eq!(responses[4]["result"]["isError"], true);
}
