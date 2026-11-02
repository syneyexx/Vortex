"""Computer use (W17.14) op de eigen PC: schermafbeelding (mss), klikken, typen, toetsen,
scrollen (pyautogui). Alleen met een bureaublad; in een server/cloud-omgeving niet beschikbaar.
"""
# VX:W17.14

from __future__ import annotations

from pathlib import Path


def act(op: str, cfg: dict) -> dict:
    if op == "screenshot":
        import mss  # type: ignore
        import mss.tools  # type: ignore

        out = Path(cfg["out"])
        out.parent.mkdir(parents=True, exist_ok=True)
        with mss.mss() as m:
            mon = m.monitors[int(cfg.get("monitor", 1))]
            img = m.grab(mon)
            mss.tools.to_png(img.rgb, img.size, output=str(out))
        return {"path": str(out), "width": img.size[0], "height": img.size[1]}
    import pyautogui  # type: ignore

    pyautogui.FAILSAFE = True  # muis in een hoek = noodstop
    if op == "click":
        pyautogui.click(
            int(cfg["x"]), int(cfg["y"]), clicks=int(cfg.get("clicks", 1)), button=cfg.get("button", "left")
        )
    elif op == "move":
        pyautogui.moveTo(int(cfg["x"]), int(cfg["y"]), duration=float(cfg.get("duration", 0.2)))
    elif op == "type":
        pyautogui.write(cfg["text"], interval=float(cfg.get("interval", 0.01)))
    elif op == "key":
        pyautogui.hotkey(*[k.strip() for k in cfg["keys"].split("+")])
    elif op == "scroll":
        pyautogui.scroll(int(cfg.get("dy", -5)))
    else:
        raise ValueError(f"onbekende actie {op}")
    return {"ok": True, "op": op}
