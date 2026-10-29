#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crypto;
mod model;
mod protocol;
mod store;
mod ui;
mod validation;
mod worker;

fn main() {
    if let Err(error) = run() {
        eprintln!("LightControl: {error}");
        if !std::env::args().any(|s| s == "--diagnose" || s == "--test-device" || s == "--ui-test")
        {
            ui::error_box(&error);
        }
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|s| s == "--quit") {
        ui::request_quit();
        return Ok(());
    }
    if args.iter().any(|s| s == "--hide") {
        ui::request_hide();
        return Ok(());
    }
    if args.iter().any(|s| s == "--stats") {
        ui::request_stats();
        return Ok(());
    }
    let value = |flag: &str| {
        args.iter()
            .position(|s| s == flag)
            .and_then(|n| args.get(n + 1))
            .cloned()
    };
    let root = value("--data-dir")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(store::default_dir);
    let mut store = store::Store::open(root)?;
    if let Some(path) = value("--import-manager").or_else(|| value("--import-hikari")) {
        store.import_tuya_manager(std::path::Path::new(&path))?;
    }
    if args.iter().any(|s| s == "--diagnose") {
        let creds = store
            .credentials()?
            .ok_or("Importe as credenciais primeiro.")?;
        let mut client = protocol::Client::connect(&creds.ip, &creds.key)?;
        let status = client.query()?;
        let report = serde_json::json!({"protocol":"3.5", "connected":true, "dps":status});
        std::fs::write(
            store.root.join("diagnostics.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    if args.iter().any(|s| s == "--test-device") {
        return validation::device_test(&store);
    }
    let preview = args.iter().any(|s| s == "--preview");
    let ui_test = args.iter().any(|s| s == "--ui-test");
    if ui_test && (!preview || value("--data-dir").is_none()) {
        return Err("O teste da interface exige --preview e uma pasta de dados isolada.".into());
    }
    let tray = args.iter().any(|s| s == "--tray") || store.config.start_hidden;
    ui::run(
        store,
        preview,
        tray,
        value("--snapshot").map(std::path::PathBuf::from),
        args.iter().any(|s| s == "--mini"),
        value("--page"),
        ui_test,
    )
}
