"""Multimodaal (W16) in de worker.

- Documenten: PDF-lay-out (blokken met coördinaten, leesvolgorde, tabellen), DOCX met tabellen,
  spreadsheets (XLSX/CSV) als tabellen met structuurbehoud en cel-locators.
- Regio's met coördinaten (visuele documentretrieval) + optioneel ColPali-pagina-embeddings.
- OCR met onzekerheid per regel (PaddleOCR → Tesseract), pagina's renderen (PNG).
- Spraak: STT (faster-whisper, CPU, woord-tijden) en TTS (Piper/Kokoro), per zin.
- Beeldgeneratie/-bewerking (diffusers) — draait pas nadat de kern het LLM heeft ontladen.
- Video: adaptieve frameselectie (scènewissels via ffmpeg + min/max-interval) + transcript,
  uitgelijnd op tijd.

Zware engines zijn optioneel en worden lui geïmporteerd. `engine = "test"` is een deterministisch
test-dubbel (zelfde contract) zodat de pijplijn overal getest kan worden.
"""
# VX:W16.03

from __future__ import annotations

import csv
import json
import math
import re
import shutil
import struct
import subprocess
import wave
from pathlib import Path

# ── documenten ────────────────────────────────────────────────────────────────


def _md_table(rows: list[list[str]]) -> str:
    rows = [
        [(c or "").replace("|", "\\|").replace("\n", " ").strip() for c in r]
        for r in rows
        if any((c or "").strip() for c in r)
    ]
    if not rows:
        return ""
    n = max(len(r) for r in rows)
    rows = [r + [""] * (n - len(r)) for r in rows]
    head, body = rows[0], rows[1:]
    out = ["| " + " | ".join(head) + " |", "|" + "---|" * n]
    out += ["| " + " | ".join(r) + " |" for r in body]
    return "\n".join(out)


def _col(n: int) -> str:
    s = ""
    while n > 0:
        n, r = divmod(n - 1, 26)
        s = chr(65 + r) + s
    return s


def pdf_regions(path: str | Path) -> list[dict]:
    """Per pagina: tekstblokken + tabellen met bbox (punten), in leesvolgorde (kolommen links→rechts)."""
    import fitz  # type: ignore

    doc = fitz.open(str(path))
    out: list[dict] = []
    for pno, page in enumerate(doc, start=1):
        w = page.rect.width
        tables = []
        try:
            for t in page.find_tables().tables:
                rows = [[str(c) if c is not None else "" for c in r] for r in t.extract()]
                tables.append((fitz.Rect(t.bbox), rows))
        except Exception:
            tables = []
        blocks = []
        for b in page.get_text("blocks"):
            x0, y0, x1, y1, text = b[0], b[1], b[2], b[3], b[4]
            if not str(text).strip() or (len(b) > 6 and b[6] != 0):
                continue
            r = fitz.Rect(x0, y0, x1, y1)
            if any(r.intersects(tr) and (r & tr).get_area() > 0.5 * r.get_area() for tr, _ in tables):
                continue  # tekst die al in een tabel zit
            blocks.append(
                {
                    "page": pno,
                    "bbox": [round(x0, 1), round(y0, 1), round(x1, 1), round(y1, 1)],
                    "kind": "text",
                    "text": " ".join(str(text).split()),
                }
            )
        for i, (tr, rows) in enumerate(tables, start=1):
            blocks.append(
                {
                    "page": pno,
                    "bbox": [round(tr.x0, 1), round(tr.y0, 1), round(tr.x1, 1), round(tr.y1, 1)],
                    "kind": "table",
                    "text": _md_table(rows),
                    "table": i,
                }
            )
        # Leesvolgorde: kolom (links/rechts van het midden bij brede pagina's), dan y.
        blocks.sort(
            key=lambda b: (0 if b["bbox"][2] <= w * 0.55 or b["bbox"][0] < w * 0.45 else 1, b["bbox"][1], b["bbox"][0])
        )
        n_chars = sum(len(b["text"]) for b in blocks)
        for b in blocks:
            b["page_size"] = [round(page.rect.width, 1), round(page.rect.height, 1)]
            b["locator"] = f"p. {pno} [{', '.join(str(int(v)) for v in b['bbox'])}]"
        if n_chars < 30:
            out.append(
                {
                    "page": pno,
                    "bbox": [0, 0, round(page.rect.width, 1), round(page.rect.height, 1)],
                    "kind": "scan",
                    "text": "",
                    "locator": f"p. {pno}",
                    "page_size": [round(page.rect.width, 1), round(page.rect.height, 1)],
                }
            )
        out.extend(blocks)
    doc.close()
    return out


def docx_blocks(path: str | Path) -> list[dict]:
    """DOCX in documentvolgorde: alinea's (met kopniveau) en tabellen (markdown)."""
    import docx  # type: ignore  (python-docx)
    from docx.table import Table  # type: ignore
    from docx.text.paragraph import Paragraph  # type: ignore

    d = docx.Document(str(path))
    out: list[dict] = []
    para_no = table_no = 0
    for child in d.element.body.iterchildren():
        tag = child.tag.split("}")[-1]
        if tag == "p":
            para_no += 1
            p = Paragraph(child, d)
            text = p.text.strip()
            if not text:
                continue
            m = re.match(r"(?i)(heading|kop)\s*(\d)", p.style.name if p.style is not None else "")
            out.append(
                {
                    "kind": "heading" if m else "text",
                    "level": int(m.group(2)) if m else 0,
                    "text": text,
                    "locator": f"alinea {para_no}",
                }
            )
        elif tag == "tbl":
            table_no += 1
            t = Table(child, d)
            rows = [[c.text for c in r.cells] for r in t.rows]
            out.append({"kind": "table", "text": _md_table(rows), "locator": f"tabel {table_no}", "rows": len(rows)})
    return out


def sheet_blocks(path: str | Path, chunk_rows: int = 50) -> list[dict]:
    """XLSX/CSV → tabellen per blad, in brokken met herhaalde kop; locator = blad!A1:D20."""
    p = Path(path)
    sheets: list[tuple[str, list[list[str]]]] = []
    if p.suffix.lower() in (".csv", ".tsv"):
        delim = "\t" if p.suffix.lower() == ".tsv" else ","
        with p.open(encoding="utf-8", newline="") as f:
            sheets.append((p.stem, [list(r) for r in csv.reader(f, delimiter=delim)]))
    else:
        import openpyxl  # type: ignore

        wb = openpyxl.load_workbook(str(p), data_only=True, read_only=True)
        for ws in wb.worksheets:
            rows = [["" if v is None else str(v) for v in r] for r in ws.iter_rows(values_only=True)]
            sheets.append((ws.title, rows))
    out: list[dict] = []
    for name, rows in sheets:
        rows = [r for r in rows if any(c.strip() for c in r)]
        if not rows:
            continue
        head, body = rows[0], rows[1:]
        ncol = max(len(r) for r in rows)
        for i in range(0, max(1, len(body)), chunk_rows):
            part = body[i : i + chunk_rows]
            r0, r1 = i + 2, i + 1 + max(1, len(part))
            out.append(
                {
                    "kind": "table",
                    "sheet": name,
                    "text": _md_table([head, *part]),
                    "locator": f"{name}!A{r0 - 1 if i == 0 else r0}:{_col(ncol)}{r1}",
                    "rows": len(part),
                    "columns": head,
                }
            )
    return out


def render_page(pdf: str | Path, page: int, out: str | Path, zoom: float = 2.0) -> dict:
    import fitz  # type: ignore

    doc = fitz.open(str(pdf))
    pg = doc[page - 1]
    pix = pg.get_pixmap(matrix=fitz.Matrix(zoom, zoom))
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    pix.save(str(out))
    r = {
        "path": str(out),
        "width": pix.width,
        "height": pix.height,
        "zoom": zoom,
        "page_size": [pg.rect.width, pg.rect.height],
    }
    doc.close()
    return r


# VX:W16.02
# ── OCR ───────────────────────────────────────────────────────────────────────


def ocr(image: str | Path, engine: str = "auto", lang: str = "nl", low: float = 0.6) -> dict:
    """Regels met tekst, bbox en zekerheid (0..1). `uncertain` = regels onder `low`."""
    lines: list[dict] = []
    used = None
    if engine in ("auto", "paddle"):
        try:
            from paddleocr import PaddleOCR  # type: ignore

            res = PaddleOCR(lang="en" if lang not in ("ch", "en") else lang, use_angle_cls=True, show_log=False).ocr(
                str(image), cls=True
            )
            for page in res or []:
                for box, (text, conf) in page or []:
                    xs, ys = [p[0] for p in box], [p[1] for p in box]
                    lines.append({"text": text, "conf": float(conf), "bbox": [min(xs), min(ys), max(xs), max(ys)]})
            used = "paddleocr"
        except ImportError:
            if engine == "paddle":
                raise
    if used is None and engine in ("auto", "tesseract"):
        try:
            import pytesseract  # type: ignore
            from PIL import Image

            d = pytesseract.image_to_data(
                Image.open(image), lang={"nl": "nld", "en": "eng"}.get(lang, lang), output_type=pytesseract.Output.DICT
            )
            cur: dict[tuple, dict] = {}
            for i, w in enumerate(d["text"]):
                if not str(w).strip() or float(d["conf"][i]) < 0:
                    continue
                k = (d["block_num"][i], d["par_num"][i], d["line_num"][i])
                x, y, ww, hh = d["left"][i], d["top"][i], d["width"][i], d["height"][i]
                e = cur.setdefault(k, {"text": [], "conf": [], "bbox": [x, y, x + ww, y + hh]})
                e["text"].append(w)
                e["conf"].append(float(d["conf"][i]) / 100)
                e["bbox"] = [
                    min(e["bbox"][0], x),
                    min(e["bbox"][1], y),
                    max(e["bbox"][2], x + ww),
                    max(e["bbox"][3], y + hh),
                ]
            lines = [
                {"text": " ".join(e["text"]), "conf": sum(e["conf"]) / len(e["conf"]), "bbox": e["bbox"]}
                for e in cur.values()
            ]
            used = "tesseract"
        except (ImportError, OSError):
            if engine == "tesseract":
                raise
    if used is None and engine == "test":
        # Test-dubbel: tekst uit een zij-bestand `<afbeelding>.ocr.json` [{"text", "conf"}].
        side = Path(str(image) + ".ocr.json")
        data = json.loads(side.read_text(encoding="utf-8")) if side.exists() else []
        lines = [
            {"text": x["text"], "conf": float(x.get("conf", 0.9)), "bbox": x.get("bbox", [0, 30 * i, 100, 30 * i + 20])}
            for i, x in enumerate(data)
        ]
        used = "test"
    if used is None:
        raise RuntimeError("geen OCR-engine: installeer de extra 'media' (paddleocr of pytesseract + Tesseract)")
    mean = sum(x["conf"] for x in lines) / len(lines) if lines else 0.0
    return {
        "engine": used,
        "lines": lines,
        "text": "\n".join(x["text"] for x in lines),
        "mean_conf": round(mean, 3),
        "uncertain": [x for x in lines if x["conf"] < low],
    }


# VX:W16.05
# ── spraak ────────────────────────────────────────────────────────────────────


def stt(audio: str | Path, engine: str = "auto", model: str = "small", lang: str | None = "nl") -> dict:
    """Transcript met segmenten (start, end, tekst, zekerheid)."""
    segs: list[dict] = []
    used = None
    if engine in ("auto", "faster-whisper"):
        try:
            from faster_whisper import WhisperModel  # type: ignore

            m = WhisperModel(model, device="cpu", compute_type="int8")
            it, _info = m.transcribe(str(audio), language=lang, vad_filter=True, word_timestamps=False)
            for s in it:
                segs.append(
                    {
                        "start": round(s.start, 2),
                        "end": round(s.end, 2),
                        "text": s.text.strip(),
                        "conf": round(math.exp(s.avg_logprob), 3),
                    }
                )
            used = f"faster-whisper:{model}"
        except ImportError:
            if engine == "faster-whisper":
                raise
    if used is None and engine == "test":
        side = Path(str(audio) + ".stt.json")
        segs = json.loads(side.read_text(encoding="utf-8")) if side.exists() else []
        used = "test"
    if used is None:
        raise RuntimeError("geen spraakherkenning: installeer de extra 'media' (faster-whisper)")
    return {
        "engine": used,
        "segments": segs,
        "text": " ".join(s["text"] for s in segs),
        "duration": segs[-1]["end"] if segs else 0.0,
    }


def sentences(text: str) -> list[str]:
    """Zinnen (TTS per zin → onderbreekbaar en snel de eerste klank)."""
    parts = re.split(r"(?<=[.!?…])\s+|\n+", text.strip())
    return [p.strip() for p in parts if p.strip()]


def tts(text: str, out: str | Path, engine: str = "auto", voice: str | None = None) -> dict:
    """Tekst → WAV (mono 22,05 kHz)."""
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    if engine in ("auto", "piper") and shutil.which("piper") and voice:
        subprocess.run(
            ["piper", "--model", voice, "--output_file", str(out)], input=text.encode(), check=True, capture_output=True
        )
        return {"engine": "piper", "path": str(out), "seconds": _wav_seconds(out)}
    if engine in ("auto", "kokoro"):
        try:
            import numpy as np
            import soundfile as sf  # type: ignore
            from kokoro import KPipeline  # type: ignore

            pipe = KPipeline(lang_code="a")
            audio = np.concatenate([a for _, _, a in pipe(text, voice=voice or "af_heart")])
            sf.write(str(out), audio, 24000)
            return {"engine": "kokoro", "path": str(out), "seconds": len(audio) / 24000}
        except ImportError:
            if engine == "kokoro":
                raise
    if engine == "test":
        # Test-dubbel: toon, 60 ms per teken.
        rate, secs = 22050, max(0.2, 0.06 * len(text))
        with wave.open(str(out), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(rate)
            w.writeframes(
                b"".join(
                    struct.pack("<h", int(3000 * math.sin(2 * math.pi * 440 * i / rate)))
                    for i in range(int(rate * secs))
                )
            )
        return {"engine": "test", "path": str(out), "seconds": round(secs, 2)}
    raise RuntimeError("geen TTS: installeer Piper (met stem) of de extra 'media' (kokoro)")


def _wav_seconds(p: Path) -> float:
    with wave.open(str(p)) as w:
        return w.getnframes() / w.getframerate()


# ── beeld ─────────────────────────────────────────────────────────────────────


def imagegen(
    prompt: str,
    out: str | Path,
    engine: str = "auto",
    model: str | None = None,
    init_image: str | None = None,
    steps: int = 25,
    width: int = 1024,
    height: int = 1024,
    seed: int | None = None,
) -> dict:
    """Tekst → beeld (of bewerken met `init_image`). Draait op de GPU nadat het LLM ontladen is."""
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    if engine in ("auto", "diffusers"):
        try:
            import torch  # type: ignore
            from diffusers import AutoPipelineForImage2Image, AutoPipelineForText2Image  # type: ignore

            mid = model or "stabilityai/sdxl-turbo"
            g = torch.Generator("cuda").manual_seed(seed) if seed is not None else None
            if init_image:
                from PIL import Image

                pipe = AutoPipelineForImage2Image.from_pretrained(mid, torch_dtype=torch.float16).to("cuda")
                img = pipe(
                    prompt, image=Image.open(init_image).convert("RGB"), num_inference_steps=steps, generator=g
                ).images[0]
            else:
                pipe = AutoPipelineForText2Image.from_pretrained(mid, torch_dtype=torch.float16).to("cuda")
                img = pipe(prompt, num_inference_steps=steps, width=width, height=height, generator=g).images[0]
            img.save(out)
            del pipe
            torch.cuda.empty_cache()
            return {"engine": f"diffusers:{mid}", "path": str(out), "width": img.width, "height": img.height}
        except ImportError:
            if engine == "diffusers":
                raise
    if engine == "test":
        from PIL import Image, ImageDraw

        h = abs(hash(prompt))
        if init_image:
            img = Image.open(init_image).convert("RGB").resize((256, 256))
        else:
            img = Image.new("RGB", (256, 256), ((h >> 16) & 255, (h >> 8) & 255, h & 255))
        ImageDraw.Draw(img).text((8, 8), prompt[:40], fill=(255, 255, 255))
        img.save(out)
        return {"engine": "test", "path": str(out), "width": 256, "height": 256}
    raise RuntimeError("geen beeldgenerator: installeer de extra 'media-gpu' (diffusers + PyTorch)")


# VX:W16.07
# ── video ─────────────────────────────────────────────────────────────────────


def _probe_duration(path: Path) -> float:
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1", str(path)],
        capture_output=True,
        text=True,
        check=True,
    )
    return float(r.stdout.strip() or 0)


def video(
    path: str | Path,
    out_dir: str | Path,
    scene: float = 0.3,
    min_gap: float = 2.0,
    max_gap: float = 30.0,
    max_frames: int = 40,
    stt_engine: str = "auto",
) -> dict:
    """Adaptieve frames (scènewissel + vangnet elke `max_gap` s) en transcript, uitgelijnd op tijd."""
    path, out_dir = Path(path), Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    dur = _probe_duration(path)
    # Scènewissels (tijden) via ffmpeg showinfo.
    r = subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-i",
            str(path),
            "-vf",
            f"select='gt(scene,{scene})',showinfo",
            "-an",
            "-f",
            "null",
            "-",
        ],
        capture_output=True,
        text=True,
    )
    cuts = [float(m) for m in re.findall(r"pts_time:([0-9.]+)", r.stderr)]
    times: list[float] = [0.0]
    for t in sorted(cuts):
        if t - times[-1] >= min_gap:
            times.append(t)
    # Vangnet: geen gat groter dan max_gap.
    filled: list[float] = []
    for a, b in zip(times, [*times[1:], dur], strict=False):
        filled.append(a)
        t = a + max_gap
        while t < b - min_gap / 2:
            filled.append(round(t, 2))
            t += max_gap
    if len(filled) > max_frames:  # gelijkmatig uitdunnen
        step = len(filled) / max_frames
        filled = [filled[int(i * step)] for i in range(max_frames)]
    frames = []
    for i, t in enumerate(filled):
        fp = out_dir / f"frame_{i:03d}_{t:07.2f}.jpg"
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-ss",
                f"{t:.2f}",
                "-i",
                str(path),
                "-frames:v",
                "1",
                "-q:v",
                "3",
                str(fp),
            ],
            check=True,
        )
        frames.append(
            {
                "t": round(t, 2),
                "path": str(fp),
                "scene_cut": t in cuts or (i > 0 and any(abs(t - c) < 0.05 for c in cuts)),
            }
        )
    # Audio → transcript (als er een audiospoor is).
    audio = out_dir / "audio.wav"
    has_audio = (
        subprocess.run(
            [
                "ffprobe",
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
                str(path),
            ],
            capture_output=True,
            text=True,
        ).stdout.strip()
        != ""
    )
    transcript: dict = {"segments": [], "text": "", "engine": None}
    if has_audio:
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(path),
                "-vn",
                "-ac",
                "1",
                "-ar",
                "16000",
                str(audio),
            ],
            check=True,
        )
        if stt_engine == "test":
            side = Path(str(path) + ".stt.json")
            if side.exists():
                shutil.copy(side, str(audio) + ".stt.json")
        try:
            transcript = stt(audio, stt_engine)
        except RuntimeError as e:
            transcript = {"segments": [], "text": "", "engine": None, "error": str(e)}
    # Uitlijnen: elk frame krijgt de spraak tussen dit en het volgende frame.
    bounds = [f["t"] for f in frames] + [dur]
    for i, f in enumerate(frames):
        a, b = bounds[i], bounds[i + 1]
        f["end"] = round(b, 2)
        f["speech"] = " ".join(s["text"] for s in transcript.get("segments", []) if s["end"] > a and s["start"] < b)
    return {
        "duration": round(dur, 2),
        "frames": frames,
        "scene_cuts": [round(c, 2) for c in cuts],
        "transcript": transcript,
        "has_audio": has_audio,
    }


# ── visuele retrieval (ColPali) ───────────────────────────────────────────────


def colpali_index(pdf: str | Path, out: str | Path, model: str = "vidore/colqwen2-v1.0") -> dict:
    """Pagina-embeddings (multi-vector) voor visuele documentretrieval. Alleen met GPU + extra."""
    import fitz  # type: ignore
    import torch  # type: ignore
    from colpali_engine.models import ColQwen2, ColQwen2Processor  # type: ignore
    from PIL import Image

    m = ColQwen2.from_pretrained(model, torch_dtype=torch.bfloat16, device_map="cuda").eval()
    proc = ColQwen2Processor.from_pretrained(model)
    doc = fitz.open(str(pdf))
    imgs = []
    for pg in doc:
        pix = pg.get_pixmap(matrix=fitz.Matrix(1.5, 1.5))
        imgs.append(Image.frombytes("RGB", (pix.width, pix.height), pix.samples))
    embs = []
    with torch.no_grad():
        for i in range(0, len(imgs), 4):
            embs.extend(m(**proc.process_images(imgs[i : i + 4]).to(m.device)).cpu())
    torch.save(embs, str(out))
    return {"pages": len(embs), "path": str(out), "model": model}


def colpali_query(index: str | Path, query: str, k: int = 5, model: str = "vidore/colqwen2-v1.0") -> list[dict]:
    import torch  # type: ignore
    from colpali_engine.models import ColQwen2, ColQwen2Processor  # type: ignore

    m = ColQwen2.from_pretrained(model, torch_dtype=torch.bfloat16, device_map="cuda").eval()
    proc = ColQwen2Processor.from_pretrained(model)
    embs = torch.load(str(index))
    with torch.no_grad():
        q = m(**proc.process_queries([query]).to(m.device)).cpu()
    scores = proc.score_multi_vector(q, embs)[0]
    top = torch.topk(scores, min(k, len(embs)))
    return [{"page": int(i) + 1, "score": float(s)} for s, i in zip(top.values, top.indices, strict=True)]


# ── job-ingang (gRPC Train `media:<op>`) ──────────────────────────────────────


def run_job(op: str, cfg: dict) -> dict:
    if op == "regions":
        p = Path(cfg["path"])
        if p.suffix.lower() == ".pdf":
            return {"regions": pdf_regions(p)}
        if p.suffix.lower() == ".docx":
            return {"regions": docx_blocks(p)}
        if p.suffix.lower() in (".xlsx", ".xlsm", ".csv", ".tsv"):
            return {"regions": sheet_blocks(p)}
        raise ValueError(f"geen lay-out voor {p.suffix}")
    if op == "render_page":
        return render_page(cfg["path"], int(cfg.get("page", 1)), cfg["out"], float(cfg.get("zoom", 2.0)))
    if op == "ocr":
        return ocr(cfg["path"], cfg.get("engine", "auto"), cfg.get("lang", "nl"), float(cfg.get("low", 0.6)))
    if op == "stt":
        return stt(cfg["path"], cfg.get("engine", "auto"), cfg.get("model", "small"), cfg.get("lang", "nl"))
    if op == "tts":
        parts = sentences(cfg["text"]) if cfg.get("split", True) else [cfg["text"]]
        out = Path(cfg["out"])
        res = [
            tts(s, out.with_name(f"{out.stem}_{i:03d}.wav"), cfg.get("engine", "auto"), cfg.get("voice"))
            for i, s in enumerate(parts)
        ]
        return {"parts": [{**r, "text": t} for r, t in zip(res, parts, strict=True)]}
    if op == "imagegen":
        return imagegen(
            cfg["prompt"],
            cfg["out"],
            cfg.get("engine", "auto"),
            cfg.get("model"),
            cfg.get("init_image"),
            int(cfg.get("steps", 25)),
            int(cfg.get("width", 1024)),
            int(cfg.get("height", 1024)),
            cfg.get("seed"),
        )
    if op == "video":
        return video(
            cfg["path"],
            cfg["out_dir"],
            float(cfg.get("scene", 0.3)),
            float(cfg.get("min_gap", 2.0)),
            float(cfg.get("max_gap", 30.0)),
            int(cfg.get("max_frames", 40)),
            cfg.get("stt_engine", "auto"),
        )
    if op == "colpali_index":
        return colpali_index(cfg["path"], cfg["out"], cfg.get("model", "vidore/colqwen2-v1.0"))
    if op == "colpali_query":
        return {
            "pages": colpali_query(
                cfg["index"], cfg["query"], int(cfg.get("k", 5)), cfg.get("model", "vidore/colqwen2-v1.0")
            )
        }
    raise ValueError(f"onbekende media-actie {op}")
