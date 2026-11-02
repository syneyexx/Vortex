from vortex_worker.gen.vortex.common.v1 import common_pb2 as _common_pb2
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ExecRequest(_message.Message):
    __slots__ = ("trace", "language", "code", "workdir", "timeout_ms", "max_output_bytes")
    TRACE_FIELD_NUMBER: _ClassVar[int]
    LANGUAGE_FIELD_NUMBER: _ClassVar[int]
    CODE_FIELD_NUMBER: _ClassVar[int]
    WORKDIR_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    MAX_OUTPUT_BYTES_FIELD_NUMBER: _ClassVar[int]
    trace: _common_pb2.TraceContext
    language: str
    code: str
    workdir: str
    timeout_ms: int
    max_output_bytes: int
    def __init__(self, trace: _Optional[_Union[_common_pb2.TraceContext, _Mapping]] = ..., language: _Optional[str] = ..., code: _Optional[str] = ..., workdir: _Optional[str] = ..., timeout_ms: _Optional[int] = ..., max_output_bytes: _Optional[int] = ...) -> None: ...

class ExecReply(_message.Message):
    __slots__ = ("exit_code", "stdout", "stderr", "truncated", "timed_out", "duration_ms")
    EXIT_CODE_FIELD_NUMBER: _ClassVar[int]
    STDOUT_FIELD_NUMBER: _ClassVar[int]
    STDERR_FIELD_NUMBER: _ClassVar[int]
    TRUNCATED_FIELD_NUMBER: _ClassVar[int]
    TIMED_OUT_FIELD_NUMBER: _ClassVar[int]
    DURATION_MS_FIELD_NUMBER: _ClassVar[int]
    exit_code: int
    stdout: str
    stderr: str
    truncated: bool
    timed_out: bool
    duration_ms: int
    def __init__(self, exit_code: _Optional[int] = ..., stdout: _Optional[str] = ..., stderr: _Optional[str] = ..., truncated: _Optional[bool] = ..., timed_out: _Optional[bool] = ..., duration_ms: _Optional[int] = ...) -> None: ...
