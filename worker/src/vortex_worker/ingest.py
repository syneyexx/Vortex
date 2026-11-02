"""Ingest (W10.01): bestanden/mappen → parsen → JSONL per document (op de koude schijf, W10.14)
→ de kern leest de JSONL en schrijft naar het geheugen (chunken, contextbeschrijving, embeddings,
claims). Incrementeel: zelfde content-hash = overslaan."""

# VX:W10.01
from __future__ import annotations

import json
import threading
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path

from vortex_worker.parsers import SUPPORTED, parse

SKIP_DIRS = {".git", "node_modules", "target", "__pycache__", ".venv", "dist"}


@dataclass
class Update:
    kind: str  # "progress" | "batch" | "error" | "done"
    fraction: float = -1.0
    stage: str = ""
    message: str = ""
    path: str = ""
    size: int = 0
    content_hash: str = ""


def collect(paths: list[str]) -> list[Path]:
    out: list[Path] = []
    for p in map(Path, paths):
        if p.is_dir():
            for f in sorted(p.rglob("*")):
                if f.is_file() and f.suffix.lower() in SUPPORTED and not (set(f.parts) & SKIP_DIRS):
                    out.append(f)
        elif p.is_file():
            out.append(p)
    return out


def ingest(
    paths: list[str], out_dir: Path, cancel: threading.Event | None = None, known: Callable[[str], bool] | None = None
) -> Iterator[Update]:
    files = collect(paths)
    out_dir.mkdir(parents=True, exist_ok=True)
    if not files:
        yield Update("error", message="geen ondersteunde bestanden gevonden")
        yield Update("done")
        return
    for i, f in enumerate(files):
        if cancel is not None and cancel.is_set():
            yield Update("error", message="geannuleerd")
            break
        yield Update("progress", fraction=i / len(files), stage="parsen", message=f.name)
        try:
            parsed = parse(f)
        except Exception as e:  # één kapot bestand stopt de rest niet
            yield Update("error", message=f"{f.name}: {e}")
            continue
        if known is not None and known(parsed.content_hash):
            yield Update("progress", fraction=(i + 1) / len(files), stage="ongewijzigd", message=f.name)
            continue
        target = out_dir / f"{parsed.content_hash}.jsonl"
        with target.open("w", encoding="utf-8") as w:
            w.write(
                json.dumps(
                    {
                        "type": "doc",
                        "path": str(f.resolve()),
                        "content_hash": parsed.content_hash,
                        "metadata": parsed.metadata,
                    },
                    ensure_ascii=False,
                )
                + "\n"
            )
            for s in parsed.sections:
                w.write(
                    json.dumps(
                        {"type": "section", "title": s.title, "level": s.level, "text": s.text, "locator": s.locator},
                        ensure_ascii=False,
                    )
                    + "\n"
                )
        yield Update(
            "batch", path=str(target), size=target.stat().st_size, content_hash=parsed.content_hash, message=f.name
        )
    yield Update("progress", fraction=1.0, stage="klaar")
    yield Update("done")
