"""gRPC-server van de worker. W00: Health + Cancel; W10: Parse + Ingest; training volgt in W13."""

# VX:W00.12
from __future__ import annotations

import argparse
import json
import logging
import os
import threading
from concurrent import futures
from pathlib import Path

import grpc

from vortex_worker import __version__
from vortex_worker.gen.vortex.common.v1 import common_pb2
from vortex_worker.gen.vortex.worker.v1 import worker_pb2, worker_pb2_grpc
from vortex_worker.ingest import ingest
from vortex_worker.parsers import parse

log = logging.getLogger("vortex.worker")


class CancelRegistry:
    """Per trace een annuleer-vlag; lange taken controleren die op veilige punten."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._flags: dict[str, threading.Event] = {}

    def flag(self, trace_id: str) -> threading.Event:
        with self._lock:
            return self._flags.setdefault(trace_id, threading.Event())

    def cancel(self, trace_id: str) -> bool:
        with self._lock:
            ev = self._flags.get(trace_id)
            if ev is None:
                return False
            ev.set()
            return True

    def forget(self, trace_id: str) -> None:
        with self._lock:
            self._flags.pop(trace_id, None)


class WorkerService(worker_pb2_grpc.WorkerServiceServicer):
    def __init__(self, out_dir: Path | None = None) -> None:
        self.cancels = CancelRegistry()
        # Ruwe/tussenbestanden op de koude schijf (W10.14).
        self.out_dir = out_dir or Path(os.environ.get("VORTEX_COLD_DIR", "data/cold")) / "ingest"

    def GetHealth(self, request, context):
        return common_pb2.Health(ok=True, component="worker", detail="gereed", version=__version__)

    def Cancel(self, request, context):
        return common_pb2.CancelReply(accepted=self.cancels.cancel(request.trace.trace_id))

    def Parse(self, request, context):
        if not Path(request.path).exists():
            context.abort(grpc.StatusCode.NOT_FOUND, f"{request.path} bestaat niet")
        try:
            p = parse(request.path, request.media_type)
        except FileNotFoundError:
            context.abort(grpc.StatusCode.NOT_FOUND, f"{request.path} bestaat niet")
        except Exception as e:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, f"parsen mislukt: {e}")
        return worker_pb2.ParseReply(
            content_hash=p.content_hash,
            sections=[
                worker_pb2.ParsedSection(title=s.title, level=s.level, text=s.text, locator=s.locator)
                for s in p.sections
            ],
            metadata=p.metadata,
        )

    def Ingest(self, request, context):
        trace = request.trace
        flag = self.cancels.flag(trace.trace_id) if trace.trace_id else threading.Event()
        try:
            for u in ingest(list(request.paths), self.out_dir, cancel=flag):
                if u.kind == "progress":
                    yield worker_pb2.IngestUpdate(
                        progress=common_pb2.Progress(trace=trace, fraction=u.fraction, stage=u.stage, message=u.message)
                    )
                elif u.kind == "batch":
                    yield worker_pb2.IngestUpdate(
                        chunk_batch=common_pb2.ArtifactRef(
                            artifact_id=u.content_hash,
                            path=u.path,
                            media_type="application/x-ndjson",
                            size_bytes=u.size,
                        )
                    )
                elif u.kind == "error":
                    yield worker_pb2.IngestUpdate(error=u.message)
                else:
                    yield worker_pb2.IngestUpdate(done=True)
        finally:
            if trace.trace_id:
                self.cancels.forget(trace.trace_id)

    def Train(self, request, context):
        """W11: `layer:<naam>` (MLP → ONNX). W13: `lora:sft` / `lora:dpo` (QLoRA → GGUF-adapter)."""
        import tomllib

        if request.job_kind.startswith("lora:"):
            yield from self._train_lora(request, tomllib.loads(request.config_toml or ""))
            return
        if request.job_kind.startswith("media:"):
            yield from _media_updates(request, tomllib.loads(request.config_toml or ""))
            return
        if request.job_kind.startswith("dataset:"):
            yield from _dataset_updates(request, tomllib.loads(request.config_toml or ""))
            return
        if not request.job_kind.startswith("layer:"):
            context.abort(grpc.StatusCode.UNIMPLEMENTED, f"onbekende trainingssoort {request.job_kind}")

        from vortex_worker.train_layer import train

        cfg = tomllib.loads(request.config_toml or "")
        trace = request.trace
        yield worker_pb2.TrainUpdate(
            progress=common_pb2.Progress(trace=trace, fraction=0.05, stage="trainen", message=request.job_kind)
        )
        try:
            m = train(
                cfg["train"],
                cfg["out"],
                int(cfg.get("classes", 1)),
                int(cfg.get("hidden", 16)),
                request.job_kind.split(":", 1)[1],
                cfg.get("test"),
                prefer_torch=not cfg.get("numpy", False),
            )
        except Exception as e:
            yield worker_pb2.TrainUpdate(error=str(e))
            yield worker_pb2.TrainUpdate(done=True)
            return
        metrics = {k: float(v) for k, v in m.items() if isinstance(v, (int, float))}
        yield worker_pb2.TrainUpdate(
            artifact=common_pb2.ArtifactRef(artifact_id=m["trainer"], path=m["onnx"], media_type="application/onnx"),
            metrics=metrics,
        )
        yield worker_pb2.TrainUpdate(done=True, metrics=metrics)


def _dataset_updates(request, cfg: dict):
    """W14.09: `dataset:inspect` (schema, mapping-voorstel, voorbeeld, licentie → JSON) en
    `dataset:import` (filter, dedup, decontaminatie → JSONL voor de bestemming)."""
    from vortex_worker import hf_datasets as hf

    trace = request.trace
    try:
        if request.job_kind == "dataset:inspect":
            info = hf.inspect(
                cfg["source"], cfg.get("split", "train"), cfg.get("config"), int(cfg.get("n", 50)), cfg.get("license")
            )
            out = Path(cfg["out"])
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(json.dumps(info, ensure_ascii=False), encoding="utf-8")
            yield worker_pb2.TrainUpdate(
                artifact=common_pb2.ArtifactRef(artifact_id="inspect", path=str(out), media_type="application/json")
            )
        elif request.job_kind == "dataset:import":
            c = hf.ImportCfg.from_dict(cfg)

            def prog(f: float, m: str) -> None:
                pass

            r = hf.run_import(c, prog)
            meta = Path(c.out).with_suffix(".meta.json")
            meta.write_text(json.dumps(r, ensure_ascii=False), encoding="utf-8")
            metrics = {k: float(v) for k, v in r.items() if isinstance(v, int | float)}
            yield worker_pb2.TrainUpdate(
                artifact=common_pb2.ArtifactRef(
                    artifact_id=r["version"], path=str(meta), media_type="application/json"
                ),
                metrics=metrics,
            )
        else:
            yield worker_pb2.TrainUpdate(error=f"onbekende datasetactie {request.job_kind}")
    except Exception as e:
        yield worker_pb2.TrainUpdate(error=f"{type(e).__name__}: {e}")
    yield worker_pb2.TrainUpdate(done=True, progress=common_pb2.Progress(trace=trace, fraction=1.0, stage="klaar"))


def _media_updates(request, cfg: dict):
    """W16: `media:<op>` (regions, render_page, ocr, stt, tts, imagegen, video, colpali_*) → JSON-artefact."""
    from vortex_worker import media

    op = request.job_kind.split(":", 1)[1]
    try:
        res = media.run_job(op, cfg)
        out = Path(
            cfg.get("result") or Path(cfg.get("out_dir") or Path(cfg.get("out", "media")).parent) / f"{op}.result.json"
        )
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(res, ensure_ascii=False), encoding="utf-8")
        yield worker_pb2.TrainUpdate(
            artifact=common_pb2.ArtifactRef(artifact_id=op, path=str(out), media_type="application/json")
        )
    except Exception as e:
        yield worker_pb2.TrainUpdate(error=f"{type(e).__name__}: {e}")
    yield worker_pb2.TrainUpdate(done=True)


def _lora_updates(self_, request, cfg: dict):
    """LoRA-training in een thread; voortgang via een queue; annuleren via de trace-vlag."""
    import queue

    from vortex_worker.train_lora import LoraCfg, train, validate

    trace = request.trace
    c = LoraCfg.from_dict({**cfg, "kind": request.job_kind.split(":", 1)[1]})
    try:
        stats = validate(c)
    except ValueError as e:
        yield worker_pb2.TrainUpdate(error=f"dataset/instellingen: {e}")
        yield worker_pb2.TrainUpdate(done=True)
        return
    yield worker_pb2.TrainUpdate(
        progress=common_pb2.Progress(trace=trace, fraction=0.0, stage="gevalideerd", message=json.dumps(stats))
    )
    flag = self_.cancels.flag(trace.trace_id) if trace.trace_id else threading.Event()
    q: queue.Queue = queue.Queue()
    result: dict = {}

    def run() -> None:
        try:
            result.update(train(c, cancel=flag, progress=lambda f, m: q.put((f, m))))
        except ImportError as e:
            result["error"] = f"training vereist de extra 'train' (PyTorch/transformers/peft/trl): {e}"
        except Exception as e:
            result["error"] = str(e)
        finally:
            q.put(None)

    threading.Thread(target=run, daemon=True).start()
    while (item := q.get()) is not None:
        yield worker_pb2.TrainUpdate(
            progress=common_pb2.Progress(trace=trace, fraction=item[0], stage="trainen", message=item[1])
        )
    if trace.trace_id:
        self_.cancels.forget(trace.trace_id)
    if "error" in result:
        yield worker_pb2.TrainUpdate(error=result["error"])
    elif result.get("paused"):
        yield worker_pb2.TrainUpdate(
            progress=common_pb2.Progress(
                trace=trace, fraction=-1, stage="gepauzeerd", message=result.get("checkpoint", "")
            )
        )
    else:
        metrics = {k: float(v) for k, v in result.items() if isinstance(v, (int, float))}
        yield worker_pb2.TrainUpdate(
            artifact=common_pb2.ArtifactRef(
                artifact_id=result.get("adapter_dir", ""), path=result.get("gguf", ""), media_type="application/gguf"
            ),
            metrics=metrics,
        )
    yield worker_pb2.TrainUpdate(done=True)


WorkerService._train_lora = _lora_updates  # type: ignore[attr-defined]


def build_server(address: str, max_workers: int = 4, out_dir: Path | None = None) -> tuple[grpc.Server, int]:
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=max_workers))
    worker_pb2_grpc.add_WorkerServiceServicer_to_server(WorkerService(out_dir), server)
    port = server.add_insecure_port(address)
    return server, port


def main() -> None:
    p = argparse.ArgumentParser(description="Vortex Python-worker")
    p.add_argument("--address", default="127.0.0.1:50061")
    p.add_argument("--out-dir", default=None, help="map voor ingest-JSONL (koude schijf)")
    args = p.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s %(message)s")
    server, port = build_server(args.address, out_dir=Path(args.out_dir) if args.out_dir else None)
    server.start()
    log.info("worker luistert op poort %s", port)
    server.wait_for_termination()


if __name__ == "__main__":
    main()
