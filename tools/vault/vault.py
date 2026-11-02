#!/usr/bin/env python3
"""Vortex VAULT-tool (alleen stdlib, werkt op Windows en Linux).

Gebruik (vanuit de repo-root):
    python tools/vault/vault.py todo      # VAULT/TODO.md opnieuw genereren uit waves/
    python tools/vault/vault.py check     # links, ID's, frontmatter, TODO actueel?
    python tools/vault/vault.py find W04.07   # waar staat dit item (VAULT + code-tags)
    python tools/vault/vault.py status    # voortgang per wave in de terminal

Bron van waarheid voor taken = de wave-files. TODO.md is afgeleid: nooit met de hand bewerken.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VAULT = ROOT / "VAULT"
WAVES = VAULT / "waves"
TODO = VAULT / "TODO.md"

ITEM_RE = re.compile(r"^\s*- \[( |x|X|~)\] `(W\d\d\.\d\d)` (.*)$")
WIKILINK_RE = re.compile(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]")
CODE_TAG_RE = re.compile(r"VX:(W\d\d\.\d\d)")
CODE_EXT = {".rs", ".py", ".ts", ".tsx", ".js", ".toml", ".proto", ".sql", ".glsl", ".wgsl"}
SKIP_DIRS = {".git", "target", "node_modules", ".venv", "dist", "build", "__pycache__"}


def read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def frontmatter(text: str) -> dict[str, str]:
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    if end == -1:
        return {}
    out = {}
    for line in text[3:end].splitlines():
        if ":" in line:
            k, v = line.split(":", 1)
            out[k.strip()] = v.split("#", 1)[0].strip()
    return out


def wave_files() -> list[Path]:
    return sorted(WAVES.glob("W[0-9][0-9]-*.md"))


def parse_waves():
    waves = []
    for f in wave_files():
        text = read(f)
        fm = frontmatter(text)
        title = next((l[2:].strip() for l in text.splitlines() if l.startswith("# ")), f.stem)
        items, section = [], ""
        for line in text.splitlines():
            if line.startswith("## "):
                section = line[3:].strip()
            m = ITEM_RE.match(line)
            if m:
                state = {" ": "open", "x": "af", "X": "af", "~": "bezig"}[m.group(1)]
                items.append({"id": m.group(2), "state": state, "text": m.group(3), "section": section})
        waves.append({"file": f, "stem": f.stem, "title": title, "status": fm.get("status", "?"),
                      "trigger": fm.get("trigger", "").strip('"'), "items": items})
    return waves


def render_todo(waves) -> str:
    total = sum(len(w["items"]) for w in waves)
    done = sum(1 for w in waves for i in w["items"] if i["state"] == "af")
    lines = [
        "---", "type: todo", "generated: true", "---",
        "# TODO — alle taken van Vortex (gegenereerd)",
        "",
        "> **Niet met de hand bewerken.** Bron = `waves/*.md`. Afvinken doe je in de wave-file,",
        "> daarna `python tools/vault/vault.py todo`. Legenda: `[ ]` open · `[~]` bezig · `[x]` af.",
        "> Functionaliteiten per domein (wat Vortex *kan*): [[FUNCTIES]].",
        "",
        f"**Totaal: {done}/{total} af.**",
        "",
        "| Wave | Status | Af | Open |",
        "| --- | --- | --- | --- |",
    ]
    for w in waves:
        d = sum(1 for i in w["items"] if i["state"] == "af")
        lines.append(f"| [[waves/{w['stem']}\\|{w['title']}]] | {w['status']} | {d} | {len(w['items']) - d} |")
    for w in waves:
        lines += ["", f"## {w['title']}", f"Bron: [[waves/{w['stem']}]] · status: `{w['status']}`"]
        if not w["items"]:
            lines.append(f"- Geparkeerd. Trigger: **\"{w['trigger']}\"** — zie [[GEPARKEERD]].")
            continue
        section = None
        for i in w["items"]:
            if i["section"] != section:
                section = i["section"]
                if section:
                    lines += ["", f"**{section}**"]
            box = {"open": " ", "af": "x", "bezig": "~"}[i["state"]]
            lines.append(f"- [{box}] `{i['id']}` {i['text']}")
    return "\n".join(lines) + "\n"


def cmd_todo() -> int:
    TODO.write_text(render_todo(parse_waves()), encoding="utf-8", newline="\n")
    print(f"geschreven: {TODO.relative_to(ROOT)}")
    return 0


def cmd_status() -> int:
    for w in parse_waves():
        n = len(w["items"])
        d = sum(1 for i in w["items"] if i["state"] == "af")
        b = sum(1 for i in w["items"] if i["state"] == "bezig")
        bar = ("#" * round(20 * d / n)).ljust(20, ".") if n else "geparkeerd".ljust(20)
        print(f"{w['stem'][:26]:26} {bar} {d:3}/{n:<3} bezig:{b}")
    return 0


def all_notes() -> dict[str, Path]:
    notes = {}
    for p in VAULT.rglob("*.md"):
        rel = p.relative_to(VAULT).with_suffix("").as_posix()
        notes[rel] = p
        notes.setdefault(p.stem, p)  # korte vorm [[LOG]] mag ook
    return notes


def cmd_check() -> int:
    errors: list[str] = []
    notes = all_notes()
    # 1. wikilinks
    for p in VAULT.rglob("*.md"):
        if "bronnen" in p.parts:
            continue
        for target in WIKILINK_RE.findall(read(p)):
            t = target.strip().replace("\\", "")
            if t not in notes:
                errors.append(f"dode link [[{t}]] in {p.relative_to(ROOT)}")
    # 2. unieke ID's + ID hoort bij eigen wave
    seen: dict[str, Path] = {}
    for w in parse_waves():
        wid = w["stem"][:3]
        for i in w["items"]:
            if i["id"] in seen:
                errors.append(f"dubbel ID {i['id']} in {w['file'].name} en {seen[i['id']].name}")
            seen[i["id"]] = w["file"]
            if not i["id"].startswith(wid):
                errors.append(f"ID {i['id']} staat in verkeerde wave {w['file'].name}")
        # items zonder ID
        for n, line in enumerate(read(w["file"]).splitlines(), 1):
            if re.match(r"^\s*- \[[ xX~]\] ", line) and not ITEM_RE.match(line):
                errors.append(f"item zonder ID: {w['file'].name}:{n}")
    # 3. frontmatter
    for p in list(VAULT.glob("*.md")) + list((VAULT / "modules").glob("*.md")) + wave_files():
        if not frontmatter(read(p)):
            errors.append(f"geen frontmatter: {p.relative_to(ROOT)}")
    # 4. code-tags verwijzen naar bestaande items
    for f, tag in iter_code_tags():
        if tag not in seen:
            errors.append(f"code-tag VX:{tag} in {f} verwijst naar onbekend item")
    # 5. TODO actueel
    if not TODO.exists() or read(TODO) != render_todo(parse_waves()):
        errors.append("VAULT/TODO.md is verouderd → draai: python tools/vault/vault.py todo")
    for e in errors:
        print("FOUT:", e)
    print("VAULT ok" if not errors else f"{len(errors)} fout(en)")
    return 1 if errors else 0


def iter_code_tags():
    for p in ROOT.rglob("*"):
        if p.is_file() and p.suffix in CODE_EXT and not (SKIP_DIRS & set(p.parts)):
            try:
                for m in CODE_TAG_RE.finditer(read(p)):
                    yield p.relative_to(ROOT).as_posix(), m.group(1)
            except UnicodeDecodeError:
                continue


def cmd_find(query: str) -> int:
    q = query.lower()
    for p in sorted(VAULT.rglob("*.md")):
        if p == TODO:
            continue
        for n, line in enumerate(read(p).splitlines(), 1):
            if q in line.lower():
                print(f"{p.relative_to(ROOT).as_posix()}:{n}: {line.strip()[:140]}")
    if re.fullmatch(r"w\d\d\.\d\d", q):
        for f, tag in iter_code_tags():
            if tag.lower() == q:
                print(f"code: {f}")
    return 0


def main(argv: list[str]) -> int:
    if not argv or argv[0] in {"-h", "--help"}:
        print(__doc__)
        return 0
    cmd, rest = argv[0], argv[1:]
    if cmd == "todo":
        return cmd_todo()
    if cmd == "check":
        return cmd_check()
    if cmd == "status":
        return cmd_status()
    if cmd == "find" and rest:
        return cmd_find(" ".join(rest))
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
