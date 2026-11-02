//! Programmatic tool calling (W17.13): het model schrijft één Python-programma dat tools
//! aanroept (`vx.call("read_file", path="a.txt")`), met lussen, filters en tussenresultaten in
//! code i.p.v. een modelronde per aanroep. Aanroepen lopen via een lokale socket naar de
//! registry (schema, capabilities); alleen tools zonder externe bijwerking (Read/Local).
// VX:W17.13

use std::sync::{Arc, Weak};

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use super::{s, u};
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolCtx, ToolOutput};

const PRELUDE: &str = r#"
import json as _json, socket as _socket
class _VX:
    def __init__(self, port):
        self._s = _socket.create_connection(("127.0.0.1", port))
        self._f = self._s.makefile("rw", encoding="utf-8")
        self.calls = 0
    def call(self, tool, **args):
        self.calls += 1
        self._f.write(_json.dumps({"tool": tool, "args": args}) + "\n"); self._f.flush()
        r = _json.loads(self._f.readline())
        if not r.get("ok"):
            raise RuntimeError(f"{tool}: {r.get('error')}")
        return r.get("data") if r.get("data") not in (None, {}) else r.get("text")
vx = _VX(__PORT__)
"#;

#[derive(Debug)]
pub struct ToolProgram {
    c: ToolContract,
    reg: Weak<ToolRegistry>,
}

impl ToolProgram {
    pub fn new(reg: Weak<ToolRegistry>) -> Self {
        Self {
            c: ToolContract::new(
                "tool_program",
                "Schrijf Python dat tools aanroept met `vx.call(\"tool\", arg=…)` (resultaat = data of tekst) — voor veel/herhaalde aanroepen, filteren en combineren in één keer. Print het eindresultaat. Alleen tools zonder externe bijwerking.",
                Effect::Local,
                json!({"type": "object", "required": ["code"], "properties": {
                    "code": {"type": "string", "minLength": 1},
                    "timeout_s": {"type": "integer", "minimum": 1, "maximum": 3600},
                    "max_calls": {"type": "integer", "minimum": 1, "maximum": 1000}
                }}),
            )
            .resources(vec![ResourceUse::Cpu])
            .timeout(3_700_000),
            reg,
        }
    }
}

#[async_trait::async_trait]
impl Tool for ToolProgram {
    fn contract(&self) -> &ToolContract {
        &self.c
    }

    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        if !ctx.caps.processes {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag geen processen starten"));
        }
        let reg = self.reg.upgrade().ok_or_else(|| ToolError::failed("registry weg"))?;
        let max_calls = u(&i, "max_calls").unwrap_or(200) as usize;
        let l = TcpListener::bind("127.0.0.1:0").await?;
        let port = l.local_addr()?.port();
        let log = Arc::new(parking_lot::Mutex::new(Vec::<serde_json::Value>::new()));
        let (reg2, ctx2, log2) = (reg.clone(), ctx.clone(), log.clone());
        let server = tokio::spawn(async move {
            let Ok((sock, _)) = l.accept().await else { return };
            let (r, mut w) = sock.into_split();
            let mut lines = BufReader::new(r).lines();
            let mut n = 0usize;
            while let Ok(Some(line)) = lines.next_line().await {
                n += 1;
                let req: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
                let name = req["tool"].as_str().unwrap_or("").to_string();
                let args = req["args"].clone();
                let resp = if n > max_calls {
                    json!({"ok": false, "error": format!("maximum van {max_calls} aanroepen bereikt")})
                } else {
                    match nested(&reg2, &ctx2, &name, args.clone()).await {
                        Ok(o) => json!({"ok": true, "data": o.data, "text": o.text}),
                        Err(e) => json!({"ok": false, "error": e.message}),
                    }
                };
                log2.lock().push(json!({"tool": name, "ok": resp["ok"]}));
                if w.write_all(format!("{resp}\n").as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        let code = format!("{}\n{}", PRELUDE.replace("__PORT__", &port.to_string()), s(&i, "code"));
        let mut lim = ctx.sandbox.limits.clone();
        if let Some(t) = u(&i, "timeout_s") {
            lim.timeout_ms = t.clamp(1, 3600) * 1000;
        }
        let r = ctx.sandbox.python(&code, Some(lim), ctx.run, ctx.trace, ctx.cancel.clone()).await;
        server.abort();
        let r = r?;
        let calls = log.lock().clone();
        let text = format!("{}\n({} toolaanroep(en) vanuit het programma)", r.text(), calls.len());
        if r.timed_out {
            return Err(ToolError::new(ToolErrorKind::Timeout, text));
        }
        if r.exit_code != Some(0) {
            return Err(
                ToolError::failed(text).with_hint("lees de traceback (ook fouten van vx.call), pas de code aan")
            );
        }
        Ok(ToolOutput { data: json!({"report": r, "calls": calls}), text, preview: None })
    }
}

/// Eén geneste aanroep: bestaat, mag (capability + geen externe bijwerking), schema klopt.
async fn nested(
    reg: &Arc<ToolRegistry>,
    ctx: &ToolCtx,
    name: &str,
    args: serde_json::Value,
) -> Result<ToolOutput, ToolError> {
    if name == "tool_program" {
        return Err(ToolError::invalid("geen geneste tool_program"));
    }
    let t =
        reg.get(name).ok_or_else(|| ToolError::new(ToolErrorKind::NotFound, format!("tool '{name}' bestaat niet")))?;
    let c = t.contract().clone();
    if !ctx.caps.allows_tool(&c.name) {
        return Err(ToolError::new(ToolErrorKind::Capability, format!("tool {name} niet toegestaan")));
    }
    if !matches!(c.effect, Effect::Read | Effect::Local) {
        return Err(ToolError::new(
            ToolErrorKind::Capability,
            format!("{name} heeft externe bijwerkingen: roep die rechtstreeks aan (idempotentie-ledger)"),
        ));
    }
    ctx.caps.check_effect(c.effect, !c.net_destinations.is_empty())?;
    let input = crate::schema::coerce(&c.input_schema, args);
    crate::schema::validate(&c.input_schema, &input).map_err(|e| ToolError::invalid(e.join("; ")))?;
    t.call(ctx, input).await
}
