import json
import shutil
import subprocess
import wave

import pytest

from vortex_worker import media


def _pdf_with_table(path):
    import fitz

    doc = fitz.open()
    page = doc.new_page()
    page.insert_text((72, 60), "Kwartaalcijfers", fontsize=18)
    page.insert_text((72, 90), "De omzet steeg in het derde kwartaal door export.", fontsize=11)
    x0, y0, cw, rh = 72, 120, 150, 24
    cells = [["Kwartaal", "Omzet", "Winst"], ["Q1", "100", "10"], ["Q2", "120", "14"], ["Q3", "150", "21"]]
    for r, row in enumerate(cells):
        for c, val in enumerate(row):
            rect = fitz.Rect(x0 + c * cw, y0 + r * rh, x0 + (c + 1) * cw, y0 + (r + 1) * rh)
            page.draw_rect(rect, color=(0, 0, 0), width=0.8)
            page.insert_text((rect.x0 + 4, rect.y1 - 7), val, fontsize=10)
    doc.new_page()  # lege pagina = "scan"
    doc.save(str(path))


def test_pdf_regions_tables_and_coordinates(tmp_path):
    p = tmp_path / "rapport.pdf"
    _pdf_with_table(p)
    regs = media.pdf_regions(p)
    tables = [r for r in regs if r["kind"] == "table"]
    assert tables, regs
    assert "| Kwartaal | Omzet | Winst |" in tables[0]["text"]
    assert "| Q3 | 150 | 21 |" in tables[0]["text"]
    text = [r for r in regs if r["kind"] == "text"]
    assert any("omzet steeg" in r["text"] for r in text)
    assert all(r["locator"].startswith("p. 1 [") for r in text + tables)
    # Tabeltekst niet dubbel als los blok.
    assert not any("Q3" in r["text"] for r in text)
    assert any(r["kind"] == "scan" and r["page"] == 2 for r in regs)
    out = media.render_page(p, 1, tmp_path / "p1.png")
    assert out["width"] > 1000


def test_docx_and_sheets_keep_structure(tmp_path):
    import docx
    import openpyxl

    d = docx.Document()
    d.add_heading("Inleiding", level=1)
    d.add_paragraph("Tekst voor de tabel.")
    t = d.add_table(rows=2, cols=2)
    t.cell(0, 0).text, t.cell(0, 1).text, t.cell(1, 0).text, t.cell(1, 1).text = "Naam", "Rol", "Vortex", "assistent"
    d.add_paragraph("Na de tabel.")
    d.save(str(tmp_path / "a.docx"))
    b = media.docx_blocks(tmp_path / "a.docx")
    assert [x["kind"] for x in b] == ["heading", "text", "table", "text"]
    assert "| Vortex | assistent |" in b[2]["text"]
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Budget"
    ws.append(["Post", "Bedrag"])
    for i in range(120):
        ws.append([f"post {i}", i * 10])
    wb.save(str(tmp_path / "b.xlsx"))
    s = media.sheet_blocks(tmp_path / "b.xlsx")
    assert len(s) == 3 and s[0]["locator"] == "Budget!A1:B51"
    assert all(x["text"].startswith("| Post | Bedrag |") for x in s), "kop herhaald per brok"
    (tmp_path / "c.csv").write_text("a,b\n1,2\n", encoding="utf-8")
    assert media.sheet_blocks(tmp_path / "c.csv")[0]["text"].endswith("| 1 | 2 |")


def test_ocr_uncertainty_tts_imagegen(tmp_path):
    img = tmp_path / "scan.png"
    media.imagegen("rode zonsondergang", img, engine="test")
    assert img.exists()
    (tmp_path / "scan.png.ocr.json").write_text(
        json.dumps([{"text": "Factuur 2026", "conf": 0.97}, {"text": "Bedrag 1?3,00", "conf": 0.41}])
    )
    r = media.ocr(img, engine="test")
    assert r["text"].startswith("Factuur") and len(r["uncertain"]) == 1 and 0.6 < r["mean_conf"] < 0.7
    parts = media.run_job(
        "tts", {"text": "Hallo daar. Hoe gaat het?", "out": str(tmp_path / "s.wav"), "engine": "test"}
    )["parts"]
    assert [p["text"] for p in parts] == ["Hallo daar.", "Hoe gaat het?"]
    with wave.open(parts[0]["path"]) as w:
        assert w.getnframes() > 0
    edit = media.imagegen("zelfde maar blauw", tmp_path / "e.png", engine="test", init_image=str(img))
    assert edit["width"] == 256


@pytest.mark.skipif(not shutil.which("ffmpeg"), reason="ffmpeg ontbreekt")
def test_video_adaptive_frames_aligned_with_transcript(tmp_path):
    v = tmp_path / "clip.mp4"
    subprocess.run(
        [
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
            "-f", "lavfi", "-i", "color=c=red:s=160x120:d=4",
            "-f", "lavfi", "-i", "color=c=blue:s=160x120:d=4",
            "-f", "lavfi", "-i", "sine=frequency=300:duration=8",
            "-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0[v]", "-map", "[v]", "-map", "2:a",
            "-shortest", str(v),
        ],
        check=True,
    )  # fmt: skip
    (tmp_path / "clip.mp4.stt.json").write_text(json.dumps([
        {"start": 0.5, "end": 3.0, "text": "eerst rood", "conf": 0.9},
        {"start": 4.5, "end": 7.5, "text": "nu blauw", "conf": 0.8},
    ]))  # fmt: skip
    r = media.video(v, tmp_path / "frames", scene=0.3, min_gap=1.0, max_gap=30, stt_engine="test")
    ts = [f["t"] for f in r["frames"]]
    assert ts[0] == 0.0 and any(3.5 <= t <= 4.5 for t in ts), r
    assert r["has_audio"]
    first, cut = r["frames"][0], next(f for f in r["frames"] if 3.5 <= f["t"] <= 4.5)
    assert first["speech"] == "eerst rood" and cut["speech"] == "nu blauw"
    # Vangnet-interval zonder scènewissel.
    r2 = media.video(v, tmp_path / "f2", scene=0.99, min_gap=1.0, max_gap=2.0, stt_engine="test")
    assert len(r2["frames"]) >= 4
