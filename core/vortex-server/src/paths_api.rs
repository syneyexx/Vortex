//! Mappen en programma's instellen vanuit de GUI (config/paths.toml). Ontbrekende mappen zijn
//! nooit een reden om te crashen: de kern valt terug (data → ./data, koud → <data>/cold) en de
//! GUI laat zien wat er niet bestaat, zodat je het hier later aanpast.

use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_types::{Event, EventKind, TraceId};

use crate::api::{ApiError, AppState};

/// (sleutel, soort, uitleg, herstart nodig)
pub const KEYS: &[(&str, &str, &str, bool)] = &[
    ("data_dir", "dir", "Database, index en logs (snelle SSD). Relatief = t.o.v. de repo.", true),
    ("models_dir", "dir", "Modelbestanden (GGUF) — SSD.", false),
    ("cold_dir", "dir", "Ruwe data, archief en backups (HDD als je die hebt; leeg/ontbrekend → data\\cold).", false),
    ("hf_home", "dir", "Hugging Face-cache.", false),
    ("llama_server", "file", "Pad naar llama-server(.exe).", false),
    ("browser", "file", "Browser voor JS-pagina's en automatisering (leeg = automatisch zoeken).", false),
    ("worker_address", "addr", "Adres van de Python-worker.", true),
];

fn exists(kind: &str, v: &str) -> Option<bool> {
    match kind {
        "dir" | "file" if v.is_empty() => None,
        "dir" => Some(Path::new(v).is_dir()),
        "file" => Some(Path::new(v).is_file()),
        _ => None,
    }
}

async fn get_paths(State(s): State<AppState>) -> Json<Value> {
    let cfg = s.kernel.config().current();
    let items: Vec<Value> = KEYS
        .iter()
        .map(|(k, kind, help, restart)| {
            let v = cfg.get::<String>(&format!("paths.{k}")).ok().flatten().unwrap_or_default();
            json!({"key": k, "value": v, "kind": kind, "help": help, "restart": restart, "exists": exists(kind, &v)})
        })
        .collect();
    let data = s.chat.data_dir();
    let cold = crate::research_api::cold_dir(&cfg, &data);
    Json(
        json!({"items": items, "effective": {"data_dir": data.display().to_string(), "cold_dir": cold.display().to_string()}}),
    )
}

#[derive(Debug, Deserialize)]
struct PutPaths {
    values: serde_json::Map<String, Value>,
    /// Ontbrekende mappen aanmaken (als de schijf bestaat).
    #[serde(default)]
    create: bool,
}

/// `key = "waarde"` vervangen met behoud van commentaar, of toevoegen.
pub fn set_key(text: &str, key: &str, value: &str) -> String {
    let val = toml::Value::String(value.to_string()).to_string();
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            let t = l.trim_start();
            if !found && (t.starts_with(&format!("{key} ")) || t.starts_with(&format!("{key}="))) {
                found = true;
                let comment = l.find(" #").map(|i| format!("   {}", l[i..].trim_start())).unwrap_or_default();
                format!("{key} = {val}{comment}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        out.push(format!("{key} = {val}"));
    }
    out.join("\n") + "\n"
}

async fn put_paths(State(s): State<AppState>, Json(b): Json<PutPaths>) -> Result<Json<Value>, ApiError> {
    let file = s.kernel.config().dir().join("paths.toml");
    let mut text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut created = Vec::new();
    for (k, v) in &b.values {
        let Some((_, kind, _, _)) = KEYS.iter().find(|x| x.0 == k) else {
            return Err(ApiError::new(StatusCode::BAD_REQUEST, format!("onbekende instelling {k}")));
        };
        let v = v.as_str().unwrap_or("").trim().replace('\\', "/");
        if b.create && *kind == "dir" && !v.is_empty() && std::fs::create_dir_all(&v).is_ok() {
            created.push(v.clone());
        }
        text = set_key(&text, k, &v);
    }
    std::fs::write(&file, text).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    s.kernel.config().reload().map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    let v = s.kernel.config().current().version.clone();
    let _ = s.kernel.events().append(Event::new(
        EventKind::PromptUpdated,
        TraceId::new(),
        json!({"path": "paths.toml", "version": v}),
    ));
    let Json(now) = get_paths(State(s)).await;
    Ok(Json(json!({"version": v, "created": created, "paths": now})))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/settings/paths", get(get_paths).put(put_paths))
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_comments_and_adds_missing() {
        let t = "# kop\ncold_dir = \"E:/x\"             # koud\nmodels_dir = \"D:/m\"\n";
        let r = super::set_key(t, "cold_dir", "D:/koud");
        assert!(r.contains("cold_dir = \"D:/koud\"   # koud") && r.starts_with("# kop"));
        assert!(super::set_key(&r, "browser", "").contains("browser = \"\""));
    }
}
