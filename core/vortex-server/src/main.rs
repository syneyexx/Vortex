//! `vortex` — het kern-proces en de CLI.
//!
//! * `vortex serve`  — start de kern (crash-herstel, config hot-reload, dispatcher).
//! * `vortex status` — GPU's, modelservers, kern-database.
//! * `vortex runs`   — lopende/recente runs.
// VX:W00.15

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc; // VX:W01.10

mod bench_cli;
mod status;

#[derive(Debug, Parser)]
#[command(name = "vortex", version, about = "Vortex — lokaal AI-framework")]
struct Cli {
    /// Map met config (models.toml, paths.toml, prompts/, roles/, skills/).
    #[arg(long, env = "VORTEX_CONFIG", default_value = "config")]
    config: PathBuf,
    /// Map voor data (SQLite e.d.). Standaard uit config/paths.toml of ./data.
    #[arg(long, env = "VORTEX_DATA")]
    data: Option<PathBuf>,
    /// JSON-logs i.p.v. leesbare tekst.
    #[arg(long)]
    json_logs: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Start de kern + API voor de GUI.
    Serve {
        /// Adres van de API (alleen lokaal; telefoon via Tailscale in W15).
        #[arg(long, env = "VORTEX_LISTEN", default_value = "127.0.0.1:7420")]
        listen: String,
        /// Map met de gebouwde GUI (`app/dist`) om via de browser te serveren.
        #[arg(long, default_value = "../app/dist")]
        app_dist: PathBuf,
    },
    /// Toon status van GPU's, modelservers en kern.
    Status {
        /// Uitvoer als JSON.
        #[arg(long)]
        json: bool,
    },
    /// Eval draaien via de draaiende kern en vergelijken met de vorige run (W06).
    Eval {
        /// Suite (bestandsnaam zonder .jsonl) of "all".
        #[arg(long, default_value = "all")]
        suite: String,
        #[arg(long)]
        label: Option<String>,
        /// API van de kern.
        #[arg(long, env = "VORTEX_API", default_value = "http://127.0.0.1:7420")]
        api: String,
        /// Modus voor alle taken: "quick" (single-agent) of "deep" (debatteam).
        #[arg(long)]
        mode: Option<String>,
        /// Alleen de retrieval-eval.
        #[arg(long)]
        retrieval: bool,
        /// Alleen de lekkagecontrole.
        #[arg(long)]
        leakage: bool,
    },
    /// 🔒 Prestatiedoelen meten (alleen op commando; --quick = korte harnas-check).
    Perf {
        #[arg(long, env = "VORTEX_API", default_value = "http://127.0.0.1:7420")]
        api: String,
        #[arg(long)]
        quick: bool,
    },
    /// 🔒 Soak-test (24 u alleen op commando): gesimuleerd gebruik + RSS-trend.
    Soak {
        #[arg(long, env = "VORTEX_API", default_value = "http://127.0.0.1:7420")]
        api: String,
        #[arg(long, default_value_t = 1440)]
        minutes: u64,
    },
    /// 🔒 Tiering-benchmark (TB-simulatie alleen op commando): N synthetische herinneringen naar de
    /// koude laag (zstd), dan p50/p95 zoeklatency over de koude blokken.
    Tierbench {
        #[arg(long, default_value_t = 20_000)]
        memories: usize,
        /// Koude schijf (HDD) voor de blokken.
        #[arg(long)]
        cold: Option<std::path::PathBuf>,
    },
    /// 🔒 Fault injection (W17.06, GROTE TEST — alleen op commando): Vortex in een tijdelijke
    /// datamap met de echte config, modelverkeer via een fout-proxy (weg, afgekapt, verminkt,
    /// traag), crash + herstart, volle schijf bij backup. Rapport + exitcode.
    Faults {
        /// Wachttijd per scenario (s).
        #[arg(long, default_value_t = 180)]
        timeout: u64,
    },
    /// 🔒 Ablations (W17.02, GROTE TEST — alleen op commando): elke component uit vs alles aan op
    /// de eval-suites, gepaard; toont per component de meetbare waarde.
    Ablate {
        #[arg(long, env = "VORTEX_API", default_value = "http://127.0.0.1:7420")]
        api: String,
        /// Suites (leeg = alle).
        #[arg(long)]
        suite: Vec<String>,
        /// Componenten (leeg = memory, debate, system_prompt, reasoning).
        #[arg(long)]
        component: Vec<String>,
    },
    /// Speculative decoding meten (W17.08, op de PC met GPU): tok/s met en zonder draft-model.
    /// Zet daarna `spec_gain = <uitkomst>` bij het model in models.toml (≥ 1,05 → aan).
    Specbench {
        #[arg(long)]
        model: String,
        #[arg(long, default_value_t = 3)]
        rounds: usize,
    },
    /// Backup terugzetten (W15.10). De kern moet gestopt zijn; de huidige database wordt eerst
    /// bewaard als `vortex.db.voor-restore`.
    Restore {
        /// Backupbestand (`<koude schijf>/backups/bk_….db`).
        file: PathBuf,
        /// Ook de meegebackupte config terugzetten.
        #[arg(long)]
        with_config: bool,
    },
    /// Toon runs.
    Runs {
        /// Ook afgeronde runs.
        #[arg(long)]
        all: bool,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

fn data_dir(cli: &Cli) -> PathBuf {
    if let Some(d) = &cli.data {
        return d.clone();
    }
    let paths = cli.config.join("paths.toml");
    if let Ok(text) = std::fs::read_to_string(&paths)
        && let Ok(t) = text.parse::<toml::Table>()
        && let Some(d) = t.get("data_dir").and_then(|v| v.as_str())
    {
        // Ontbrekende schijf/map is geen reden om te crashen: terugvallen op ./data (aanpasbaar in
        // de GUI onder Instellingen → Mappen).
        let p = PathBuf::from(d);
        let usable = p.is_dir() || p.parent().is_none_or(|q| q.as_os_str().is_empty() || q.is_dir());
        if usable {
            return p;
        }
        eprintln!("Let op: data_dir {d} is niet bruikbaar (schijf/map ontbreekt) — ./data wordt gebruikt.");
    }
    PathBuf::from("data")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    #[cfg(feature = "console")]
    console_subscriber::init();
    #[cfg(not(feature = "console"))]
    vortex_core::telemetry::init(if cli.json_logs {
        vortex_core::telemetry::LogFormat::Json
    } else {
        vortex_core::telemetry::LogFormat::Pretty
    });

    match &cli.cmd {
        Cmd::Serve { listen, app_dist } => serve(&cli, listen, app_dist).await,
        Cmd::Restore { file, with_config } => restore(&cli, file, *with_config).await,
        Cmd::Faults { timeout } => faults(&cli, *timeout).await,
        Cmd::Specbench { model, rounds } => specbench(&cli, model, *rounds).await,
        Cmd::Ablate { api, suite, component } => ablate(api, suite, component).await,
        Cmd::Status { json } => status::run(&cli.config, &data_dir(&cli), *json).await,
        Cmd::Runs { all, limit } => runs(&cli, *all, *limit),
        Cmd::Perf { api, quick } => {
            let ok = bench_cli::perf(api, *quick).await?;
            anyhow::ensure!(ok, "prestatiedoel(en) niet gehaald");
            Ok(())
        }
        Cmd::Tierbench { memories, cold } => {
            bench_cli::tierbench(*memories, cold.clone().unwrap_or_else(|| data_dir(&cli).join("tierbench"))).await
        }
        Cmd::Soak { api, minutes } => {
            let ok = bench_cli::soak(api, *minutes, &data_dir(&cli).join("soak")).await?;
            anyhow::ensure!(ok, "geheugen stijgt");
            Ok(())
        }
        Cmd::Eval { suite, label, api, retrieval, leakage, mode } => {
            eval_cmd(api, suite, label.as_deref(), mode.as_deref(), *retrieval, *leakage).await
        }
    }
}

async fn serve(cli: &Cli, listen: &str, app_dist: &std::path::Path) -> anyhow::Result<()> {
    let opts = vortex_core::KernelOptions::new(data_dir(cli), &cli.config);
    let (kernel, report) = vortex_core::Kernel::open(opts).await.context("kern openen")?;
    println!("{}", report.summary());
    let stop = kernel.shutdown_token();
    let disp = kernel.dispatcher().spawn(stop.child_token(), Duration::from_secs(1));
    let state = vortex_server::api::build_state(kernel.clone())?;
    let app = vortex_server::api::router(state, Some(app_dist.to_path_buf()));
    let listener = tokio::net::TcpListener::bind(listen).await.with_context(|| format!("luisteren op {listen}"))?;
    println!(
        "Vortex-kern draait (config {}) — API op http://{listen}. Ctrl+C om te stoppen.",
        kernel.config().current().version
    );
    // Telefoon via Tailscale (W15.08): extra listener op het tailnet-adres, geen open poorten.
    let cfg = kernel.config().current();
    if cfg.get::<bool>("autonomy.remote.tailscale").ok().flatten().unwrap_or(false) {
        let port = listen.rsplit(':').next().unwrap_or("7420");
        match vortex_server::autonomy::tailscale_ip() {
            Some(ip) => {
                let addr = format!("{ip}:{port}");
                match tokio::net::TcpListener::bind(&addr).await {
                    Ok(l) => {
                        println!("Telefoon (Tailscale): http://{addr}");
                        let (app2, st2) = (app.clone(), stop.clone());
                        tokio::spawn(async move {
                            axum::serve(l, app2).with_graceful_shutdown(async move { st2.cancelled().await }).await
                        });
                    }
                    Err(e) => eprintln!("Tailscale-listener op {addr} mislukt: {e}"),
                }
            }
            None => eprintln!("Tailscale aan in config, maar `tailscale ip -4` gaf geen adres (draait Tailscale?)"),
        }
    }
    let srv_stop = stop.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).with_graceful_shutdown(async move { srv_stop.cancelled().await }).await
    });
    tokio::signal::ctrl_c().await?;
    println!("Afsluiten…");
    kernel.shutdown(Duration::from_secs(5)).await?;
    let _ = disp.await;
    let _ = server.await;
    Ok(())
}

async fn ablate(api: &str, suites: &[String], comps: &[String]) -> anyhow::Result<()> {
    let http = reqwest::Client::new();
    let r: serde_json::Value = http
        .post(format!("{api}/api/experiments/ablation"))
        .json(&serde_json::json!({"suites": suites, "components": comps, "confirm": "GROTE TESTEN NU"}))
        .send()
        .await?
        .json()
        .await?;
    let id = r["id"].as_str().ok_or_else(|| anyhow::anyhow!("{r}"))?.to_string();
    println!("Ablation gestart ({id}); dit duurt lang…");
    loop {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let e: serde_json::Value = http.get(format!("{api}/api/experiments/{id}")).send().await?.json().await?;
        if e["status"] == "running" {
            continue;
        }
        for c in e["result"]["components"].as_array().into_iter().flatten() {
            println!(
                "{:<14} volledig {:.3} · zonder {:.3} · waarde {:+.3} → {}",
                c["component"].as_str().unwrap_or(""),
                c["full"].as_f64().unwrap_or(0.0),
                c["without"].as_f64().unwrap_or(0.0),
                c["value"].as_f64().unwrap_or(0.0),
                c["verdict"].as_str().unwrap_or("")
            );
        }
        anyhow::ensure!(e["status"] == "done", "ablation mislukt: {}", e["result"]);
        return Ok(());
    }
}

async fn specbench(cli: &Cli, model: &str, rounds: usize) -> anyhow::Result<()> {
    use vortex_models::backend::{Backend, ChatRequest, Message};
    let opts = vortex_core::KernelOptions::new(data_dir(cli), &cli.config);
    let (kernel, _) = vortex_core::Kernel::open(opts).await?;
    let cfg = kernel.config().current();
    let reg = vortex_models::Registry::from_config(&cfg)?;
    let entry = reg.models.get(model).cloned().ok_or_else(|| anyhow::anyhow!("onbekend model {model}"))?;
    anyhow::ensure!(entry.draft.is_some(), "geen `draft` ingesteld bij {model}");
    let srv = reg.server(&entry.gpu).ok_or_else(|| anyhow::anyhow!("server onbekend"))?;
    let port: u16 =
        srv.url.rsplit(':').next().and_then(|p| p.trim_end_matches('/').parse().ok()).unwrap_or(8080) + 1000;
    let lc = vortex_models::LaunchConfig {
        program: cfg.get::<String>("paths.llama_server").ok().flatten().unwrap_or_else(|| "llama-server".into()),
        program_args: srv.launch_args.clone(),
        host: "127.0.0.1".into(),
        port,
        gpu: srv.gpu,
        log_dir: data_dir(cli).join("logs"),
        health_timeout: Duration::from_secs(300),
        drain_timeout: Duration::from_secs(10),
        slot_save_dir: None,
    };
    let ms = vortex_models::ModelServer::new("specbench", lc, kernel.supervisor().clone(), kernel.events().clone());
    let prompts = [
        "Schrijf een Python-functie die priemgetallen tot n zeeft, met uitleg.",
        "Leg uit hoe een B-tree werkt, in vijf alinea's.",
        "Vat de oorzaken van de Eerste Wereldoorlog samen.",
    ];
    let measure = |with: bool| {
        let mut e = entry.clone();
        e.spec_gain = if with { Some(99.0) } else { None };
        let ms = ms.clone();
        async move {
            ms.load(model, &e).await?;
            let g = ms.begin_request().await?;
            let mut tps = Vec::new();
            for i in 0..rounds {
                let mut req = ChatRequest::new(vec![Message::user(prompts[i % prompts.len()])]);
                req.max_tokens = Some(256);
                req.temperature = Some(0.0);
                let r = g.backend.chat(&req, &tokio_util::sync::CancellationToken::new()).await?;
                tps.push(r.metrics.decode_tps.unwrap_or(0.0));
            }
            drop(g);
            ms.unload().await?;
            Ok::<f64, anyhow::Error>(tps.iter().sum::<f64>() / tps.len().max(1) as f64)
        }
    };
    let base = measure(false).await?;
    let with = measure(true).await?;
    let gain = if base > 0.0 { with / base } else { 0.0 };
    println!(
        "{model}: zonder draft {base:.1} tok/s, met draft {with:.1} tok/s → spec_gain = {gain:.2} ({})",
        if gain >= vortex_models::lifecycle::SPEC_MIN_GAIN { "AAN" } else { "uit laten" }
    );
    std::fs::write(
        data_dir(cli).join("specbench.json"),
        serde_json::json!({"model": model, "base_tps": base, "draft_tps": with, "spec_gain": gain}).to_string(),
    )?;
    kernel.shutdown(Duration::from_secs(5)).await?;
    Ok(())
}

async fn faults(cli: &Cli, timeout: u64) -> anyhow::Result<()> {
    use vortex_core::faults::{FaultMode, FaultProxy};
    let tmp = std::env::temp_dir().join(format!("vortex-faults-{}", std::process::id()));
    let cfg_dir = tmp.join("config");
    std::fs::create_dir_all(&cfg_dir)?;
    for e in std::fs::read_dir(&cli.config)? {
        let e = e?;
        if e.path().is_file() {
            std::fs::copy(e.path(), cfg_dir.join(e.file_name()))?;
        } else if e.path().is_dir() {
            let d = cfg_dir.join(e.file_name());
            std::fs::create_dir_all(&d)?;
            for f in std::fs::read_dir(e.path())?.flatten() {
                if f.path().is_file() {
                    std::fs::copy(f.path(), d.join(f.file_name()))?;
                }
            }
        }
    }
    // Elke modelserver via een proxy.
    let models = std::fs::read_to_string(cfg_dir.join("models.toml"))?;
    let mut rewritten = models.clone();
    let mut proxies = Vec::new();
    for line in models.lines().filter(|l| l.trim_start().starts_with("url")) {
        if let Some(u) = line.split('"').nth(1) {
            let host = u.trim_start_matches("http://").trim_end_matches('/');
            let p = FaultProxy::start(host).await?;
            rewritten = rewritten.replace(u, &p.url());
            proxies.push(p);
        }
    }
    std::fs::write(cfg_dir.join("models.toml"), rewritten)?;
    let set = |m: FaultMode| proxies.iter().for_each(|p| p.set(m));
    let mut report = Vec::new();
    let data = tmp.join("data");
    {
        let (kernel, _) = vortex_core::Kernel::open(vortex_core::KernelOptions::new(data.clone(), &cfg_dir)).await?;
        let state = vortex_server::api::build_state(kernel.clone())?;
        for (name, mode) in [
            ("dienst weg", FaultMode::Refuse),
            ("verbinding weg midden in stream", FaultMode::CutAfter(600)),
            ("kapotte uitvoer", FaultMode::Garble),
            ("trage dienst", FaultMode::Delay(1500)),
            ("normaal", FaultMode::Pass),
        ] {
            set(mode);
            let sess = state.chat.new_session()?;
            let t0 = std::time::Instant::now();
            let r = state.chat.send(
                sess.id,
                vortex_server::chat::SendRequest {
                    text: format!("Fouttest: {name}. Antwoord kort."),
                    mode: vortex_server::chat::Mode::Quick,
                    model: None,
                    attachments: vec![],
                },
            );
            let mut status = "send-fout".to_string();
            if r.is_ok() {
                while t0.elapsed().as_secs() < timeout {
                    let ms = state.chat.sessions().messages(sess.id).unwrap_or_default();
                    if let Some(m) = ms.iter().find(|m| m.role == "assistant").filter(|m| m.status != "streaming") {
                        status = m.status.clone();
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                if status == "send-fout" {
                    status = "HANGT".into();
                }
            }
            let ok = match mode {
                FaultMode::Pass | FaultMode::Delay(_) => status == "complete",
                FaultMode::Refuse => status == "failed",
                _ => status != "HANGT",
            };
            println!(
                "{:<34} → {status:<10} {:>6} ms {}",
                name,
                t0.elapsed().as_millis(),
                if ok { "OK" } else { "FOUT" }
            );
            report.push(serde_json::json!({"scenario": name, "status": status, "ms": t0.elapsed().as_millis() as u64, "ok": ok}));
        }
        // Volle schijf bij backup: koude map is een bestand → nette fout, geen crash.
        std::fs::write(tmp.join("vol"), "x")?;
        std::fs::write(
            cfg_dir.join("paths.toml"),
            format!("cold_dir = {:?}\n", tmp.join("vol").display().to_string()),
        )?;
        kernel.config().reload()?;
        let b = vortex_server::autonomy::backup::run(&state).await;
        println!(
            "{:<34} → {}",
            "volle/onbruikbare schijf (backup)",
            if b.is_err() { "nette fout OK" } else { "FOUT: geen fout" }
        );
        report.push(serde_json::json!({"scenario": "schijf", "ok": b.is_err(), "error": b.err()}));
        // Crash: niet netjes afsluiten (lopende run achterlaten).
        set(FaultMode::Delay(5000));
        let sess = state.chat.new_session()?;
        let _ = state.chat.send(
            sess.id,
            vortex_server::chat::SendRequest {
                text: "Lang antwoord dat onderbroken wordt.".into(),
                mode: vortex_server::chat::Mode::Quick,
                model: None,
                attachments: vec![],
            },
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
        drop(state);
        drop(kernel);
    }
    set(FaultMode::Pass);
    let (k2, crash) = vortex_core::Kernel::open(vortex_core::KernelOptions::new(data.clone(), &cfg_dir)).await?;
    println!("{:<34} → {}", "crash + herstart", crash.summary());
    report.push(serde_json::json!({"scenario": "crash", "recovery": crash.summary()}));
    k2.shutdown(Duration::from_secs(5)).await?;
    let ok = report.iter().all(|r| r["ok"].as_bool() != Some(false));
    std::fs::write(tmp.join("faults-report.json"), serde_json::to_string_pretty(&report)?)?;
    println!("Rapport: {}", tmp.join("faults-report.json").display());
    anyhow::ensure!(ok, "fault injection: niet alles herstelde correct");
    Ok(())
}

async fn restore(cli: &Cli, file: &std::path::Path, with_config: bool) -> anyhow::Result<()> {
    let db = data_dir(cli).join("vortex.db");
    if std::net::TcpStream::connect("127.0.0.1:7420").is_ok() {
        anyhow::bail!("de kern lijkt te draaien (poort 7420): eerst stoppen");
    }
    if db.exists() {
        std::fs::copy(&db, db.with_extension("db.voor-restore")).context("huidige database bewaren")?;
        for ext in ["db-wal", "db-shm"] {
            let _ = std::fs::remove_file(db.with_extension(ext));
        }
    }
    vortex_memory::MemoryStore::restore(file, &db).context("terugzetten")?;
    if with_config {
        let src = file.with_extension("config");
        anyhow::ensure!(src.is_dir(), "geen config-map naast de backup ({})", src.display());
        for e in std::fs::read_dir(&src)? {
            let e = e?;
            if e.path().is_file() {
                std::fs::copy(e.path(), cli.config.join(e.file_name()))?;
            }
        }
    }
    println!("Teruggezet: {} → {} (vorige database: vortex.db.voor-restore)", file.display(), db.display());
    Ok(())
}

async fn eval_cmd(
    api: &str,
    suite: &str,
    label: Option<&str>,
    mode: Option<&str>,
    retrieval: bool,
    leakage: bool,
) -> anyhow::Result<()> {
    let c = reqwest::Client::new();
    if leakage {
        let v: serde_json::Value = c.get(format!("{api}/api/eval/leakage")).send().await?.json().await?;
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    if retrieval {
        let v: serde_json::Value = c.post(format!("{api}/api/eval/retrieval")).send().await?.json().await?;
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    let before: Vec<serde_json::Value> = c.get(format!("{api}/api/eval/runs")).send().await?.json().await?;
    let known: std::collections::HashSet<String> =
        before.iter().filter_map(|r| r["id"].as_str().map(String::from)).collect();
    let started: serde_json::Value = c
        .post(format!("{api}/api/eval/run"))
        .json(&serde_json::json!({"suite": suite, "label": label, "mode": mode}))
        .send()
        .await?
        .json()
        .await?;
    let names: Vec<String> =
        started["started"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
    anyhow::ensure!(!names.is_empty(), "{started}");
    println!("Gestart: {}", names.join(", "));
    let mut done = std::collections::HashSet::new();
    while done.len() < names.len() {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let runs: Vec<serde_json::Value> = c.get(format!("{api}/api/eval/runs")).send().await?.json().await?;
        for r in runs.iter().filter(|r| !known.contains(r["id"].as_str().unwrap_or("")) && !r["finished_at"].is_null())
        {
            let id = r["id"].as_str().unwrap_or("").to_string();
            if done.insert(id.clone()) {
                let d: serde_json::Value = c.get(format!("{api}/api/eval/runs/{id}")).send().await?.json().await?;
                let cmp = &d["comparison"];
                println!(
                    "{:<12} {}/{} geslaagd ({} overgeslagen) · score {} · {}",
                    r["suite"].as_str().unwrap_or(""),
                    r["passed"],
                    r["n"],
                    r["skipped"],
                    r["score"].as_f64().map(|x| format!("{:.0}%", x * 100.0)).unwrap_or("—".into()),
                    cmp["verdict"].as_str().unwrap_or("")
                );
            }
        }
    }
    Ok(())
}

fn runs(cli: &Cli, all: bool, limit: usize) -> anyhow::Result<()> {
    let db =
        std::sync::Arc::new(vortex_core::Db::open(data_dir(cli).join("vortex.db"), vortex_core::db::CORE_MIGRATIONS)?);
    let store = vortex_core::RunStore::new(vortex_core::EventLog::new(db));
    let list = if all { store.recent(limit)? } else { store.active()? };
    if list.is_empty() {
        println!("Geen runs.");
    }
    for r in list {
        println!(
            "{}  {:<10} {:<11} {:?}  {}",
            r.id,
            r.kind.as_str(),
            r.state.as_str(),
            r.priority,
            r.created_at.format("%Y-%m-%d %H:%M:%S")
        );
    }
    Ok(())
}
