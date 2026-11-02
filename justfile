# Vaste commando's voor Vortex (W00.14). Installeren: `cargo install just`. Overzicht: `just`
set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# Alles bouwen (Rust-kern + GUI-frontend)
build:
    cd core && cargo build --workspace
    cd app && pnpm build

# Alle tests (Rust, Python-worker, frontend-typecheck)
test:
    cd core && cargo test --workspace
    cd worker && uv run pytest -q
    cd app && pnpm typecheck

# Format + lint + VAULT + afhankelijkheidsrichting (moet groen vóór commit)
check:
    cd core && cargo fmt --all --check
    cd core && cargo clippy --workspace --all-targets -- -D warnings
    cd worker && uv run ruff check . && uv run ruff format --check .
    cd app && pnpm lint
    python tools/check_deps.py
    python tools/vault/vault.py check

# Automatisch formatteren
fmt:
    cd core && cargo fmt --all
    cd worker && uv run ruff format . && uv run ruff check --fix .
    cd app && pnpm format

# Contract-code opnieuw genereren uit proto/ (Rust gebeurt automatisch bij build)
gen:
    cd worker && uv run python scripts/gen_proto.py
    cd app && pnpm gen

# Kern starten
run:
    cd core && cargo run --release --bin vortex -- --config ../config serve

# Status van GPU's, modelservers en kern
status:
    cd core && cargo run --release --bin vortex -- --config ../config status

# GUI in dev-modus (Tauri)
gui:
    cd app && pnpm tauri dev

# Python-worker starten
worker:
    cd worker && uv run vortex-worker

# Eval-suite (komt in W06)
eval:
    @echo "Eval-runner volgt in W06"

# llama-servers starten/stoppen (Windows)
models-up:
    pwsh scripts/models-up.ps1

models-down:
    pwsh scripts/models-down.ps1

# Voortgang per wave
vault:
    python tools/vault/vault.py status
