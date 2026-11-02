"""Browserautomatisering (W17.14) met Playwright (Chromium, headless): een reeks stappen
(goto, click, fill, press, wait, extract, screenshot, eval) en het resultaat per stap.
Browser: `executable` (bv. Chrome/Edge op de PC) of de door Playwright geïnstalleerde Chromium.
"""
# VX:W17.14

from __future__ import annotations

import os
from pathlib import Path


def _find_executable(given: str | None) -> str | None:
    for c in [given, os.environ.get("VORTEX_BROWSER"), "/opt/pw-browsers/chromium"]:
        if c and Path(c).exists():
            return c
    return None


def run(
    steps: list[dict], out_dir: str, executable: str | None = None, headless: bool = True, timeout_ms: int = 20000
) -> dict:
    from playwright.sync_api import sync_playwright  # type: ignore

    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    results: list[dict] = []
    shots: list[str] = []
    with sync_playwright() as p:
        exe = _find_executable(executable)
        browser = (
            p.chromium.launch(headless=headless, executable_path=exe) if exe else p.chromium.launch(headless=headless)
        )
        page = browser.new_page()
        page.set_default_timeout(timeout_ms)
        try:
            for i, st in enumerate(steps):
                a = st.get("action")
                try:
                    if a == "goto":
                        page.goto(st["url"], wait_until=st.get("wait_until", "load"))
                        results.append({"i": i, "action": a, "ok": True, "url": page.url})
                    elif a == "click":
                        page.click(st["selector"])
                        results.append({"i": i, "action": a, "ok": True})
                    elif a == "fill":
                        page.fill(st["selector"], st.get("text", ""))
                        results.append({"i": i, "action": a, "ok": True})
                    elif a == "press":
                        page.keyboard.press(st["key"])
                        results.append({"i": i, "action": a, "ok": True})
                    elif a == "wait":
                        if "selector" in st:
                            page.wait_for_selector(st["selector"])
                        else:
                            page.wait_for_timeout(int(st.get("ms", 500)))
                        results.append({"i": i, "action": a, "ok": True})
                    elif a == "extract":
                        sel = st.get("selector", "body")
                        el = page.query_selector(sel)
                        text = el.inner_text() if el else ""
                        results.append({"i": i, "action": a, "ok": el is not None, "text": text[:20000]})
                    elif a == "screenshot":
                        f = out / f"shot_{i:02d}.png"
                        page.screenshot(path=str(f), full_page=bool(st.get("full_page", False)))
                        shots.append(str(f))
                        results.append({"i": i, "action": a, "ok": True, "path": str(f)})
                    elif a == "eval":
                        results.append({"i": i, "action": a, "ok": True, "value": page.evaluate(st["js"])})
                    else:
                        results.append({"i": i, "action": a, "ok": False, "error": f"onbekende actie {a}"})
                        break
                except Exception as e:
                    results.append({"i": i, "action": a, "ok": False, "error": f"{type(e).__name__}: {e}"})
                    break
            title, url = page.title(), page.url
        finally:
            browser.close()
    return {"results": results, "title": title, "url": url, "screenshots": shots}
