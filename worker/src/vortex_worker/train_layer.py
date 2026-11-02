"""Training van een neurale layer (W11.03): JSONL-dataset (x, y, w) → MLP → ONNX.

Gebruikt PyTorch als dat geïnstalleerd is (GPU als die er is); anders een numpy-trainer (zelfde
architectuur: invoernormalisatie → Linear → ReLU → Linear → sigmoid/softmax, Adam, gewogen
verlies, early stopping). Export bouwt de ONNX-graaf zelf (onnx.helper), zodat de Rust-runtime
hem direct kan laden: Sub/Div (normalisatie), Gemm (transB=1), Relu, Sigmoid/Softmax.
"""

# VX:W11.03
from __future__ import annotations

import argparse
import json
import math
import random
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np


@dataclass
class Result:
    weights: list[tuple[np.ndarray, np.ndarray]]
    mean: np.ndarray
    std: np.ndarray
    metrics: dict[str, float] = field(default_factory=dict)
    trainer: str = "numpy"


def load(path: str | Path) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    xs, ys, ws = [], [], []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        r = json.loads(line)
        xs.append(r["x"])
        ys.append(r["y"])
        ws.append(r.get("w", 1.0))
    if not xs:
        return np.zeros((0, 0), np.float32), np.zeros(0, np.float32), np.zeros(0, np.float32)
    return np.asarray(xs, np.float32), np.asarray(ys, np.float32), np.asarray(ws, np.float32)


def _forward(params, x, classes):
    w1, b1, w2, b2 = params
    h = np.maximum(0.0, x @ w1.T + b1)
    z = h @ w2.T + b2
    if classes == 1:
        p = 1.0 / (1.0 + np.exp(-np.clip(z, -30, 30)))
    else:
        z = z - z.max(axis=1, keepdims=True)
        e = np.exp(z)
        p = e / e.sum(axis=1, keepdims=True)
    return h, p


def _loss_acc(params, x, y, w, classes):
    _, p = _forward(params, x, classes)
    if classes == 1:
        q = np.clip(p[:, 0], 1e-7, 1 - 1e-7)
        loss = -(w * (y * np.log(q) + (1 - y) * np.log(1 - q))).sum() / max(w.sum(), 1e-9)
        acc = float(((q >= 0.5) == (y >= 0.5)).mean()) if len(y) else 0.0
    else:
        idx = y.astype(int)
        q = np.clip(p[np.arange(len(y)), idx], 1e-7, 1.0)
        loss = -(w * np.log(q)).sum() / max(w.sum(), 1e-9)
        acc = float((p.argmax(axis=1) == idx).mean()) if len(y) else 0.0
    return float(loss), acc


def train_numpy(x, y, w, classes: int, hidden: int, epochs: int = 300, lr: float = 0.01, seed: int = 7) -> Result:
    rng = np.random.default_rng(seed)
    mean = x.mean(axis=0)
    std = x.std(axis=0)
    std[std < 1e-6] = 1.0
    xn = (x - mean) / std
    # Gewogen verlies: klassen in balans + voorbeeldgewicht.
    if classes == 1:
        pos = max(float((y >= 0.5).sum()), 1.0)
        neg = max(float((y < 0.5).sum()), 1.0)
        cw = np.where(y >= 0.5, len(y) / (2 * pos), len(y) / (2 * neg))
    else:
        counts = np.bincount(y.astype(int), minlength=classes).astype(np.float32)
        cw = len(y) / (classes * np.maximum(counts[y.astype(int)], 1.0))
    wt = (w * cw).astype(np.float32)
    # Validatiesplit voor early stopping.
    order = rng.permutation(len(y))
    nval = max(1, len(y) // 6) if len(y) >= 12 else 0
    vi, ti = order[:nval], order[nval:]
    d = x.shape[1]
    w1 = rng.normal(0, math.sqrt(2.0 / d), (hidden, d)).astype(np.float32)
    b1 = np.zeros(hidden, np.float32)
    w2 = rng.normal(0, math.sqrt(1.0 / hidden), (classes, hidden)).astype(np.float32)
    b2 = np.zeros(classes, np.float32)
    params = [w1, b1, w2, b2]
    m = [np.zeros_like(p) for p in params]
    v = [np.zeros_like(p) for p in params]
    best, best_params, patience = float("inf"), [p.copy() for p in params], 0
    xt, yt, wtt = xn[ti], y[ti], wt[ti]
    for ep in range(1, epochs + 1):
        h, p = _forward(params, xt, classes)
        if classes == 1:
            g = (p[:, 0] - yt)[:, None] * wtt[:, None]
        else:
            onehot = np.zeros_like(p)
            onehot[np.arange(len(yt)), yt.astype(int)] = 1.0
            g = (p - onehot) * wtt[:, None]
        g /= max(wtt.sum(), 1e-9)
        gw2 = g.T @ h
        gb2 = g.sum(axis=0)
        gh = (g @ params[2]) * (h > 0)
        gw1 = gh.T @ xt
        gb1 = gh.sum(axis=0)
        grads = [gw1 + 1e-4 * params[0], gb1, gw2 + 1e-4 * params[2], gb2]
        for i in range(4):
            m[i] = 0.9 * m[i] + 0.1 * grads[i]
            v[i] = 0.999 * v[i] + 0.001 * grads[i] ** 2
            mh = m[i] / (1 - 0.9**ep)
            vh = v[i] / (1 - 0.999**ep)
            params[i] = (params[i] - lr * mh / (np.sqrt(vh) + 1e-8)).astype(np.float32)
        if nval:
            vl, _ = _loss_acc(params, xn[vi], y[vi], wt[vi], classes)
            if vl < best - 1e-5:
                best, best_params, patience = vl, [p.copy() for p in params], 0
            else:
                patience += 1
                if patience >= 30:
                    break
        else:
            best_params = [p.copy() for p in params]
    params = best_params
    tl, ta = _loss_acc(params, xn, y, np.ones_like(y), classes)
    return Result(
        [(params[0], params[1]), (params[2], params[3])],
        mean,
        std,
        {"train_loss": tl, "train_acc": ta, "epochs": float(ep)},
        "numpy",
    )


def train_torch(x, y, w, classes: int, hidden: int, epochs: int = 300) -> Result:  # pragma: no cover - alleen met torch
    import torch  # type: ignore

    dev = "cuda" if torch.cuda.is_available() else "cpu"
    mean = x.mean(axis=0)
    std = x.std(axis=0)
    std[std < 1e-6] = 1.0
    xt = torch.tensor((x - mean) / std, device=dev)
    yt = torch.tensor(y, device=dev)
    wt = torch.tensor(w, device=dev)
    net = torch.nn.Sequential(
        torch.nn.Linear(x.shape[1], hidden), torch.nn.ReLU(), torch.nn.Linear(hidden, classes)
    ).to(dev)
    opt = torch.optim.Adam(net.parameters(), lr=0.01, weight_decay=1e-4)
    for _ in range(epochs):
        opt.zero_grad()
        z = net(xt)
        if classes == 1:
            loss = (torch.nn.functional.binary_cross_entropy_with_logits(z[:, 0], yt, reduction="none") * wt).mean()
        else:
            loss = (torch.nn.functional.cross_entropy(z, yt.long(), reduction="none") * wt).mean()
        loss.backward()
        opt.step()
    l1, l2 = net[0], net[2]
    ws = [
        (l1.weight.detach().cpu().numpy(), l1.bias.detach().cpu().numpy()),
        (l2.weight.detach().cpu().numpy(), l2.bias.detach().cpu().numpy()),
    ]
    params = [ws[0][0], ws[0][1], ws[1][0], ws[1][1]]
    tl, ta = _loss_acc(params, (x - mean) / std, y, np.ones_like(y), classes)
    return Result(ws, mean, std, {"train_loss": tl, "train_acc": ta}, f"torch-{dev}")


def export_onnx(r: Result, classes: int, path: str | Path, meta: dict[str, str]) -> None:
    import onnx
    from onnx import TensorProto, helper, numpy_helper

    init = [
        numpy_helper.from_array(r.mean.astype(np.float32), "mean"),
        numpy_helper.from_array(r.std.astype(np.float32), "std"),
    ]
    nodes = [helper.make_node("Sub", ["x", "mean"], ["xc"]), helper.make_node("Div", ["xc", "std"], ["xn"])]
    cur = "xn"
    for i, (w, b) in enumerate(r.weights):
        init += [
            numpy_helper.from_array(w.astype(np.float32), f"w{i}"),
            numpy_helper.from_array(b.astype(np.float32), f"b{i}"),
        ]
        nodes.append(helper.make_node("Gemm", [cur, f"w{i}", f"b{i}"], [f"h{i}"], transB=1))
        cur = f"h{i}"
        if i + 1 < len(r.weights):
            nodes.append(helper.make_node("Relu", [cur], [f"a{i}"]))
            cur = f"a{i}"
    nodes.append(
        helper.make_node(
            "Sigmoid" if classes == 1 else "Softmax", [cur], ["y"], **({} if classes == 1 else {"axis": 1})
        )
    )
    d = int(r.mean.shape[0])
    graph = helper.make_graph(
        nodes,
        "layer",
        [helper.make_tensor_value_info("x", TensorProto.FLOAT, ["n", d])],
        [helper.make_tensor_value_info("y", TensorProto.FLOAT, ["n", classes])],
        init,
    )
    model = helper.make_model(graph, producer_name="vortex-worker", opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 8
    for k, v in {**meta, "input_dim": str(d), "trainer": r.trainer}.items():
        e = model.metadata_props.add()
        e.key, e.value = k, str(v)
    onnx.checker.check_model(model)
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    onnx.save(model, str(path))


def train(
    train_path: str,
    out: str,
    classes: int,
    hidden: int,
    layer: str,
    test_path: str | None = None,
    prefer_torch: bool = True,
) -> dict:
    x, y, w = load(train_path)
    if len(y) < 4:
        raise ValueError(f"te weinig voorbeelden ({len(y)})")
    random.seed(7)
    r = None
    if prefer_torch:
        try:
            import torch  # type: ignore  # noqa: F401

            r = train_torch(x, y, w, classes, hidden)
        except ImportError:
            r = None
    if r is None:
        r = train_numpy(x, y, w, classes, hidden)
    if test_path:
        xt, yt, _ = load(test_path)
        if len(yt):
            params = [r.weights[0][0], r.weights[0][1], r.weights[1][0], r.weights[1][1]]
            _, acc = _loss_acc(params, (xt - r.mean) / r.std, yt, np.ones_like(yt), classes)
            r.metrics["test_acc"] = acc
    export_onnx(r, classes, out, {"layer": layer, "classes": str(classes)})
    return {"onnx": out, "trainer": r.trainer, "n": len(y), **r.metrics}


def main() -> None:
    p = argparse.ArgumentParser(description="Train een Vortex-layer (MLP → ONNX)")
    p.add_argument("--layer", required=True)
    p.add_argument("--train", required=True)
    p.add_argument("--test")
    p.add_argument("--out", required=True)
    p.add_argument("--classes", type=int, default=1, help="1 = binair (sigmoid), >1 = softmax")
    p.add_argument("--hidden", type=int, default=16)
    p.add_argument("--numpy", action="store_true", help="forceer de numpy-trainer")
    a = p.parse_args()
    print(json.dumps(train(a.train, a.out, a.classes, a.hidden, a.layer, a.test, prefer_torch=not a.numpy)))


if __name__ == "__main__":
    main()
