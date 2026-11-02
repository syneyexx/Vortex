"""Visuele UI-controle (W17.16): de gebouwde GUI (app/dist) in headless Chromium openen met
vaste nep-API-antwoorden, per pagina een screenshot maken en vergelijken met de referentie
(tools/visual/ref/*.png). Afwijking boven de drempel → diff-beeld + exitcode 1.

Gebruik (met de worker-venv, die Playwright + Pillow heeft):
    cd app && npm run build
    ../worker/.venv/bin/python ../tools/visual/visual_check.py            # vergelijken
    ../worker/.venv/bin/python ../tools/visual/visual_check.py --update   # referenties vernieuwen
"""
# VX:W17.16

from __future__ import annotations

import argparse
import functools
import http.server
import json
import os
import sys
import threading
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DIST = ROOT / "app" / "dist"
REF = Path(__file__).resolve().parent / "ref"
OUT = Path(__file__).resolve().parent / "out"

# Pagina's: (naam, tab-tekst in de topnav of None = start).
PAGES = [("chat", None), ("research", "Research"), ("memory", "Memory"), ("models", "Models"), ("studio", "Studio"), ("autonomie", "Autonomie")]

FIXTURES = {
    "/api/sessions": [],
    "/api/models": {"models": [], "default": "gpt-oss-20b"},
    "/api/status": {"version": "visual", "subsystems": [], "gpus": [], "gpu_detail": None, "servers": [], "default_model": "gpt-oss-20b", "tok_s": [], "last_seq": 0},
    "/api/autonomy": {"user_idle": True, "external_gpu_load": False, "running": [], "queued": 0, "suggestions": 0, "remote": {"tailscale": False}},
    "/api/research": [],
    "/api/training": {"datasets": [], "adapters": [], "calibrator": None, "config": {"base_model": "", "idle": True, "min_new_examples": 200}},
    "/api/memory/stats": {"memories": 0, "entities": 0, "edges": 0, "open_conflicts": 0, "by_kind": {}},
    "/api/memory": [],
    "/api/media": {"vision": False, "ocr_engine": "auto", "stt_engine": "auto", "tts_engine": "auto", "imagegen_engine": "auto"},
}


def diff_ratio(a, b, out_path: Path, tol: int = 24) -> float:
    from PIL import Image, ImageChops

    a, b = a.convert("RGB"), b.convert("RGB")
    if a.size != b.size:
        return 1.0
    d = ImageChops.difference(a, b).convert("L")
    mask = d.point(lambda v: 255 if v > tol else 0)
    changed = sum(1 for v in mask.getdata() if v)
    if changed:
        overlay = b.copy()
        red = Image.new("RGB", b.size, (255, 0, 0))
        overlay.paste(red, mask=mask)
        overlay.save(out_path)
    return changed / (a.size[0] * a.size[1])


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--update", action="store_true")
    ap.add_argument("--threshold", type=float, default=0.005)
    ap.add_argument("--browser", default=os.environ.get("VORTEX_BROWSER", "/opt/pw-browsers/chromium"))
    a = ap.parse_args()
    if not (DIST / "index.html").exists():
        print("app/dist ontbreekt: eerst `npm run build` in app/", file=sys.stderr)
        return 2
    from PIL import Image
    from playwright.sync_api import sync_playwright

    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(DIST))
    handler.log_message = lambda *x: None  # type: ignore[method-assign]
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    REF.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    failed = []
    with sync_playwright() as p:
        exe = a.browser if Path(a.browser).exists() else None
        b = p.chromium.launch(executable_path=exe) if exe else p.chromium.launch()
        page = b.new_page(viewport={"width": 1440, "height": 900}, reduced_motion="reduce")

        def api(route):
            path = route.request.url.split("://", 1)[1].split("/", 1)[1].split("?")[0]
            body = FIXTURES.get("/" + path, [] if path.endswith("s") else {})
            if os.environ.get("VISUAL_DEBUG"):
                print("API", "/" + path, "fixture" if "/" + path in FIXTURES else "standaard")
            route.fulfill(status=200, content_type="application/json", body=json.dumps(body))

        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.route("**/api/**", api)
        page.route("**/ws**", lambda r: r.abort())
        page.goto(f"http://127.0.0.1:{srv.server_port}/")
        # Animaties (WebGL-vortex, cursors) uit voor stabiele beelden.
        page.add_style_tag(content="canvas{visibility:hidden!important} *{animation:none!important;transition:none!important;caret-color:transparent!important}")
        for name, tab in PAGES:
            if tab:
                btn = page.get_by_role("button", name=tab, exact=True).first
                try:
                    btn.click(timeout=5000)
                except Exception:
                    print(f"{name}: tab niet klikbaar — paginafouten: {errors[-3:]}")
                    failed.append(name)
                    page.goto(f"http://127.0.0.1:{srv.server_port}/")
                    errors.clear()
                    continue
            page.wait_for_timeout(400)
            if errors:
                print(f"{name}: JavaScript-fout(en): {errors}")
                failed.append(name)
                errors.clear()
            shot = OUT / f"{name}.png"
            page.screenshot(path=str(shot))
            ref = REF / f"{name}.png"
            if a.update or not ref.exists():
                shot.replace(ref)
                print(f"{name}: referentie {'vernieuwd' if a.update else 'aangemaakt'}")
                continue
            r = diff_ratio(Image.open(ref), Image.open(shot), OUT / f"{name}.diff.png")
            ok = r <= a.threshold
            print(f"{name}: {r * 100:.2f}% afwijking {'OK' if ok else 'AFWIJKING'}")
            if not ok:
                failed.append(name)
        b.close()
    srv.shutdown()
    if failed:
        print(f"Afwijkingen: {', '.join(failed)} (zie {OUT})")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
