"""Genereer Python-gRPC-code uit ../proto naar src/vortex_worker/gen. Draai: uv run python scripts/gen_proto.py"""

# VX:W00.11
from __future__ import annotations

import pathlib
import re
import shutil
import sys

from grpc_tools import protoc

ROOT = pathlib.Path(__file__).resolve().parents[1]
PROTO = ROOT.parent / "proto"
OUT = ROOT / "src" / "vortex_worker" / "gen"


def main() -> int:
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    files = sorted(str(p.relative_to(PROTO)) for p in PROTO.rglob("*.proto"))
    include = pathlib.Path(protoc.__file__).parent / "_proto"
    args = [
        "protoc",
        f"-I{PROTO}",
        f"-I{include}",
        f"--python_out={OUT}",
        f"--pyi_out={OUT}",
        f"--grpc_python_out={OUT}",
        *files,
    ]
    if protoc.main(args) != 0:
        return 1
    # Pakketten maken + absolute imports omzetten naar het gen-pakket.
    for d in [OUT, *[p for p in OUT.rglob("*") if p.is_dir()]]:
        (d / "__init__.py").touch()
    for f in OUT.rglob("*.py*"):
        text = f.read_text(encoding="utf-8")
        text = re.sub(r"^from vortex\.", "from vortex_worker.gen.vortex.", text, flags=re.M)
        f.write_text(text, encoding="utf-8")
    print(f"gegenereerd: {len(files)} proto-bestanden → {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
