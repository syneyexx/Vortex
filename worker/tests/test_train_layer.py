"""W11.03: numpy-trainer + ONNX-export; via gRPC Train (layer:<naam>)."""

import json
import random

import grpc
import numpy as np
import pytest

from vortex_worker.gen.vortex.worker.v1 import worker_pb2, worker_pb2_grpc
from vortex_worker.server import build_server
from vortex_worker.train_layer import train


def make(path, n, classes, seed):
    rnd = random.Random(seed)
    rows = []
    for _ in range(n):
        x = [rnd.random() for _ in range(4)]
        # binair: x0+x1 > 1 (een heuristiek op x0 alleen zit er vaak naast); multiclass: banden van x2
        y = (1.0 if x[0] + x[1] > 1.0 else 0.0) if classes == 1 else float(int(x[2] * 3) % 3)
        rows.append({"x": x, "y": y, "h": [0.5], "w": 1.0, "src": str(rnd.random()), "label_source": "outcome"})
    path.write_text("\n".join(json.dumps(r) for r in rows))
    return path


def test_binary_and_multiclass_export(tmp_path):
    onnx = pytest.importorskip("onnx")
    m = train(
        str(make(tmp_path / "b.jsonl", 400, 1, 1)),
        str(tmp_path / "b.onnx"),
        1,
        16,
        "importance",
        str(make(tmp_path / "bt.jsonl", 100, 1, 2)),
        prefer_torch=False,
    )
    assert m["test_acc"] > 0.85, m
    model = onnx.load(str(tmp_path / "b.onnx"))
    ops = [n.op_type for n in model.graph.node]
    assert ops == ["Sub", "Div", "Gemm", "Relu", "Gemm", "Sigmoid"]
    meta = {p.key: p.value for p in model.metadata_props}
    assert meta["layer"] == "importance" and meta["input_dim"] == "4"
    m3 = train(
        str(make(tmp_path / "m.jsonl", 600, 3, 3)),
        str(tmp_path / "m.onnx"),
        3,
        24,
        "router",
        str(make(tmp_path / "mt.jsonl", 150, 3, 4)),
        prefer_torch=False,
    )
    assert m3["test_acc"] > 0.8, m3


def test_grpc_train_layer(tmp_path):
    pytest.importorskip("onnx")
    data = make(tmp_path / "d.jsonl", 200, 1, 5)
    server, port = build_server("127.0.0.1:0")
    server.start()
    try:
        with grpc.insecure_channel(f"127.0.0.1:{port}") as ch:
            stub = worker_pb2_grpc.WorkerServiceStub(ch)
            cfg = f'train = "{data}"\nout = "{tmp_path / "x.onnx"}"\nclasses = 1\nhidden = 8\nnumpy = true\n'
            ups = list(stub.Train(worker_pb2.TrainRequest(job_kind="layer:salience", config_toml=cfg), timeout=60))
            art = next(u.artifact for u in ups if u.WhichOneof("update") == "artifact")
            assert art.path.endswith("x.onnx")
            assert ups[-1].done and ups[-1].metrics["train_acc"] > 0.8
    finally:
        server.stop(0)


def test_numpy_trainer_is_deterministic():
    x = np.random.default_rng(0).random((50, 3)).astype(np.float32)
    y = (x[:, 0] > 0.5).astype(np.float32)
    from vortex_worker.train_layer import train_numpy

    a = train_numpy(x, y, np.ones_like(y), 1, 8)
    b = train_numpy(x, y, np.ones_like(y), 1, 8)
    assert np.allclose(a.weights[0][0], b.weights[0][0])


def test_continual_learning_reduces_forgetting():
    from vortex_worker.train_layer import continual_experiment

    r = continual_experiment()
    v = r["varianten"]
    assert v["naief"]["vergeten"] > 0.1, r
    assert v["replay"]["vergeten"] < v["naief"]["vergeten"], r
    assert v["replay+l2sp"]["vergeten"] < v["naief"]["vergeten"], r
    assert v["replay+l2sp"]["B"] > 0.75, r
