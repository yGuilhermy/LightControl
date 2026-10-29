use crate::{model::Dps, protocol::Client, store::Store};
use serde_json::json;
use std::time::Instant;

pub fn device_test(store: &Store) -> Result<(), String> {
    let credentials = store
        .credentials()?
        .ok_or("Importe as credenciais primeiro.")?;
    let mut client = Client::connect(&credentials.ip, &credentials.key)?;
    let original = client.query()?;
    let mut restore = Dps::new();
    for k in ["20", "21", "22", "23", "24", "25", "34"] {
        if let Some(v) = original.get(k) {
            restore.insert(k.into(), v.clone());
        }
    }
    let power = original
        .get("20")
        .and_then(|v| v.as_bool())
        .ok_or("A lâmpada não reportou energia.")?;
    let mut results = Vec::new();
    let test = (|| -> Result<(), String> {
        let cases = [
            ("brilho branco", json!({"20":power,"21":"white","22":505})),
            ("temperatura", json!({"20":power,"21":"white","23":500})),
            (
                "cor e brilho HSV",
                json!({"20":power,"21":"colour","24":"00df01f4012c"}),
            ),
            ("energia", json!({"20":!power})),
            ("restaurar energia", json!({"20":power})),
        ];
        for (name, value) in cases {
            let began = Instant::now();
            client.set(value.as_object().unwrap())?;
            results
                .push(json!({"test":name,"passed":true,"latency_ms":began.elapsed().as_millis()}));
        }
        if let Some(scene) = original.get("25") {
            let began = Instant::now();
            client.set(
                json!({"20":power,"21":"scene","25":scene})
                    .as_object()
                    .unwrap(),
            )?;
            results.push(json!({"test":"cena nativa","passed":true,"latency_ms":began.elapsed().as_millis()}));
            if let Some(scene) = store.config.scenes.first() {
                let payload = scene.payload()?;
                let began = Instant::now();
                client.set(
                    json!({"20":power,"21":"scene","25":payload})
                        .as_object()
                        .unwrap(),
                )?;
                results.push(json!({"test":"cena criada pelo editor","passed":true,"latency_ms":began.elapsed().as_millis()}));
            }
        }
        if let Some(dnd) = original.get("34").and_then(|v| v.as_bool()) {
            let began = Instant::now();
            client.set(json!({"34":!dnd}).as_object().unwrap())?;
            client.set(json!({"34":dnd}).as_object().unwrap())?;
            results.push(json!({"test":"não perturbe após falta de energia","passed":true,"latency_ms":began.elapsed().as_millis()}));
        }
        if original.get("26").and_then(|v| v.as_u64()) == Some(0) {
            let began = Instant::now();
            client.set(json!({"26":60}).as_object().unwrap())?;
            client.set(json!({"26":0}).as_object().unwrap())?;
            results.push(json!({"test":"timer nativo e cancelamento","passed":true,"latency_ms":began.elapsed().as_millis()}));
        }
        Ok(())
    })();
    let restored = client.set(&restore).or_else(|_| {
        Client::connect(&credentials.ip, &credentials.key).and_then(|mut c| c.set(&restore))
    });
    if original.get("26").and_then(|v| v.as_u64()) == Some(0) {
        let _ = client.set(json!({"26":0}).as_object().unwrap());
    }
    let report = json!({"tests":results,"restored":restored.is_ok(),"passed":test.is_ok()&&restored.is_ok(),"error":test.as_ref().err()});
    std::fs::write(
        store.root.join("hardware-tests.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    restored.map_err(|_| {
        "Não foi possível restaurar o estado original da lâmpada. Reconecte e confira o controle."
    })?;
    test
}
