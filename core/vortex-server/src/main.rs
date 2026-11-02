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
        return PathBuf::from(d);
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
