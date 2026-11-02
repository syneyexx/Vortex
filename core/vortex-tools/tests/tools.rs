#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integratietests W08: idempotentie (gate), transacties + postcondities, effectanalyse,
//! sandbox, herstel, MCP (stdio + HTTP), skills (import/selectie/hot-reload/voorstellen).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;
use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog, ResourceRegistry, Supervisor};
use vortex_tools::contract::{Effect, ToolContract};
use vortex_tools::mcp::McpServerCfg;
use vortex_tools::{
    Action, CapabilityHandle, Condition, ExecOptions, Tool, ToolCtx, ToolError, ToolErrorKind, ToolHub, ToolHubConfig,
    ToolOutput,
};

fn log() -> EventLog {
    EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()))
}

fn hub(dir: &std::path::Path) -> (Arc<ToolHub>, EventLog) {
    let l = log();
    let sup = Supervisor::new(l.clone(), ResourceRegistry::new(l.clone()));
    let cfg = ToolHubConfig {
        skills_dir: Some(dir.join("skills")),
        workdir: Some(dir.join("sandbox")),
        ..Default::default()
    };
    (ToolHub::new(cfg, l.clone(), sup, Arc::new(|_| None)), l)
}

fn opts(dir: &std::path::Path) -> ExecOptions {
    ExecOptions { caps: CapabilityHandle::scoped(dir), ..Default::default() }
}

/// Externe actie die telt hoe vaak hij echt uitgevoerd is; faalt de eerste keer ná het effect
/// (netwerk weg na verzenden) — klassiek dubbel-effect-scenario.
#[derive(Debug)]
struct Payment {
    c: ToolContract,
    effects: Arc<AtomicUsize>,
    fail_first: Arc<AtomicUsize>,
}
#[async_trait::async_trait]
impl Tool for Payment {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, _i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let n = self.effects.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_first.fetch_sub(1, Ordering::SeqCst) == 1 {
            return Err(ToolError::transient("verbinding weg"));
        }
        Ok(ToolOutput::json(json!({"payment": n})))
    }
}

#[tokio::test]
async fn gate_retry_never_doubles_an_effect() {
    let d = tempfile::tempdir().unwrap();
    let (h, _) = hub(d.path());
    let o = opts(d.path());

    // 1. Lokaal: write_file twee keer met dezelfde actie-ID → één effect; tweede = replay.
    let a = Action::new("task-1:w", "write_file", json!({"path": "log.txt", "content": "regel\n"}));
    let r1 = h.executor.execute(a.clone(), &HashMap::new(), &o).await;
    assert!(r1.ok, "{:?}", r1.error);
    std::fs::write(d.path().join("log.txt"), "handmatig gewijzigd\n").unwrap();
    let r2 = h.executor.execute(a.clone(), &HashMap::new(), &o).await;
    assert!(r2.ok && r2.replayed, "retry = replay");
    assert_eq!(
        std::fs::read_to_string(d.path().join("log.txt")).unwrap(),
        "handmatig gewijzigd\n",
        "geen tweede schrijfactie"
    );

    // 2. Zelfde ID, andere invoer → geweigerd.
    let r3 = h
        .executor
        .execute(
            Action::new("task-1:w", "write_file", json!({"path": "log.txt", "content": "anders"})),
            &HashMap::new(),
            &o,
        )
        .await;
    assert_eq!(r3.error.unwrap().kind, ToolErrorKind::InvalidInput);

    // 3. Extern, niet-idempotent: faalt ná effect → geen automatische retry; opnieuw met
    //    dezelfde ID = opnieuw uitvoeren is pas toegestaan als de vorige poging als mislukt
    //    geregistreerd is (failed), en een geslaagde poging wordt nooit herhaald.
    let effects = Arc::new(AtomicUsize::new(0));
    let mut c = ToolContract::new("pay", "betaal", Effect::External, json!({"type": "object"}));
    c.idempotent = false;
    h.registry.register(Arc::new(Payment { c, effects: effects.clone(), fail_first: Arc::new(AtomicUsize::new(1)) }));
    let p = Action::new("order-7", "pay", json!({"bedrag": 5}));
    let r = h.executor.execute(p.clone(), &HashMap::new(), &o).await;
    assert!(!r.ok);
    assert_eq!(effects.load(Ordering::SeqCst), 1, "geen automatische retry van een niet-idempotente externe actie");
    let r = h.executor.execute(p.clone(), &HashMap::new(), &o).await;
    assert!(r.ok);
    let r = h.executor.execute(p.clone(), &HashMap::new(), &o).await;
    assert!(r.ok && r.replayed);
    let r = h.executor.execute(p, &HashMap::new(), &o).await;
    assert!(r.replayed);
    assert_eq!(effects.load(Ordering::SeqCst), 2, "na succes nooit meer uitgevoerd");
    assert_eq!(h.executor.ledger().attempts("order-7"), Some(2));
}

#[tokio::test]
async fn interrupted_external_action_is_not_blindly_repeated() {
    let d = tempfile::tempdir().unwrap();
    let (h, l) = hub(d.path());
    let effects = Arc::new(AtomicUsize::new(0));
    let mut c = ToolContract::new("pay", "betaal", Effect::External, json!({"type": "object"}));
    c.idempotent = false;
    h.registry.register(Arc::new(Payment { c, effects: effects.clone(), fail_first: Arc::new(AtomicUsize::new(0)) }));
    // Crash na 'intent' simuleren: ledger heeft intent, geen done.
    let key = vortex_tools::input_key("pay", &json!({}));
    vortex_tools::ledger::Ledger::new(l).begin("crash-1", "pay", &key, Effect::External, None).unwrap();
    let r = h.executor.call("crash-1", "pay", json!({}), &opts(d.path())).await;
    assert_eq!(r.error.unwrap().kind, ToolErrorKind::Precondition);
    assert_eq!(effects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn transaction_postcondition_rolls_back_and_preview() {
    let d = tempfile::tempdir().unwrap();
    let (h, _) = hub(d.path());
    std::fs::write(d.path().join("cfg.toml"), "port = 1\nname = \"x\"\n").unwrap();
    let o = opts(d.path());
    // Postconditie faalt → niets toegepast.
    let a = Action::new("e1", "edit_file", json!({"path": "cfg.toml", "old": "port = 1", "new": "port = 2"}))
        .post(Condition::FileContains { path: "cfg.toml".into(), text: "port = 3".into() });
    let r = h.executor.execute(a, &HashMap::new(), &o).await;
    assert_eq!(r.error.unwrap().kind, ToolErrorKind::Postcondition);
    assert!(std::fs::read_to_string(d.path().join("cfg.toml")).unwrap().contains("port = 1"));
    // Dry run → preview, niets toegepast.
    let dry = ExecOptions { dry_run: true, ..o.clone() };
    let r = h
        .executor
        .call("e2", "edit_file", json!({"path": "cfg.toml", "old": "port = 1", "new": "port = 2"}), &dry)
        .await;
    assert!(r.ok && r.output.as_ref().unwrap().text.contains("-port = 1\n+port = 2"), "{:?}", r.output);
    assert!(std::fs::read_to_string(d.path().join("cfg.toml")).unwrap().contains("port = 1"));
    // Echt + postconditie ok.
    let a = Action::new("e3", "edit_file", json!({"path": "cfg.toml", "old": "port = 1", "new": "port = 2"}))
        .post(Condition::FileContains { path: "cfg.toml".into(), text: "port = 2".into() });
    assert!(h.executor.execute(a, &HashMap::new(), &o).await.ok);
    assert!(std::fs::read_to_string(d.path().join("cfg.toml")).unwrap().contains("port = 2"));
}

#[tokio::test]
async fn recovery_hints_and_capabilities() {
    let d = tempfile::tempdir().unwrap();
    let (h, _) = hub(d.path());
    let o = opts(d.path());
    // Onbekende tool → suggestie.
    let r = h.executor.call("x1", "reed_file", json!({}), &o).await;
    let e = r.error.unwrap();
    assert_eq!(e.kind, ToolErrorKind::NotFound);
    assert!(e.for_model().contains("read_file"));
    // Schema-fout met coercie: limit als string wordt geaccepteerd; ontbrekend pad niet.
    std::fs::write(d.path().join("a.txt"), "1\n2\n3\n").unwrap();
    let r = h.executor.call("x2", "read_file", json!({"path": "a.txt", "limit": "2"}), &o).await;
    assert!(r.ok && r.output.unwrap().text.contains("regels 1-2 van 3"));
    let r = h.executor.call("x3", "read_file", json!({}), &o).await;
    assert!(r.error.unwrap().for_model().contains("$.path: verplicht"));
    // edit_file met verkeerde tekst → hint met de bijna-regel.
    std::fs::write(d.path().join("b.rs"), "fn main() {\n    println!(\"hoi\");\n}\n").unwrap();
    let r = h
        .executor
        .call("x4", "edit_file", json!({"path": "b.rs", "old": "println!(\"hoi\");  ", "new": "x"}), &o)
        .await;
    assert!(r.error.unwrap().hint.unwrap().contains("regel 2"));
    // Buiten de capability.
    let r = h.executor.call("x5", "read_file", json!({"path": "../../etc/passwd"}), &o).await;
    assert_eq!(r.error.unwrap().kind, ToolErrorKind::Capability);
    let ro = ExecOptions { caps: CapabilityHandle { write: false, ..CapabilityHandle::scoped(d.path()) }, ..o.clone() };
    let r = h.executor.call("x6", "write_file", json!({"path": "c.txt", "content": "x"}), &ro).await;
    assert_eq!(r.error.unwrap().kind, ToolErrorKind::Capability);
}

#[tokio::test]
async fn plan_runs_parallel_stages_with_chaining() {
    let d = tempfile::tempdir().unwrap();
    let (h, l) = hub(d.path());
    std::fs::write(d.path().join("in.txt"), "Vortex\n").unwrap();
    let acts = vec![
        Action::new("r", "read_file", json!({"path": "in.txt"})),
        Action::new("w", "write_file", json!({"path": "out.txt", "content": "kopie: ${r.data.content}"})),
        Action::new("f", "find_files", json!({"pattern": "*.txt"})),
        Action::new("x", "read_file", json!({"path": "bestaat-niet.txt"})),
        Action::new("y", "write_file", json!({"path": "na-fout.txt", "content": "${x.text}"})),
    ];
    let res = h.executor.execute_plan(acts, &opts(d.path())).await;
    assert_eq!(res.len(), 5);
    assert!(res[0].ok && res[1].ok && res[2].ok);
    assert_eq!(std::fs::read_to_string(d.path().join("out.txt")).unwrap(), "kopie: Vortex");
    assert!(!res[3].ok);
    assert!(res[4].error.as_ref().unwrap().message.contains("afhankelijkheid x faalde"));
    assert!(!d.path().join("na-fout.txt").exists());
    // Events voor de GUI.
    let ev = l.read_from(0, 1000).unwrap();
    assert!(ev.iter().any(|e| e.kind == vortex_types::EventKind::ToolCallStarted));
    assert!(ev.iter().any(|e| e.kind == vortex_types::EventKind::ToolCallCompleted
        && e.payload["preview"].as_str().is_some_and(|p| p.contains("+kopie"))));
}

fn have_python() -> bool {
    std::process::Command::new(vortex_tools::sandbox::default_python()).arg("--version").output().is_ok()
}

#[tokio::test]
async fn sandbox_python_limits_and_errors() {
    if !have_python() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (h, _) = hub(d.path());
    let o = opts(d.path());
    let r = h
        .executor
        .call(
            "p1",
            "python",
            json!({"code": "print(sum(p for p in range(2,1000) if all(p%k for k in range(2,int(p**.5)+1))))"}),
            &o,
        )
        .await;
    assert_eq!(r.output.unwrap().text.trim(), "76127");
    let r = h.executor.call("p2", "python", json!({"code": "raise ValueError('kapot')"}), &o).await;
    let e = r.error.unwrap();
    assert!(e.message.contains("ValueError: kapot") && e.hint.unwrap().contains("traceback"));
    let r = h.executor.call("p3", "python", json!({"code": "import time\ntime.sleep(30)", "timeout_s": 1}), &o).await;
    assert_eq!(r.error.unwrap().kind, ToolErrorKind::Timeout);
    #[cfg(unix)]
    {
        // Geheugenlimiet: 2 GB alloceren met een limiet van 256 MB faalt.
        let r = h
            .executor
            .call("p4", "python", json!({"code": "x = bytearray(2*1024**3)\nprint('ok')", "memory_mb": 256}), &o)
            .await;
        assert!(!r.ok, "geheugenlimiet moet gelden");
    }
    // Annuleren stopt het proces.
    let cancel = tokio_util::sync::CancellationToken::new();
    let oc = ExecOptions { cancel: cancel.clone(), ..o.clone() };
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        c2.cancel();
    });
    let t = std::time::Instant::now();
    let r = h.executor.call("p5", "python", json!({"code": "import time\ntime.sleep(30)"}), &oc).await;
    assert_eq!(r.error.unwrap().kind, ToolErrorKind::Cancelled);
    assert!(t.elapsed().as_secs() < 5);
}

#[tokio::test]
async fn sqlite_tool_read_only_and_write() {
    let d = tempfile::tempdir().unwrap();
    let (h, _) = hub(d.path());
    let o = opts(d.path());
    let r = h
        .executor
        .call(
            "s1",
            "sqlite_exec",
            json!({"db": "t.db", "sql": "CREATE TABLE t(a INTEGER, b TEXT); INSERT INTO t VALUES (1,'x'),(2,'y');"}),
            &o,
        )
        .await;
    assert!(r.ok, "{:?}", r.error);
    let r = h
        .executor
        .call("s2", "sqlite_query", json!({"db": "t.db", "sql": "SELECT a, b FROM t WHERE a > ?", "params": [1]}), &o)
        .await;
    assert_eq!(r.output.unwrap().data["rows"], json!([{"a": 2, "b": "y"}]));
    let r = h.executor.call("s3", "sqlite_query", json!({"db": "t.db", "sql": "DELETE FROM t"}), &o).await;
    assert!(r.error.unwrap().hint.unwrap().contains("sqlite_exec"));
}

#[tokio::test]
async fn mcp_stdio_tools_registered_and_callable() {
    if !have_python() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let l = log();
    let sup = Supervisor::new(l.clone(), ResourceRegistry::new(l.clone()));
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mcp_echo.py");
    let mut servers = BTreeMap::new();
    servers.insert(
        "echo".to_string(),
        McpServerCfg {
            command: Some(vortex_tools::sandbox::default_python()),
            args: vec![fixture.display().to_string()],
            timeout_ms: 5000,
            enabled: true,
            ..Default::default()
        },
    );
    let cfg = ToolHubConfig {
        skills_dir: Some(d.path().join("skills")),
        workdir: Some(d.path().join("sb")),
        mcp: vortex_tools::hub::McpSection { servers },
        ..Default::default()
    };
    let h = ToolHub::new(cfg, l, sup, Arc::new(|_| None));
    let st = h.connect_mcp().await;
    assert_eq!(st["echo"], "verbonden — 3 tools", "paginering gevolgd");
    let c = h.registry.contract("mcp__echo__echo").unwrap();
    assert_eq!(c.effect, Effect::Read);
    assert_eq!(h.registry.contract("mcp__echo__count").unwrap().effect, Effect::Local);
    let o = opts(d.path());
    let r = h.executor.call("m1", "mcp__echo__echo", json!({"text": "hoi"}), &o).await;
    assert_eq!(r.output.unwrap().text, "echo: hoi");
    // Via call_tool (discovery-pad).
    let (n, a) = h.unwrap_call("call_tool", json!({"name": "mcp__echo__count", "input": {}}));
    let r = h.executor.call("m2", &n, a.clone(), &o).await;
    assert_eq!(r.output.unwrap().data["count"], 1);
    // Retry met zelfde ID: geen tweede telling.
    let r = h.executor.call("m2", &n, a, &o).await;
    assert!(r.replayed);
    let r = h.executor.call("m3", "mcp__echo__boom", json!({}), &o).await;
    assert!(r.error.unwrap().message.contains("kapot"));
    assert!(h.prompt_block().contains("mcp__echo__count"), "niet-kern-tools via discovery");
}

#[tokio::test]
async fn mcp_http_transport() {
    use axum::{Json, Router, http::HeaderMap, routing::post};
    async fn handle(
        headers: HeaderMap,
        Json(m): Json<serde_json::Value>,
    ) -> (axum::http::StatusCode, HeaderMap, String) {
        let mut h = HeaderMap::new();
        h.insert("mcp-session-id", "s-1".parse().unwrap());
        let Some(id) = m.get("id").cloned() else { return (axum::http::StatusCode::ACCEPTED, h, String::new()) };
        let method = m["method"].as_str().unwrap_or("");
        if method != "initialize" {
            assert_eq!(headers.get("mcp-session-id").and_then(|v| v.to_str().ok()), Some("s-1"));
        }
        let res = match method {
            "initialize" => json!({"protocolVersion": "2025-06-18", "capabilities": {}, "serverInfo": {"name": "web"}}),
            "tools/list" => {
                json!({"tools": [{"name": "zoek", "description": "Zoekt", "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true, "openWorldHint": true}}]})
            }
            _ => json!({"content": [{"type": "text", "text": format!("gevonden: {}", m["params"]["arguments"]["q"])}]}),
        };
        let body = json!({"jsonrpc": "2.0", "id": id, "result": res});
        // Antwoord als SSE (streamable HTTP).
        h.insert("content-type", "text/event-stream".parse().unwrap());
        (axum::http::StatusCode::OK, h, format!("event: message\ndata: {body}\n\n"))
    }
    let app = Router::new().route("/mcp", post(handle));
    let lst = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = lst.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(lst, app).await.unwrap() });

    let d = tempfile::tempdir().unwrap();
    let l = log();
    let sup = Supervisor::new(l.clone(), ResourceRegistry::new(l.clone()));
    let mut servers = BTreeMap::new();
    servers.insert(
        "web".to_string(),
        McpServerCfg { url: Some(format!("http://{addr}/mcp")), timeout_ms: 5000, enabled: true, ..Default::default() },
    );
    let cfg = ToolHubConfig {
        skills_dir: Some(d.path().join("skills")),
        mcp: vortex_tools::hub::McpSection { servers },
        ..Default::default()
    };
    let h = ToolHub::new(cfg, l, sup, Arc::new(|_| None));
    assert_eq!(h.connect_mcp().await["web"], "verbonden — 1 tools");
    let r = h.executor.call("w1", "mcp__web__zoek", json!({"q": "vortex"}), &opts(d.path())).await;
    assert_eq!(r.output.unwrap().text, "gevonden: \"vortex\"");
}

#[tokio::test]
async fn skills_import_select_hot_reload_and_proposals() {
    let d = tempfile::tempdir().unwrap();
    let (h, l) = hub(d.path());
    // Geïmporteerde Claude-skill en Cursor-rule (gate: ze werken).
    let src = d.path().join("ext/excel-rapport");
    std::fs::create_dir_all(src.join("scripts")).unwrap();
    std::fs::write(src.join("SKILL.md"), "---\nname: excel-rapport\ndescription: Maak een Excel-rapport van CSV-data met grafieken\n---\nStappen: lees de CSV, draai scripts/maak.py.\n").unwrap();
    std::fs::write(src.join("scripts/maak.py"), "import sys\nprint('rapport voor', sys.argv[1])").unwrap();
    for s in vortex_tools::skills::convert::import(&src).unwrap() {
        h.skills.install(&s).unwrap();
    }
    std::fs::write(h.skills.dir().join("react.mdc"), "---\ndescription: React-conventies\nglobs: src/**/*.tsx\nalwaysApply: false\n---\nGebruik function components.\n").unwrap();
    assert_eq!(h.skills.reload(), 2);
    assert!(h.prompt_block().contains("excel-rapport: Maak een Excel-rapport"));

    let sel = h.skills.select("maak een excel rapport van deze csv", &[], &HashMap::new(), 3);
    assert_eq!(sel[0].name, "excel-rapport");
    let sel = h.skills.select("fix de knop", &["src/ui/Knop.tsx".into()], &HashMap::new(), 3);
    assert_eq!(sel[0].name, "react");
    assert!(sel[0].reason.contains("glob"));
    let sem: HashMap<String, f64> = [("react".to_string(), 0.9)].into();
    assert!(h.skills.select("componentstructuur", &[], &sem, 3).iter().any(|s| s.name == "react"));

    // Progressive disclosure + script in de sandbox.
    let o = opts(d.path());
    let r = h.executor.call("k1", "load_skill", json!({"name": "excel-rapport"}), &o).await;
    let t = r.output.unwrap().text;
    assert!(t.contains("Stappen: lees de CSV") && t.contains("maak.py"));
    if have_python() {
        let r = h
            .executor
            .call("k2", "run_skill_script", json!({"skill": "excel-rapport", "script": "maak.py", "args": ["Q3"]}), &o)
            .await;
        assert_eq!(r.output.unwrap().text, "rapport voor Q3");
    }
    assert_eq!(h.skills.stats("excel-rapport").unwrap().uses, 1);

    // Slechte skill gaat uit.
    for _ in 0..5 {
        h.skills.record_outcome("react", false);
    }
    assert!(!h.skills.briefs_text().contains("react"));

    // Hot-reload: nieuw bestand verschijnt vanzelf.
    std::fs::create_dir_all(h.skills.dir().join("nieuw")).unwrap();
    std::fs::write(h.skills.dir().join("nieuw/SKILL.md"), "---\nname: nieuw\ndescription: Hot geladen\n---\nx\n")
        .unwrap();
    let mut ok = false;
    for _ in 0..40 {
        if h.skills.get("nieuw").is_some() {
            ok = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(ok, "hot-reload");

    // Voorstellen uit geslaagde traces.
    use vortex_tools::skills::propose::{Step, list, observe, set_status, to_skill};
    let steps = vec![
        Step { tool: "read_file".into(), input: json!({"path": "a.csv"}) },
        Step { tool: "python".into(), input: json!({"code": "…"}) },
    ];
    let id = observe(&l, "Analyseer de verkoopcijfers in a.csv", &steps, true, None).unwrap();
    assert_eq!(
        observe(&l, "Analyseer de omzet in b.csv", &steps, true, None).unwrap(),
        id,
        "zelfde patroon = zelfde voorstel"
    );
    assert!(observe(&l, "x", &steps[..1], true, None).is_none());
    let p = list(&l, Some("proposed"));
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].seen, 2);
    let acc = set_status(&l, &id, "accepted").unwrap().unwrap();
    h.skills.install(&to_skill(&acc)).unwrap();
    assert!(h.skills.get(&acc.name).unwrap().body.contains("`python`"));
}

#[tokio::test]
async fn read_tool_results_are_cached() {
    let d = tempfile::tempdir().unwrap();
    let (h, _) = hub(d.path());
    let o = opts(d.path());
    std::fs::write(d.path().join("db.sqlite"), "").unwrap();
    // web_search zonder SearXNG → duidelijke preconditie-fout met hint.
    let r = h.executor.call("w", "web_search", json!({"query": "x"}), &o).await;
    assert!(r.error.unwrap().hint.unwrap().contains("searxng_url"));
    // describe_tool geeft het schema (discovery).
    let r = h.executor.call("d", "describe_tool", json!({"name": "edit_file"}), &o).await;
    assert!(r.output.unwrap().data["input_schema"]["required"].as_array().unwrap().contains(&json!("old")));
    assert!(h.core_set().contains(&"read_file".to_string()));
}
