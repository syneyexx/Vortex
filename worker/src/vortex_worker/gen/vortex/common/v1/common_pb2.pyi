from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class RunState(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    RUN_STATE_UNSPECIFIED: _ClassVar[RunState]
    RUN_STATE_QUEUED: _ClassVar[RunState]
    RUN_STATE_STARTING: _ClassVar[RunState]
    RUN_STATE_RUNNING: _ClassVar[RunState]
    RUN_STATE_WAITING: _ClassVar[RunState]
    RUN_STATE_CANCELLING: _ClassVar[RunState]
    RUN_STATE_COMPLETED: _ClassVar[RunState]
    RUN_STATE_FAILED: _ClassVar[RunState]
    RUN_STATE_CANCELLED: _ClassVar[RunState]
RUN_STATE_UNSPECIFIED: RunState
RUN_STATE_QUEUED: RunState
RUN_STATE_STARTING: RunState
RUN_STATE_RUNNING: RunState
RUN_STATE_WAITING: RunState
RUN_STATE_CANCELLING: RunState
RUN_STATE_COMPLETED: RunState
RUN_STATE_FAILED: RunState
RUN_STATE_CANCELLED: RunState

class TraceContext(_message.Message):
    __slots__ = ("trace_id", "span_id", "run_id")
    TRACE_ID_FIELD_NUMBER: _ClassVar[int]
    SPAN_ID_FIELD_NUMBER: _ClassVar[int]
    RUN_ID_FIELD_NUMBER: _ClassVar[int]
    trace_id: str
    span_id: str
    run_id: str
    def __init__(self, trace_id: _Optional[str] = ..., span_id: _Optional[str] = ..., run_id: _Optional[str] = ...) -> None: ...

class CancelRequest(_message.Message):
    __slots__ = ("trace", "reason")
    TRACE_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    trace: TraceContext
    reason: str
    def __init__(self, trace: _Optional[_Union[TraceContext, _Mapping]] = ..., reason: _Optional[str] = ...) -> None: ...

class CancelReply(_message.Message):
    __slots__ = ("accepted",)
    ACCEPTED_FIELD_NUMBER: _ClassVar[int]
    accepted: bool
    def __init__(self, accepted: _Optional[bool] = ...) -> None: ...

class Health(_message.Message):
    __slots__ = ("ok", "component", "detail", "version")
    OK_FIELD_NUMBER: _ClassVar[int]
    COMPONENT_FIELD_NUMBER: _ClassVar[int]
    DETAIL_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    ok: bool
    component: str
    detail: str
    version: str
    def __init__(self, ok: _Optional[bool] = ..., component: _Optional[str] = ..., detail: _Optional[str] = ..., version: _Optional[str] = ...) -> None: ...

class ArtifactRef(_message.Message):
    __slots__ = ("artifact_id", "path", "media_type", "size_bytes")
    ARTIFACT_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    MEDIA_TYPE_FIELD_NUMBER: _ClassVar[int]
    SIZE_BYTES_FIELD_NUMBER: _ClassVar[int]
    artifact_id: str
    path: str
    media_type: str
    size_bytes: int
    def __init__(self, artifact_id: _Optional[str] = ..., path: _Optional[str] = ..., media_type: _Optional[str] = ..., size_bytes: _Optional[int] = ...) -> None: ...

class Progress(_message.Message):
    __slots__ = ("trace", "fraction", "stage", "message")
    TRACE_FIELD_NUMBER: _ClassVar[int]
    FRACTION_FIELD_NUMBER: _ClassVar[int]
    STAGE_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    trace: TraceContext
    fraction: float
    stage: str
    message: str
    def __init__(self, trace: _Optional[_Union[TraceContext, _Mapping]] = ..., fraction: _Optional[float] = ..., stage: _Optional[str] = ..., message: _Optional[str] = ...) -> None: ...
