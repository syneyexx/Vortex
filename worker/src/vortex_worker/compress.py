"""Experiment (W17.15): quantum-geïnspireerde tensor-netwerk-compressie (MPO, CompactifAI-
achtig) van eigen neurale layers, vergeleken met gewone int8-kwantisatie op kwaliteit x
snelheid x grootte.

MPO: gewichtsmatrix W (mxn) → tensor (m1, m2, n1, n2) → herschikt naar (m1·n1) x (m2·n2) →
afgekapte SVD met bond-dimensie χ = twee MPO-kernen. Daarna korte "healing": de rest van het
net (biases + uitvoerlaag) traint een paar epochs mee met de vaste gecomprimeerde laag.

**Alleen voor eigen neurale layers en hulpmodellen (router/extractie) — NOOIT voor het
redeneermodel** (besluit 2026-11-01).
"""
# VX:W17.15

from __future__ import annotations

import math
import time

import numpy as np

from vortex_worker.train_layer import _forward, _loss_acc, load, train_numpy


def _split(n: int) -> tuple[int, int]:
    a = int(math.sqrt(n))
    while a > 1 and n % a:
        a -= 1
    return a, n // a


def mpo_compress(w: np.ndarray, chi: int) -> tuple[list[np.ndarray], tuple[int, int, int, int]]:
    m, n = w.shape
    m1, m2 = _split(m)
    n1, n2 = _split(n)
    t = w.reshape(m1, m2, n1, n2).transpose(0, 2, 1, 3).reshape(m1 * n1, m2 * n2)
    u, s, vt = np.linalg.svd(t, full_matrices=False)
    k = max(1, min(chi, len(s)))
    a = (u[:, :k] * s[:k]).astype(np.float32)
    b = vt[:k, :].astype(np.float32)
    return [a, b], (m1, m2, n1, n2)


def mpo_reconstruct(cores: list[np.ndarray], dims: tuple[int, int, int, int]) -> np.ndarray:
    m1, m2, n1, n2 = dims
    t = cores[0] @ cores[1]
    return t.reshape(m1, n1, m2, n2).transpose(0, 2, 1, 3).reshape(m1 * m2, n1 * n2)


def int8(w: np.ndarray) -> np.ndarray:
    scale = float(np.abs(w).max()) / 127.0 or 1.0
    return (np.clip(np.round(w / scale), -127, 127) * scale).astype(np.float32)


def _heal(params, xn, y, classes, epochs=40, lr=0.005):
    """Train b1, w2, b2 met vaste (gecomprimeerde) w1."""
    w1, b1, w2, b2 = [p.copy() for p in params]
    m = [np.zeros_like(p) for p in (b1, w2, b2)]
    v = [np.zeros_like(p) for p in (b1, w2, b2)]
    for ep in range(1, epochs + 1):
        h, p = _forward([w1, b1, w2, b2], xn, classes)
        if classes == 1:
            g = (p[:, 0] - y)[:, None]
        else:
            oh = np.zeros_like(p)
            oh[np.arange(len(y)), y.astype(int)] = 1.0
            g = p - oh
        g /= len(y)
        gw2, gb2 = g.T @ h, g.sum(axis=0)
        gb1 = ((g @ w2) * (h > 0)).sum(axis=0)
        for i, (par, gr) in enumerate(((b1, gb1), (w2, gw2), (b2, gb2))):
            m[i] = 0.9 * m[i] + 0.1 * gr
            v[i] = 0.999 * v[i] + 0.001 * gr**2
            par -= lr * (m[i] / (1 - 0.9**ep)) / (np.sqrt(v[i] / (1 - 0.999**ep)) + 1e-8)
    return [w1, b1, w2, b2]


def _speed(params, x, classes, reps=20) -> float:
    t = time.perf_counter()
    for _ in range(reps):
        _forward(params, x, classes)
    return (time.perf_counter() - t) / reps * 1000


def experiment(xtr, ytr, xte, yte, classes: int, hidden: int, chis=(2, 4, 8), seed: int = 7) -> dict:
    r = train_numpy(xtr, ytr, np.ones_like(ytr), classes, hidden, epochs=400, seed=seed)
    (w1, b1), (w2, b2) = r.weights
    xn_tr = (xtr - r.mean) / r.std
    xn_te = (xte - r.mean) / r.std
    base = [w1, b1, w2, b2]
    ones = np.ones_like(yte)
    nparams = w1.size + b1.size + w2.size + b2.size
    rows = []

    def row(name, params, nbytes):
        _, acc = _loss_acc(params, xn_te, yte, ones, classes)
        rows.append(
            {
                "variant": name,
                "test_acc": round(acc, 4),
                "bytes": int(nbytes),
                "ms_forward": round(_speed(params, xn_te, classes), 3),
            }
        )

    row("fp32", base, nparams * 4)
    row("int8", [int8(w1), b1, int8(w2), b2], w1.size + w2.size + (b1.size + b2.size) * 4 + 8)
    for chi in chis:
        cores, dims = mpo_compress(w1, chi)
        w1c = mpo_reconstruct(cores, dims)
        size = (sum(c.size for c in cores) + b1.size + w2.size + b2.size) * 4
        row(f"mpo χ={chi}", [w1c, b1, w2, b2], size)
        row(f"mpo χ={chi} + healing", _heal([w1c, b1, w2, b2], xn_tr, ytr, classes), size)
    ref = rows[0]["test_acc"]
    for x in rows:
        x["acc_delta"] = round(x["test_acc"] - ref, 4)
        x["compression"] = round(rows[0]["bytes"] / max(x["bytes"], 1), 2)
    best = max((x for x in rows[1:] if x["acc_delta"] >= -0.01), key=lambda x: x["compression"], default=None)
    return {
        "hidden": hidden,
        "rows": rows,
        "advice": f"beste zonder kwaliteitsverlies (>-1 pt): {best['variant']} ({best['compression']}x kleiner)"
        if best
        else "geen variant zonder kwaliteitsverlies",
    }


def run(train_path: str, test_path: str, classes: int, hidden: int, chis=(2, 4, 8)) -> dict:
    xtr, ytr, _ = load(train_path)
    xte, yte, _ = load(test_path)
    return experiment(xtr, ytr, xte, yte, classes, hidden, tuple(chis))
