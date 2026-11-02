from vortex_worker.gen.vortex.common.v1 import common_pb2 as _common_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class HealthRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ParseRequest(_message.Message):
    __slots__ = ("trace", "path", "media_type")
    TRACE_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    MEDIA_TYPE_FIELD_NUMBER: _ClassVar[int]
    trace: _common_pb2.TraceContext
    path: str
    media_type: str
    def __init__(self, trace: _Optional[_Union[_common_pb2.TraceContext, _Mapping]] = ..., path: _Optional[str] = ..., media_type: _Optional[str] = ...) -> None: ...

class ParsedSection(_message.Message):
    __slots__ = ("title", "level", "text", "locator")
    TITLE_FIELD_NUMBER: _ClassVar[int]
    LEVEL_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    LOCATOR_FIELD_NUMBER: _ClassVar[int]
    title: str
    level: int
    text: str
    locator: str
    def __init__(self, title: _Optional[str] = ..., level: _Optional[int] = ..., text: _Optional[str] = ..., locator: _Optional[str] = ...) -> None: ...

class ParseReply(_message.Message):
    __slots__ = ("content_hash", "sections", "metadata")
    class MetadataEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    CONTENT_HASH_FIELD_NUMBER: _ClassVar[int]
    SECTIONS_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    content_hash: str
    sections: _containers.RepeatedCompositeFieldContainer[ParsedSection]
    metadata: _containers.ScalarMap[str, str]
    def __init__(self, content_hash: _Optional[str] = ..., sections: _Optional[_Iterable[_Union[ParsedSection, _Mapping]]] = ..., metadata: _Optional[_Mapping[str, str]] = ...) -> None: ...

class IngestRequest(_message.Message):
    __slots__ = ("trace", "paths", "workspace_id")
    TRACE_FIELD_NUMBER: _ClassVar[int]
    PATHS_FIELD_NUMBER: _ClassVar[int]
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    trace: _common_pb2.TraceContext
    paths: _containers.RepeatedScalarFieldContainer[str]
    workspace_id: str
    def __init__(self, trace: _Optional[_Union[_common_pb2.TraceContext, _Mapping]] = ..., paths: _Optional[_Iterable[str]] = ..., workspace_id: _Optional[str] = ...) -> None: ...

class IngestUpdate(_message.Message):
    __slots__ = ("progress", "chunk_batch", "error", "done")
    PROGRESS_FIELD_NUMBER: _ClassVar[int]
    CHUNK_BATCH_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    DONE_FIELD_NUMBER: _ClassVar[int]
    progress: _common_pb2.Progress
    chunk_batch: _common_pb2.ArtifactRef
    error: str
    done: bool
    def __init__(self, progress: _Optional[_Union[_common_pb2.Progress, _Mapping]] = ..., chunk_batch: _Optional[_Union[_common_pb2.ArtifactRef, _Mapping]] = ..., error: _Optional[str] = ..., done: _Optional[bool] = ...) -> None: ...

class TrainRequest(_message.Message):
    __slots__ = ("trace", "job_kind", "config_toml", "dataset")
    TRACE_FIELD_NUMBER: _ClassVar[int]
    JOB_KIND_FIELD_NUMBER: _ClassVar[int]
    CONFIG_TOML_FIELD_NUMBER: _ClassVar[int]
    DATASET_FIELD_NUMBER: _ClassVar[int]
    trace: _common_pb2.TraceContext
    job_kind: str
    config_toml: str
    dataset: _common_pb2.ArtifactRef
    def __init__(self, trace: _Optional[_Union[_common_pb2.TraceContext, _Mapping]] = ..., job_kind: _Optional[str] = ..., config_toml: _Optional[str] = ..., dataset: _Optional[_Union[_common_pb2.ArtifactRef, _Mapping]] = ...) -> None: ...

class TrainUpdate(_message.Message):
    __slots__ = ("progress", "artifact", "error", "done", "metrics")
    class MetricsEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: float
        def __init__(self, key: _Optional[str] = ..., value: _Optional[float] = ...) -> None: ...
    PROGRESS_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    DONE_FIELD_NUMBER: _ClassVar[int]
    METRICS_FIELD_NUMBER: _ClassVar[int]
    progress: _common_pb2.Progress
    artifact: _common_pb2.ArtifactRef
    error: str
    done: bool
    metrics: _containers.ScalarMap[str, float]
    def __init__(self, progress: _Optional[_Union[_common_pb2.Progress, _Mapping]] = ..., artifact: _Optional[_Union[_common_pb2.ArtifactRef, _Mapping]] = ..., error: _Optional[str] = ..., done: _Optional[bool] = ..., metrics: _Optional[_Mapping[str, float]] = ...) -> None: ...
