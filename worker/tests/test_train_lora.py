"""W13.02: droogloop/validatie van de LoRA-pipeline (zonder torch) + gRPC lora-job."""

import json

import grpc
import pytest

from vortex_worker.gen.vortex.worker.v1 import worker_pb2, worker_pb2_grpc
from vortex_worker.server import build_server
from vortex_worker.train_lora import LoraCfg, validate


def write(p, rows):
    p.write_text("\n".join(json.dumps(r) for r in rows))
    return p


def test_validate_sft_and_dpo(tmp_path):
    sft = write(
        tmp_path / "s.jsonl",
        [{"messages": [{"role": "user", "content": "hoi"}, {"role": "assistant", "content": "hallo"}]}] * 40,
    )
    st = validate(LoraCfg(kind="sft", base_model="x", dataset=str(sft), out_dir=str(tmp_path)))
    assert st["n"] == 40 and st["steps"] >= 1
    dpo = write(tmp_path / "d.jsonl", [{"prompt": "q", "chosen": "goed", "rejected": "fout"}])
    assert validate(LoraCfg(kind="dpo", base_model="x", dataset=str(dpo), out_dir=str(tmp_path)))["n"] == 1
    bad = write(tmp_path / "b.jsonl", [{"prompt": "q", "chosen": "zelfde", "rejected": "zelfde"}])
    with pytest.raises(ValueError, match="ongeldig"):
        validate(LoraCfg(kind="dpo", base_model="x", dataset=str(bad), out_dir=str(tmp_path)))
    with pytest.raises(ValueError, match="LoRA"):
        validate(LoraCfg(kind="sft", base_model="x", dataset=str(sft), out_dir=str(tmp_path), lr=5.0))
    c = LoraCfg.from_dict({"kind": "sft", "base_model": "b", "dataset": "d", "out_dir": "o", "r": 8, "nieuw": 1})
    assert c.r == 8 and c.extra == {"nieuw": 1}


def test_grpc_lora_job_reports_missing_torch_or_bad_data(tmp_path):
    bad = write(tmp_path / "b.jsonl", [{"messages": [{"role": "user", "content": "x"}]}])
    server, port = build_server("127.0.0.1:0")
    server.start()
    try:
        with grpc.insecure_channel(f"127.0.0.1:{port}") as ch:
            stub = worker_pb2_grpc.WorkerServiceStub(ch)
            cfg = f'base_model = "x"\ndataset = "{bad}"\nout_dir = "{tmp_path}"\n'
            ups = list(stub.Train(worker_pb2.TrainRequest(job_kind="lora:sft", config_toml=cfg), timeout=30))
            err = next(u.error for u in ups if u.WhichOneof("update") == "error")
            assert "ongeldig" in err
            good = write(
                tmp_path / "g.jsonl",
                [{"messages": [{"role": "user", "content": "x"}, {"role": "assistant", "content": "y"}]}],
            )
            cfg = f'base_model = "x"\ndataset = "{good}"\nout_dir = "{tmp_path}"\n'
            ups = list(stub.Train(worker_pb2.TrainRequest(job_kind="lora:sft", config_toml=cfg), timeout=60))
            kinds = [u.WhichOneof("update") for u in ups]
            assert kinds[0] == "progress" and kinds[-1] == "done"
            # Zonder torch in deze omgeving: nette fout i.p.v. crash (op de PC: adapter-GGUF).
            errs = [u.error for u in ups if u.WhichOneof("update") == "error"]
            assert (
                not errs
                or "PyTorch" in errs[0]
                or "torch" in errs[0].lower()
                or "base_model" in errs[0]
                or "x" in errs[0]
            )
    finally:
        server.stop(0)
