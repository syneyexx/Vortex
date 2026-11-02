from vortex_worker.gen.vortex.common.v1 import common_pb2 as _common_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class Mode(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    MODE_UNSPECIFIED: _ClassVar[Mode]
    MODE_ADAPTIVE: _ClassVar[Mode]
    MODE_QUICK: _ClassVar[Mode]
    MODE_DEEP: _ClassVar[Mode]
    MODE_RESEARCH: _ClassVar[Mode]
MODE_UNSPECIFIED: Mode
MODE_ADAPTIVE: Mode
MODE_QUICK: Mode
MODE_DEEP: Mode
MODE_RESEARCH: Mode

class SubscribeRequest(_message.Message):
    __slots__ = ("from_seq", "session_id")
    FROM_SEQ_FIELD_NUMBER: _ClassVar[int]
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    from_seq: int
    session_id: str
    def __init__(self, from_seq: _Optional[int] = ..., session_id: _Optional[str] = ...) -> None: ...

class Frame(_message.Message):
    __slots__ = ("seq", "run_id", "text", "tool_start", "tool_result", "artifact", "warning", "end", "status", "progress")
    SEQ_FIELD_NUMBER: _ClassVar[int]
    RUN_ID_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    TOOL_START_FIELD_NUMBER: _ClassVar[int]
    TOOL_RESULT_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    WARNING_FIELD_NUMBER: _ClassVar[int]
    END_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    PROGRESS_FIELD_NUMBER: _ClassVar[int]
    seq: int
    run_id: str
    text: TextDelta
    tool_start: ToolStart
    tool_result: ToolResult
    artifact: _common_pb2.ArtifactRef
    warning: Warning
    end: End
    status: RunStatus
    progress: _common_pb2.Progress
    def __init__(self, seq: _Optional[int] = ..., run_id: _Optional[str] = ..., text: _Optional[_Union[TextDelta, _Mapping]] = ..., tool_start: _Optional[_Union[ToolStart, _Mapping]] = ..., tool_result: _Optional[_Union[ToolResult, _Mapping]] = ..., artifact: _Optional[_Union[_common_pb2.ArtifactRef, _Mapping]] = ..., warning: _Optional[_Union[Warning, _Mapping]] = ..., end: _Optional[_Union[End, _Mapping]] = ..., status: _Optional[_Union[RunStatus, _Mapping]] = ..., progress: _Optional[_Union[_common_pb2.Progress, _Mapping]] = ...) -> None: ...

class TextDelta(_message.Message):
    __slots__ = ("text",)
    TEXT_FIELD_NUMBER: _ClassVar[int]
    text: str
    def __init__(self, text: _Optional[str] = ...) -> None: ...

class ToolStart(_message.Message):
    __slots__ = ("tool_call_id", "tool", "summary")
    TOOL_CALL_ID_FIELD_NUMBER: _ClassVar[int]
    TOOL_FIELD_NUMBER: _ClassVar[int]
    SUMMARY_FIELD_NUMBER: _ClassVar[int]
    tool_call_id: str
    tool: str
    summary: str
    def __init__(self, tool_call_id: _Optional[str] = ..., tool: _Optional[str] = ..., summary: _Optional[str] = ...) -> None: ...

class ToolResult(_message.Message):
    __slots__ = ("tool_call_id", "ok", "summary", "duration_ms")
    TOOL_CALL_ID_FIELD_NUMBER: _ClassVar[int]
    OK_FIELD_NUMBER: _ClassVar[int]
    SUMMARY_FIELD_NUMBER: _ClassVar[int]
    DURATION_MS_FIELD_NUMBER: _ClassVar[int]
    tool_call_id: str
    ok: bool
    summary: str
    duration_ms: int
    def __init__(self, tool_call_id: _Optional[str] = ..., ok: _Optional[bool] = ..., summary: _Optional[str] = ..., duration_ms: _Optional[int] = ...) -> None: ...

class Warning(_message.Message):
    __slots__ = ("code", "message")
    CODE_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    code: str
    message: str
    def __init__(self, code: _Optional[str] = ..., message: _Optional[str] = ...) -> None: ...

class End(_message.Message):
    __slots__ = ("reason",)
    REASON_FIELD_NUMBER: _ClassVar[int]
    reason: str
    def __init__(self, reason: _Optional[str] = ...) -> None: ...

class RunStatus(_message.Message):
    __slots__ = ("state", "detail")
    STATE_FIELD_NUMBER: _ClassVar[int]
    DETAIL_FIELD_NUMBER: _ClassVar[int]
    state: _common_pb2.RunState
    detail: str
    def __init__(self, state: _Optional[_Union[_common_pb2.RunState, str]] = ..., detail: _Optional[str] = ...) -> None: ...

class SendMessageRequest(_message.Message):
    __slots__ = ("session_id", "text", "mode", "model_override", "attachments")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    MODEL_OVERRIDE_FIELD_NUMBER: _ClassVar[int]
    ATTACHMENTS_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    text: str
    mode: Mode
    model_override: str
    attachments: _containers.RepeatedCompositeFieldContainer[_common_pb2.ArtifactRef]
    def __init__(self, session_id: _Optional[str] = ..., text: _Optional[str] = ..., mode: _Optional[_Union[Mode, str]] = ..., model_override: _Optional[str] = ..., attachments: _Optional[_Iterable[_Union[_common_pb2.ArtifactRef, _Mapping]]] = ...) -> None: ...

class SendMessageReply(_message.Message):
    __slots__ = ("run_id", "message_id")
    RUN_ID_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_ID_FIELD_NUMBER: _ClassVar[int]
    run_id: str
    message_id: str
    def __init__(self, run_id: _Optional[str] = ..., message_id: _Optional[str] = ...) -> None: ...

class StopRequest(_message.Message):
    __slots__ = ("run_id",)
    RUN_ID_FIELD_NUMBER: _ClassVar[int]
    run_id: str
    def __init__(self, run_id: _Optional[str] = ...) -> None: ...

class StopReply(_message.Message):
    __slots__ = ("accepted",)
    ACCEPTED_FIELD_NUMBER: _ClassVar[int]
    accepted: bool
    def __init__(self, accepted: _Optional[bool] = ...) -> None: ...

class SetSettingRequest(_message.Message):
    __slots__ = ("key", "value_json", "apply_now")
    KEY_FIELD_NUMBER: _ClassVar[int]
    VALUE_JSON_FIELD_NUMBER: _ClassVar[int]
    APPLY_NOW_FIELD_NUMBER: _ClassVar[int]
    key: str
    value_json: str
    apply_now: bool
    def __init__(self, key: _Optional[str] = ..., value_json: _Optional[str] = ..., apply_now: _Optional[bool] = ...) -> None: ...

class SetSettingReply(_message.Message):
    __slots__ = ("config_version",)
    CONFIG_VERSION_FIELD_NUMBER: _ClassVar[int]
    config_version: str
    def __init__(self, config_version: _Optional[str] = ...) -> None: ...
