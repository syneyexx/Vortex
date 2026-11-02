from vortex_worker.gen.vortex.common.v1 import common_pb2 as _common_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class StatusRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class GpuInfo(_message.Message):
    __slots__ = ("index", "name", "vram_total_mib", "vram_used_mib", "role")
    INDEX_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    VRAM_TOTAL_MIB_FIELD_NUMBER: _ClassVar[int]
    VRAM_USED_MIB_FIELD_NUMBER: _ClassVar[int]
    ROLE_FIELD_NUMBER: _ClassVar[int]
    index: int
    name: str
    vram_total_mib: int
    vram_used_mib: int
    role: str
    def __init__(self, index: _Optional[int] = ..., name: _Optional[str] = ..., vram_total_mib: _Optional[int] = ..., vram_used_mib: _Optional[int] = ..., role: _Optional[str] = ...) -> None: ...

class ModelServer(_message.Message):
    __slots__ = ("name", "url", "ready", "model", "detail")
    NAME_FIELD_NUMBER: _ClassVar[int]
    URL_FIELD_NUMBER: _ClassVar[int]
    READY_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    DETAIL_FIELD_NUMBER: _ClassVar[int]
    name: str
    url: str
    ready: bool
    model: str
    detail: str
    def __init__(self, name: _Optional[str] = ..., url: _Optional[str] = ..., ready: _Optional[bool] = ..., model: _Optional[str] = ..., detail: _Optional[str] = ...) -> None: ...

class StatusReply(_message.Message):
    __slots__ = ("version", "components", "gpus", "models")
    VERSION_FIELD_NUMBER: _ClassVar[int]
    COMPONENTS_FIELD_NUMBER: _ClassVar[int]
    GPUS_FIELD_NUMBER: _ClassVar[int]
    MODELS_FIELD_NUMBER: _ClassVar[int]
    version: str
    components: _containers.RepeatedCompositeFieldContainer[_common_pb2.Health]
    gpus: _containers.RepeatedCompositeFieldContainer[GpuInfo]
    models: _containers.RepeatedCompositeFieldContainer[ModelServer]
    def __init__(self, version: _Optional[str] = ..., components: _Optional[_Iterable[_Union[_common_pb2.Health, _Mapping]]] = ..., gpus: _Optional[_Iterable[_Union[GpuInfo, _Mapping]]] = ..., models: _Optional[_Iterable[_Union[ModelServer, _Mapping]]] = ...) -> None: ...

class ListRunsRequest(_message.Message):
    __slots__ = ("include_finished", "limit")
    INCLUDE_FINISHED_FIELD_NUMBER: _ClassVar[int]
    LIMIT_FIELD_NUMBER: _ClassVar[int]
    include_finished: bool
    limit: int
    def __init__(self, include_finished: _Optional[bool] = ..., limit: _Optional[int] = ...) -> None: ...

class RunSummary(_message.Message):
    __slots__ = ("run_id", "kind", "state", "created_at")
    RUN_ID_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    STATE_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    run_id: str
    kind: str
    state: _common_pb2.RunState
    created_at: str
    def __init__(self, run_id: _Optional[str] = ..., kind: _Optional[str] = ..., state: _Optional[_Union[_common_pb2.RunState, str]] = ..., created_at: _Optional[str] = ...) -> None: ...

class ListRunsReply(_message.Message):
    __slots__ = ("runs",)
    RUNS_FIELD_NUMBER: _ClassVar[int]
    runs: _containers.RepeatedCompositeFieldContainer[RunSummary]
    def __init__(self, runs: _Optional[_Iterable[_Union[RunSummary, _Mapping]]] = ...) -> None: ...

class StopRunRequest(_message.Message):
    __slots__ = ("run_id",)
    RUN_ID_FIELD_NUMBER: _ClassVar[int]
    run_id: str
    def __init__(self, run_id: _Optional[str] = ...) -> None: ...

class StopRunReply(_message.Message):
    __slots__ = ("accepted",)
    ACCEPTED_FIELD_NUMBER: _ClassVar[int]
    accepted: bool
    def __init__(self, accepted: _Optional[bool] = ...) -> None: ...
