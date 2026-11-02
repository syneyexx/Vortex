# Vortex

Persoonlijk, lokaal AI-framework (harness): Rust-kern, Python ML-worker, Tauri + React GUI.
Framework + taalmodel = één brein: het framework redeneert mee, onthoudt, leert en traint eigen netwerken.

## Installeren en starten (Windows)

1. **`install.bat`** — installeert wat ontbreekt (Git, Python 3.12, uv, Node.js, Rust + C++ build
   tools, FFmpeg, Tesseract, llama.cpp via winget), bouwt de Rust-kern, maakt de Python-omgeving
   van de worker (`worker\.venv`, alle extra's + Playwright-Chromium), optioneel de GPU-pakketten
   (CUDA-PyTorch, training, beeldgeneratie), en bouwt de GUI. Opnieuw draaien mag. Log: `install.log`.
   Opties: `install.bat -Gpu` / `-NoGpu` / `-SkipTools`.
2. Pas `config\paths.toml` (schijven, `llama_server`) en `config\models.toml` (modelbestanden) aan.
3. **`start_vortex.bat`** — start de worker, de modelservers (of laat de kern ze beheren met
   `managed = true`), de kern met de GUI, en opent http://127.0.0.1:7420. **`stop_vortex.bat`** stopt alles.

- **Start hier:** [`VAULT/00-START.md`](VAULT/00-START.md)
- **Alles wat Vortex kan:** [`VAULT/FUNCTIES.md`](VAULT/FUNCTIES.md)
- **Takenlijst (gegenereerd):** [`VAULT/TODO.md`](VAULT/TODO.md)
- **Bouwvolgorde:** [`VAULT/waves/WAVES-OVERZICHT.md`](VAULT/waves/WAVES-OVERZICHT.md)
- **Instructies voor Claude:** [`CLAUDE.md`](CLAUDE.md)

```
python tools/vault/vault.py status   # voortgang per wave
python tools/vault/vault.py check    # VAULT-integriteit
```
