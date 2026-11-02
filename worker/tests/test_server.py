import grpc

from vortex_worker.gen.vortex.common.v1 import common_pb2
from vortex_worker.gen.vortex.worker.v1 import worker_pb2, worker_pb2_grpc
from vortex_worker.server import CancelRegistry, build_server


def test_health_and_cancel_roundtrip():
    server, port = build_server("127.0.0.1:0")
    server.start()
    try:
        with grpc.insecure_channel(f"127.0.0.1:{port}") as ch:
            stub = worker_pb2_grpc.WorkerServiceStub(ch)
            h = stub.GetHealth(worker_pb2.HealthRequest(), timeout=5)
            assert h.ok and h.component == "worker"
            r = stub.Cancel(common_pb2.CancelRequest(trace=common_pb2.TraceContext(trace_id="trc_x")), timeout=5)
            assert r.accepted is False  # onbekende trace
    finally:
        server.stop(0)


def test_unimplemented_is_explicit():
    server, port = build_server("127.0.0.1:0")
    server.start()
    try:
        with grpc.insecure_channel(f"127.0.0.1:{port}") as ch:
            stub = worker_pb2_grpc.WorkerServiceStub(ch)
            try:
                next(iter(stub.Train(worker_pb2.TrainRequest(job_kind="lora"), timeout=5)))
            except grpc.RpcError as e:
                assert e.code() == grpc.StatusCode.UNIMPLEMENTED
            else:
                raise AssertionError("moet UNIMPLEMENTED geven")
    finally:
        server.stop(0)


def test_cancel_registry():
    reg = CancelRegistry()
    ev = reg.flag("t1")
    assert not ev.is_set()
    assert reg.cancel("t1")
    assert ev.is_set()
    reg.forget("t1")
    assert not reg.cancel("t1")
