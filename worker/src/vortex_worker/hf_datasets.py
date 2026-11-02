"""Hugging Face-datasets (W14.09): streamen, schema herkennen, mapping-voorstel (bevestigd door
de gebruiker in de GUI; het LM kan het voorstel aan Rust-zijde verfijnen), kwaliteitsfilter,
dedup (exact + MinHash, ook tegen bestaande data), decontaminatie t.o.v. de eval-sets, licentie
+ herkomst, en schrijven naar een bestemming: kennis, SFT, DPO, neurale layer of eval-set.

Bron = HF-dataset-ID (`datasets.load_dataset(..., streaming=True)`, zodat 32 GB RAM genoeg is)
of een lokaal bestand (.jsonl/.json/.csv/.parquet) — dat laatste ook voor de tests.
"""

# VX:W14.09
from __future__ import annotations

import csv
import hashlib
import json
import re
from collections.abc import Callable, Iterable, Iterator
from dataclasses import dataclass, field
from pathlib import Path

# ── lezen ─────────────────────────────────────────────────────────────────────


def stream_rows(
    source: str, split: str = "train", config: str | None = None, limit: int | None = None
) -> Iterator[dict]:
    p = Path(source)
    if p.exists():
        it: Iterable[dict]
        if p.suffix == ".jsonl":
            it = (json.loads(line) for line in p.open(encoding="utf-8") if line.strip())
        elif p.suffix == ".json":
            data = json.loads(p.read_text(encoding="utf-8"))
            it = data if isinstance(data, list) else data.get("data", [])
        elif p.suffix == ".csv":
            it = csv.DictReader(p.open(encoding="utf-8", newline=""))
        elif p.suffix == ".parquet":
            import pyarrow.parquet as pq

            it = (r for b in pq.ParquetFile(p).iter_batches(batch_size=1024) for r in b.to_pylist())
        else:
            raise ValueError(f"onbekend bestandstype {p.suffix}")
    else:
        try:
            from datasets import load_dataset
        except ImportError as e:  # pragma: no cover
            raise ImportError("HF-datasets vereist de extra 'hf' (datasets, huggingface_hub)") from e
        it = load_dataset(source, name=config, split=split, streaming=True)
    for n, row in enumerate(it, 1):
        yield dict(row)
        if limit is not None and n >= limit:
            return


def license_of(source: str, given: str | None = None) -> str | None:
    if given:
        return given
    if Path(source).exists():
        return None
    try:
        from huggingface_hub import dataset_info

        info = dataset_info(source)
        card = getattr(info, "card_data", None) or getattr(info, "cardData", None)
        lic = card.get("license") if card else None
        return ",".join(lic) if isinstance(lic, list) else lic
    except Exception:
        return None


# ── schema + mapping ──────────────────────────────────────────────────────────

_CANDS = {
    "messages": ["messages", "conversations", "conversation", "dialog", "chat"],
    "prompt": ["prompt", "question", "instruction", "query", "input", "problem", "context"],
    "response": ["response", "answer", "output", "completion", "solution", "target", "text_answer"],
    "chosen": ["chosen", "preferred", "response_chosen", "better"],
    "rejected": ["rejected", "dispreferred", "response_rejected", "worse"],
    "code": ["code", "content", "func_code", "source", "snippet"],
    "text": ["text", "content", "document", "body", "article", "passage"],
    "title": ["title", "name", "heading", "subject"],
    "label": ["label", "labels", "category", "class", "intent"],
}


def detect_schema(rows: list[dict]) -> dict:
    """Velden + types + soort (chat, preference, qa, code, text)."""
    fields: dict[str, str] = {}
    for r in rows:
        for k, v in r.items():
            fields.setdefault(k, type(v).__name__)
    keys = {k.lower(): k for k in fields}

    def has(kind: str) -> str | None:
        return next((keys[c] for c in _CANDS[kind] if c in keys), None)

    if has("chosen") and has("rejected"):
        kind = "preference"
    elif has("messages") and fields.get(has("messages") or "") == "list":
        kind = "chat"
    elif has("prompt") and has("response"):
        kind = "code" if any("def " in str(r.get(has("response") or "", "")) for r in rows[:20]) else "qa"
    elif has("code") and any(
        re.search(r"\b(def|fn|function|class)\b", str(r.get(has("code") or "", ""))) for r in rows[:20]
    ):
        kind = "code"
    else:
        kind = "text"
    return {"kind": kind, "fields": fields, "n_sample": len(rows)}


def propose_mapping(schema: dict) -> dict:
    keys = {k.lower(): k for k in schema["fields"]}
    m: dict[str, str] = {}
    for target, cands in _CANDS.items():
        for c in cands:
            if c in keys and keys[c] not in m.values():
                m[target] = keys[c]
                break
    return m


def _msgs(v) -> list[dict]:
    out = []
    for x in v or []:
        if isinstance(x, dict):
            role = x.get("role") or x.get("from") or "user"
            role = {"human": "user", "gpt": "assistant", "bot": "assistant", "model": "assistant"}.get(role, role)
            out.append({"role": role, "content": str(x.get("content") or x.get("value") or "")})
    return out


def apply_mapping(row: dict, mapping: dict, kind: str) -> dict | None:
    g = lambda k: row.get(mapping[k]) if k in mapping else None  # noqa: E731
    if kind == "preference":
        p, c, r = g("prompt"), g("chosen"), g("rejected")
        if isinstance(c, list):  # chat-vorm (bv. HH-RLHF in messages)
            cm, rm = _msgs(c), _msgs(r)
            p = p or next((m["content"] for m in cm if m["role"] == "user"), "")
            c = next((m["content"] for m in reversed(cm) if m["role"] == "assistant"), "")
            r = next((m["content"] for m in reversed(rm) if m["role"] == "assistant"), "")
        return {"prompt": str(p or ""), "chosen": str(c or ""), "rejected": str(r or "")} if c and r else None
    if kind == "chat":
        ms = _msgs(g("messages"))
        return {"messages": ms} if len(ms) >= 2 else None
    if kind in ("qa", "code") and g("prompt") is not None and (g("response") is not None):
        return {"prompt": str(g("prompt")), "response": str(g("response"))}
    if kind == "code" and g("code") is not None:
        return {"text": str(g("code")), "title": str(g("title") or "")}
    t = g("text")
    if t is None:
        return None
    out = {"text": str(t), "title": str(g("title") or "")}
    if "label" in mapping:
        out["label"] = g("label")
    return out


def record_text(rec: dict) -> str:
    if "messages" in rec:
        return "\n".join(m["content"] for m in rec["messages"])
    return "\n".join(
        str(rec.get(k, "")) for k in ("prompt", "response", "chosen", "rejected", "title", "text") if rec.get(k)
    )


# ── filter, dedup, decontaminatie ─────────────────────────────────────────────


@dataclass
class Quality:
    min_chars: int = 20
    max_chars: int = 20000
    min_alpha: float = 0.5

    def ok(self, text: str) -> bool:
        t = text.strip()
        if not (self.min_chars <= len(t) <= self.max_chars):
            return False
        alpha = sum(c.isalnum() or c.isspace() for c in t) / max(1, len(t))
        return alpha >= self.min_alpha


def _norm(s: str) -> str:
    return " ".join(re.findall(r"\w+", s.lower()))


def _h64(s: str, seed: int) -> int:
    return int.from_bytes(
        hashlib.blake2b(s.encode(), digest_size=8, salt=seed.to_bytes(8, "little")).digest(), "little"
    )


class MinHashLSH:
    """MinHash (woord-5-grammen) + LSH-banden; near-duplicate bij geschatte Jaccard ≥ drempel."""

    def __init__(self, perms: int = 64, bands: int = 16, threshold: float = 0.8, ngram: int = 5):
        self.perms, self.bands, self.rows, self.th, self.n = perms, bands, perms // bands, threshold, ngram
        self.buckets: dict[tuple[int, bytes], list[int]] = {}
        self.sigs: list[list[int]] = []

    def signature(self, text: str) -> list[int]:
        w = _norm(text).split()
        grams = {" ".join(w[i : i + self.n]) for i in range(max(1, len(w) - self.n + 1))} or {""}
        return [min(_h64(g, s) for g in grams) for s in range(self.perms)]

    def query_add(self, text: str) -> bool:
        """True als (bijna) dubbel met iets eerders; anders toevoegen."""
        sig = self.signature(text)
        keys = [
            (b, hashlib.md5(str(sig[b * self.rows : (b + 1) * self.rows]).encode()).digest()) for b in range(self.bands)
        ]
        for k in keys:
            for j in self.buckets.get(k, []):
                other = self.sigs[j]
                if sum(a == b for a, b in zip(sig, other, strict=True)) / self.perms >= self.th:
                    return True
        idx = len(self.sigs)
        self.sigs.append(sig)
        for k in keys:
            self.buckets.setdefault(k, []).append(idx)
        return False


def _grams8(s: str) -> set[int]:
    w = _norm(s).split()
    if len(w) < 8:
        return {hash(" ".join(w))} if w else set()
    return {hash(" ".join(w[i : i + 8])) for i in range(len(w) - 7)}


def eval_index(evals_dir: str | None) -> set[int]:
    idx: set[int] = set()
    if not evals_dir or not Path(evals_dir).exists():
        return idx
    for f in Path(evals_dir).glob("*.jsonl"):
        for line in f.open(encoding="utf-8"):
            line = line.strip()
            if not line or line.startswith("//"):
                continue
            try:
                idx |= _grams8(json.loads(line).get("prompt", ""))
            except json.JSONDecodeError:
                continue
    return idx


# ── bestemmingen ──────────────────────────────────────────────────────────────

DESTINATIONS = ("knowledge", "sft", "dpo", "layer", "eval")


def to_destination(rec: dict, dest: str, kind: str, source: str, i: int, license_: str | None) -> dict | None:
    prov = {"source": source, "row": i, "license": license_}
    if dest == "knowledge":
        text = rec.get("text") or record_text(rec)
        title = rec.get("title") or text.split("\n")[0][:80]
        return {"title": title, "content": text, "source_uri": f"hf:{source}", "locator": f"rij {i}", **prov}
    if dest == "sft":
        if "messages" in rec:
            return {"messages": rec["messages"], **prov}
        if "prompt" in rec and "response" in rec:
            return {
                "messages": [
                    {"role": "user", "content": rec["prompt"]},
                    {"role": "assistant", "content": rec["response"]},
                ],
                **prov,
            }
        return None
    if dest == "dpo":
        return (
            {"prompt": rec["prompt"], "chosen": rec["chosen"], "rejected": rec["rejected"], **prov}
            if kind == "preference"
            else None
        )
    if dest == "layer":
        if rec.get("label") is None:
            return None
        return {"text": rec.get("text") or rec.get("prompt") or record_text(rec), "label": rec["label"], **prov}
    if dest == "eval":
        if "prompt" not in rec or "response" not in rec:
            return None
        ans = rec["response"].strip()
        num = re.findall(r"-?\d+(?:[.,]\d+)?", ans)
        check = (
            {"type": "number", "value": float(num[-1].replace(",", ".")), "tolerance": 0.0}
            if num and len(ans) < 40
            else {"type": "contains", "all": [ans[:60]]}
        )
        return {"id": f"hf{i}", "prompt": rec["prompt"], "checks": [check], "difficulty": 2, "source": f"hf:{source}"}
    raise ValueError(f"onbekende bestemming {dest}")


# ── pijplijn ──────────────────────────────────────────────────────────────────


@dataclass
class ImportCfg:
    source: str
    destination: str
    out: str
    mapping: dict = field(default_factory=dict)
    kind: str | None = None
    split: str = "train"
    config: str | None = None
    limit: int | None = 10000
    license: str | None = None
    evals_dir: str | None = None
    existing: list[str] = field(default_factory=list)
    min_chars: int = 20
    max_chars: int = 20000
    near_dup: float = 0.8

    @classmethod
    def from_dict(cls, d: dict) -> ImportCfg:
        known = {k: v for k, v in d.items() if k in cls.__dataclass_fields__}
        return cls(**known)


def inspect(
    source: str, split: str = "train", config: str | None = None, n: int = 50, license_: str | None = None
) -> dict:
    rows = list(stream_rows(source, split, config, n))
    schema = detect_schema(rows)
    return {
        "schema": schema,
        "mapping": propose_mapping(schema),
        "sample": [{k: str(v)[:300] for k, v in r.items()} for r in rows[:3]],
        "license": license_of(source, license_),
        "destinations": [d for d in DESTINATIONS if _fits(schema["kind"], d)],
    }


def _fits(kind: str, dest: str) -> bool:
    return {
        "knowledge": True,
        "sft": kind in ("chat", "qa", "code"),
        "dpo": kind == "preference",
        "layer": True,
        "eval": kind in ("qa", "code"),
    }[dest]


def run_import(c: ImportCfg, progress: Callable[[float, str], None] | None = None) -> dict:
    if c.destination not in DESTINATIONS:
        raise ValueError(f"bestemming moet een van {DESTINATIONS} zijn")
    head = list(stream_rows(c.source, c.split, c.config, 50))
    kind = c.kind or detect_schema(head)["kind"]
    mapping = c.mapping or propose_mapping(detect_schema(head))
    lic = license_of(c.source, c.license)
    q = Quality(c.min_chars, c.max_chars)
    lsh = MinHashLSH(threshold=c.near_dup)
    exact: set[str] = set()
    # Bestaande data (eerdere imports / trainingsets) telt mee voor dedup.
    for ex in c.existing:
        p = Path(ex)
        if p.exists():
            for line in p.open(encoding="utf-8"):
                if line.strip():
                    t = record_text(json.loads(line)) if line.lstrip().startswith("{") else line
                    exact.add(hashlib.sha1(_norm(t).encode()).hexdigest())
                    lsh.query_add(t)
    evidx = eval_index(c.evals_dir)
    stats = {
        "rows": 0,
        "kept": 0,
        "unmapped": 0,
        "low_quality": 0,
        "exact_dup": 0,
        "near_dup": 0,
        "decontaminated": 0,
        "wrong_destination": 0,
    }
    out = Path(c.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256()
    with out.open("w", encoding="utf-8") as f:
        for i, row in enumerate(stream_rows(c.source, c.split, c.config, c.limit)):
            stats["rows"] += 1
            rec = apply_mapping(row, mapping, kind)
            if rec is None:
                stats["unmapped"] += 1
                continue
            text = record_text(rec)
            if not q.ok(text):
                stats["low_quality"] += 1
                continue
            h = hashlib.sha1(_norm(text).encode()).hexdigest()
            if h in exact:
                stats["exact_dup"] += 1
                continue
            exact.add(h)
            if lsh.query_add(text):
                stats["near_dup"] += 1
                continue
            if evidx and _grams8(text) & evidx:
                stats["decontaminated"] += 1
                continue
            d = to_destination(rec, c.destination, kind, c.source, i, lic)
            if d is None:
                stats["wrong_destination"] += 1
                continue
            line = json.dumps(d, ensure_ascii=False)
            digest.update(line.encode())
            f.write(line + "\n")
            stats["kept"] += 1
            if progress and stats["rows"] % 500 == 0:
                progress(min(0.95, stats["rows"] / max(1, c.limit or 100000)), f"{stats['kept']} bewaard")
    return {
        **stats,
        "kind": kind,
        "mapping": mapping,
        "license": lic,
        "output": str(out),
        "version": digest.hexdigest()[:16],
    }
