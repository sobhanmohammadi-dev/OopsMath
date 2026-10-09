"""Small helpers: CRC32, alignment, canonical MessagePack, Zstandard."""

from __future__ import annotations

import binascii
from typing import Any, Mapping

import msgpack
import zstandard

from .constants import ALIGNMENT
from .errors import DatFormatError, StageCompileError


def crc32(data: bytes) -> int:
    return binascii.crc32(data) & 0xFFFFFFFF


def align_up(value: int, alignment: int = ALIGNMENT) -> int:
    return (value + alignment - 1) // alignment * alignment


def canonicalize(value: Any, path: str = "$") -> Any:
    """Return a plain-typed copy with string keys sorted (stable MessagePack)."""
    if isinstance(value, Mapping):
        out: dict[str, Any] = {}
        for key in sorted(value, key=lambda k: str(k)):
            if not isinstance(key, str):
                raise StageCompileError(f"{path}: non-string mapping key {key!r}")
            out[key] = canonicalize(value[key], f"{path}.{key}")
        return out
    if isinstance(value, (list, tuple)):
        return [canonicalize(v, f"{path}[{i}]") for i, v in enumerate(value)]
    if isinstance(value, float):
        if value != value or value in (float("inf"), float("-inf")):
            raise StageCompileError(f"{path}: non-finite float is not allowed")
        return value
    if value is None or isinstance(value, (bool, int, str, bytes)):
        return value
    raise StageCompileError(f"{path}: unsupported type {type(value).__name__}")


def pack_msgpack(value: Any) -> bytes:
    try:
        return msgpack.packb(canonicalize(value), use_bin_type=True)
    except (OverflowError, ValueError, TypeError) as exc:
        raise StageCompileError(f"cannot serialise data to MessagePack: {exc}") from exc


def unpack_msgpack(data: bytes) -> Any:
    try:
        return msgpack.unpackb(data, raw=False, strict_map_key=True)
    except (msgpack.exceptions.UnpackException, ValueError) as exc:
        raise DatFormatError(f"invalid MessagePack payload: {exc}") from exc


def zstd_compress(data: bytes) -> bytes:
    compressor = zstandard.ZstdCompressor(level=19, write_checksum=False,
                                          write_content_size=True, threads=0)
    return compressor.compress(data)


def zstd_decompress(data: bytes, raw_size: int) -> bytes:
    return zstandard.ZstdDecompressor().decompress(data, max_output_size=max(raw_size, 1))
