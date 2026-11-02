"""Genereert VAULT/30-API-EN-CONFIG.md: alle REST-routes van de kern (uit de broncode), de CLI-
commando's en de configbestanden met hun eerste commentaarregel. Draai na API-wijzigingen:
    python tools/vault/api_overview.py
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "core" / "vortex-server" / "src"
OUT = ROOT / "VAULT" / "30-API-EN-CONFIG.md"


def routes() -> list[tuple[str, str, str]]:
    out = []
    for f in sorted(SRC.rglob("*.rs")):
        text = f.read_text(encoding="utf-8")
        for m in re.finditer(r'\.route\(\s*"([^"]+)",\s*([^)]*\)?[^)]*)\)', text):
            path, handler = m.group(1), m.group(2)
            methods = sorted(set(re.findall(r"\b(get|post|put|patch|delete)\(", handler)))
            out.append((path, ", ".join(x.upper() for x in methods) or "?", str(f.relative_to(ROOT)).replace("\\", "/")))
    return sorted(set(out))


def cli() -> list[tuple[str, str]]:
    text = (SRC / "main.rs").read_text(encoding="utf-8")
    body = text[text.index("enum Cmd") :]
    body = body[: body.index("\n}\n")]
    out, doc = [], []
    for line in body.splitlines():
        s = line.strip()
        if s.startswith("///") and line.startswith("    ///"):
            doc.append(s[3:].strip())
        m = re.match(r"^    ([A-Z][A-Za-z]+)\b", line)
        if m:
            out.append((m.group(1).lower(), " ".join(doc)))
            doc = []
        elif s and not s.startswith("///") and line.startswith("    ") and not line.startswith("     "):
            doc = []
    return out


def configs() -> list[tuple[str, str]]:
    out = []
    for f in sorted((ROOT / "config").glob("*.toml")):
        first = next((ln.lstrip("# ").strip() for ln in f.read_text(encoding="utf-8").splitlines() if ln.startswith("#")), "")
        out.append((f.name, first))
    return out


def main() -> None:
    lines = [
        "---",
        "type: referentie",
        "---",
        "# API, CLI en config (gegenereerd)",
        "",
        "Gegenereerd door `python tools/vault/api_overview.py` — niet met de hand bewerken.",
        "Kern: `http://127.0.0.1:7420`; events live via WebSocket `/api/events?from_seq=N`.",
        "",
        "## CLI (`core/target/release/vortex[.exe]`, globale opties `--config config`, `--data <map>`)",
        "| Commando | Doet |",
        "| --- | --- |",
    ]
    lines += [f"| `vortex {c}` | {d} |" for c, d in cli()]
    lines += ["", "## Configbestanden (`config/`, hot-reload)", "| Bestand | Inhoud |", "| --- | --- |"]
    lines += [f"| `{n}` | {d} |" for n, d in configs()]
    lines += ["", "## REST-routes", "| Pad | Methoden | Bron |", "| --- | --- | --- |"]
    lines += [f"| `{p}` | {m} | `{f}` |" for p, m, f in routes()]
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"geschreven: {OUT.relative_to(ROOT)} ({len(routes())} routes)")


if __name__ == "__main__":
    main()
