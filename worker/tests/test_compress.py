import numpy as np

from vortex_worker.compress import experiment, int8, mpo_compress, mpo_reconstruct


def test_mpo_exact_at_full_rank_and_smaller_when_truncated():
    rng = np.random.default_rng(0)
    w = rng.normal(size=(24, 12)).astype(np.float32)
    cores, dims = mpo_compress(w, 1000)
    assert np.allclose(mpo_reconstruct(cores, dims), w, atol=1e-4)
    small, _ = mpo_compress(w, 2)
    assert sum(c.size for c in small) < w.size
    assert np.abs(int8(w) - w).max() < np.abs(w).max() / 100


def test_experiment_compares_variants():
    rng = np.random.default_rng(1)
    x = rng.normal(size=(600, 12)).astype(np.float32)
    y = ((x[:, 0] * x[:, 1] + x[:, 2]) > 0).astype(np.float32)
    r = experiment(x[:450], y[:450], x[450:], y[450:], classes=1, hidden=32, chis=(2, 8))
    names = [row["variant"] for row in r["rows"]]
    assert names[:2] == ["fp32", "int8"] and "mpo χ=8 + healing" in names
    by = {row["variant"]: row for row in r["rows"]}
    assert abs(by["int8"]["acc_delta"]) <= 0.03
    assert by["mpo χ=2"]["compression"] > 1.0
    assert by["mpo χ=2 + healing"]["test_acc"] >= by["mpo χ=2"]["test_acc"] - 0.02
    assert "advice" in r
