#!/usr/bin/env python3
"""Dwingt de afhankelijkheidsrichting tussen crates af (VAULT/20-CODEMAP.md). Alleen stdlib.

Een crate mag alleen crates uit een LAGERE laag importeren. Draai: python tools/check_deps.py
"""

# VX:W00.10
from __future__ import annotations

import json
import pathlib
import subprocess
import sys

LAYERS = {
    "vortex-types": 0,
    "vortex-proto": 0,
    "vortex-core": 1,
    "vortex-models": 2,
    "vortex-memory": 2,
    "vortex-tools": 2,
    "vortex-neural": 2,
    "vortex-crawler": 2,
    "vortex-context": 3,
    "vortex-cognition": 4,
    "vortex-agents": 5,
    "vortex-eval": 6,
    "vortex-server": 7,
}


def main() -> int:
    core = pathlib.Path(__file__).resolve().parents[1] / "core"
    meta = json.loads(
        subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=core)
    )
    errors = []
    for pkg in meta["packages"]:
        name = pkg["name"]
        if name not in LAYERS:
            errors.append(f"{name}: staat niet in LAYERS (voeg toe aan tools/check_deps.py en 20-CODEMAP)")
            continue
        for dep in pkg["dependencies"]:
            d = dep["name"]
            if d in LAYERS and dep.get("kind") in (None, "normal", "build") and LAYERS[d] >= LAYERS[name]:
                if d == name:
                    continue
                errors.append(f"{name} (laag {LAYERS[name]}) → {d} (laag {LAYERS[d]}): verkeerde richting")
    for e in errors:
        print("FOUT:", e)
    print("afhankelijkheden ok" if not errors else f"{len(errors)} fout(en)")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
