"""gRPC-server van de worker. W00: Health + Cancel. Ingest volgt in W10, training in W13."""

# VX:W00.12
from __future__ import annotations

import argparse
import logging
import threading
from concurrent import futures

import grpc

from vortex_worker import __version__
from vortex_worker.gen.vortex.common.v1 import common_pb2
from vortex_worker.gen.vortex.worker.v1 import worker_pb2_grpc

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
    def __init__(self) -> None:
        self.cancels = CancelRegistry()

    def GetHealth(self, request, context):
        return common_pb2.Health(ok=True, component="worker", detail="gereed", version=__version__)

    def Cancel(self, request, context):
        return common_pb2.CancelReply(accepted=self.cancels.cancel(request.trace.trace_id))

    def Parse(self, request, context):
        context.abort(grpc.StatusCode.UNIMPLEMENTED, "Parse volgt in W10")

    def Ingest(self, request, context):
        context.abort(grpc.StatusCode.UNIMPLEMENTED, "Ingest volgt in W10")

    def Train(self, request, context):
        context.abort(grpc.StatusCode.UNIMPLEMENTED, "Train volgt in W13")


def build_server(address: str, max_workers: int = 4) -> tuple[grpc.Server, int]:
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=max_workers))
    worker_pb2_grpc.add_WorkerServiceServicer_to_server(WorkerService(), server)
    port = server.add_insecure_port(address)
    return server, port


def main() -> None:
    p = argparse.ArgumentParser(description="Vortex Python-worker")
    p.add_argument("--address", default="127.0.0.1:50061")
    args = p.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s %(message)s")
    server, port = build_server(args.address)
    server.start()
    log.info("worker luistert op poort %s", port)
    server.wait_for_termination()


if __name__ == "__main__":
    main()
