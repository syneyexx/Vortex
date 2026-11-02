//! SQLite-tool: lezen (read-only verbinding, alleen leesstatements) en schrijven (transactie).

use std::sync::Arc;

use rusqlite::{OpenFlags, types::ValueRef};
use serde_json::json;

use super::{s, u};
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::tool::{Tool, ToolCtx, ToolOutput};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(SqliteQuery::new()), Arc::new(SqliteExec::new())]
}

fn params(i: &serde_json::Value) -> Vec<rusqlite::types::Value> {
    i.get("params")
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .map(|v| match v {
                    serde_json::Value::Null => rusqlite::types::Value::Null,
                    serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(i64::from(*b)),
                    serde_json::Value::Number(n) => match n.as_i64() {
                        Some(x) => rusqlite::types::Value::Integer(x),
                        None => rusqlite::types::Value::Real(n.as_f64().unwrap_or(0.0)),
                    },
                    serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                    other => rusqlite::types::Value::Text(other.to_string()),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn sql_err(e: rusqlite::Error) -> ToolError {
    ToolError::failed(e.to_string()).with_hint("controleer tabel-/kolomnamen (bv. SELECT name, sql FROM sqlite_master)")
}

fn schema() -> serde_json::Value {
    json!({"type": "object", "required": ["db", "sql"], "properties": {
        "db": {"type": "string", "minLength": 1, "description": "pad naar het .db/.sqlite-bestand"},
        "sql": {"type": "string", "minLength": 1},
        "params": {"type": "array"},
        "max_rows": {"type": "integer", "minimum": 1, "maximum": 5000, "default": 200}
    }})
}

#[derive(Debug)]
pub struct SqliteQuery {
    c: ToolContract,
}
impl SqliteQuery {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "sqlite_query",
                "Lees uit een SQLite-database (alleen SELECT/PRAGMA; read-only).",
                Effect::Read,
                schema(),
            )
            .resources(vec![ResourceUse::Disk])
            .timeout(30_000),
        }
    }
}
impl Default for SqliteQuery {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for SqliteQuery {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = ctx.caps.resolve(s(&i, "db"))?;
        if !path.exists() {
            return Err(ToolError::new(ToolErrorKind::Precondition, format!("{} bestaat niet", path.display())));
        }
        let sql = s(&i, "sql").to_string();
        let max = u(&i, "max_rows").unwrap_or(200) as usize;
        let ps = params(&i);
        tokio::task::spawn_blocking(move || {
            let c = rusqlite::Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(sql_err)?;
            let mut st = c.prepare(&sql).map_err(sql_err)?;
            if !st.readonly() {
                return Err(ToolError::invalid("dit statement wijzigt de database")
                    .with_hint("gebruik sqlite_exec om te schrijven"));
            }
            let cols: Vec<String> = st.column_names().iter().map(|c| c.to_string()).collect();
            let mut rows = st.query(rusqlite::params_from_iter(ps.iter())).map_err(sql_err)?;
            let mut out = Vec::new();
            let mut more = false;
            while let Some(r) = rows.next().map_err(sql_err)? {
                if out.len() >= max {
                    more = true;
                    break;
                }
                let mut o = serde_json::Map::new();
                for (k, col) in cols.iter().enumerate() {
                    let v = match r.get_ref(k).map_err(sql_err)? {
                        ValueRef::Null => serde_json::Value::Null,
                        ValueRef::Integer(x) => json!(x),
                        ValueRef::Real(x) => json!(x),
                        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                        ValueRef::Blob(b) => json!(format!("<blob {} bytes>", b.len())),
                    };
                    o.insert(col.clone(), v);
                }
                out.push(serde_json::Value::Object(o));
            }
            let mut text = cols.join(" | ");
            for r in &out {
                text.push('\n');
                text.push_str(
                    &cols
                        .iter()
                        .map(|c| match &r[c] {
                            serde_json::Value::String(s) => s.clone(),
                            v => v.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(" | "),
                );
            }
            if more {
                text.push_str(&format!("\n… meer dan {max} rijen (gebruik LIMIT/WHERE)"));
            }
            Ok(ToolOutput { data: json!({"columns": cols, "rows": out, "truncated": more}), text, preview: None })
        })
        .await
        .map_err(|e| ToolError::failed(e.to_string()))?
    }
}

#[derive(Debug)]
pub struct SqliteExec {
    c: ToolContract,
}
impl SqliteExec {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "sqlite_exec",
                "Schrijf naar een SQLite-database (INSERT/UPDATE/DELETE/CREATE) in één transactie.",
                Effect::Local,
                schema(),
            )
            .resources(vec![ResourceUse::Disk])
            .timeout(60_000),
        }
    }
}
impl Default for SqliteExec {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for SqliteExec {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = ctx.caps.resolve(s(&i, "db"))?;
        let sql = s(&i, "sql").to_string();
        let ps = params(&i);
        tokio::task::spawn_blocking(move || {
            let mut c = rusqlite::Connection::open(&path).map_err(sql_err)?;
            let tx = c.transaction().map_err(sql_err)?;
            let n = if ps.is_empty() {
                tx.execute_batch(&sql).map_err(sql_err)?;
                tx.changes() as usize
            } else {
                tx.execute(&sql, rusqlite::params_from_iter(ps.iter())).map_err(sql_err)?
            };
            tx.commit().map_err(sql_err)?;
            Ok(ToolOutput::json(json!({"changes": n, "db": path.display().to_string()}))
                .with_text(format!("OK — {n} rij(en) gewijzigd")))
        })
        .await
        .map_err(|e| ToolError::failed(e.to_string()))?
    }
}
