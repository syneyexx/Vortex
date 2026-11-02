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
    }
}

async fn serve(cli: &Cli, listen: &str, app_dist: &std::path::Path) -> anyhow::Result<()> {
    let opts = vortex_core::KernelOptions::new(data_dir(cli), &cli.config);
    let (kernel, report) = vortex_core::Kernel::open(opts).await.context("kern openen")?;
    println!("{}", report.summary());
    let stop = kernel.shutdown_token();
    let disp = kernel.dispatcher().spawn(stop.child_token(), Duration::from_secs(1));
    let state = vortex_server::api::build_state(kernel.clone());
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
