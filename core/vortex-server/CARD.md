# CARD — `vortex-server`

**Doel:** het kern-proces + `vortex` CLI. Later: eventstream naar GUI (W03), lokale OpenAI-API (W15).
**Waves:** W00 (CLI), W03, W15

## Publieke API (CLI)
| Commando | Doet |
| --- | --- |
| `vortex serve` | start kern: crash-herstel, config hot-reload, dispatcher; Ctrl+C = nette afsluiting |
| `vortex status [--json]` | GPU's (nvidia-smi), modelservers (`/health` uit `config/models.toml`), kern-DB |
| `vortex runs [--all] [--limit N]` | runs |
Opties: `--config` (`VORTEX_CONFIG`), `--data` (`VORTEX_DATA`, anders `config/paths.toml` → `data_dir`), `--json-logs`. Log-filter: `VORTEX_LOG`.

## Ingangen
`src/main.rs` (CLI + allocator mimalloc) · `src/status.rs`

## Features
`console` — tokio-console (`RUSTFLAGS="--cfg tokio_unstable" cargo run --features console`).
