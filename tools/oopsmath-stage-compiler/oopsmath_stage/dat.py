"""DAT v1 container: writer, reader/verifier and atomic file output."""

from __future__ import annotations

import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Sequence

import zstandard

from .constants import (
    ALIGNMENT, COMPRESSION_MASK, COMPRESSION_NONE, COMPRESSION_ZSTD, DAT_FORMAT_VERSION,
    DAT_MAGIC, HEADER_SIZE, HEADER_STRUCT, MIN_RUNTIME_VERSION, SECTION_BASE_FLAGS,
    SECTION_COMPRESSIBLE, SECTION_ENTRY_SIZE, SECTION_HEADER_FLAG, SECTION_ORDER,
    SECTION_STRUCT, STAGE_SCHEMA_VERSION, HeaderFlag,
)
from .errors import BinaryWriteError, DatFormatError
from .util import align_up, crc32, zstd_compress, zstd_decompress


@dataclass(frozen=True)
class Section:
    type: str
    payload: bytes

    @property
    def flags(self) -> int:
        return SECTION_BASE_FLAGS[self.type]

    @property
    def compressible(self) -> bool:
        return SECTION_COMPRESSIBLE[self.type]


def pack_header(*, flags: int, file_size: int, table_offset: int,
                section_count: int) -> bytes:
    """Pack the 64-byte header; CRC32 covers the header with its CRC field zeroed."""
    def raw(crc: int) -> bytes:
        return HEADER_STRUCT.pack(
            DAT_MAGIC, DAT_FORMAT_VERSION, STAGE_SCHEMA_VERSION, HEADER_SIZE, 0,
            flags, file_size, table_offset, section_count, SECTION_ENTRY_SIZE,
            MIN_RUNTIME_VERSION, crc, bytes(12))
    return raw(crc32(raw(0)))


def build_dat(sections: Sequence[Section], *, debug: bool = False) -> bytes:
    """Assemble a complete DAT v1 file from raw section payloads."""
    names = [s.type for s in sections]
    if len(set(names)) != len(names):
        raise BinaryWriteError("duplicate section types")
    unknown = [n for n in names if n not in SECTION_ORDER]
    if unknown:
        raise BinaryWriteError(f"unknown section types: {unknown}")
    if "META" not in names or "STAG" not in names:
        raise BinaryWriteError("META and STAG sections are mandatory")
    ordered = sorted(sections, key=lambda s: SECTION_ORDER.index(s.type))

    table_offset = HEADER_SIZE
    data_offset = align_up(table_offset + len(ordered) * SECTION_ENTRY_SIZE)
    table = bytearray()
    body = bytearray()
    header_flags = HeaderFlag(0)
    cursor = data_offset
    for section in ordered:
        raw = section.payload
        stored, compression = raw, COMPRESSION_NONE
        if section.compressible and raw:
            candidate = zstd_compress(raw)
            if len(candidate) < len(raw):          # only when strictly beneficial
                stored, compression = candidate, COMPRESSION_ZSTD
                header_flags |= HeaderFlag.HAS_COMPRESSED_SECTIONS
        table += SECTION_STRUCT.pack(
            section.type.encode("ascii"), section.flags | compression, cursor,
            len(stored), len(raw), crc32(raw), 0, bytes(8))
        padded = align_up(len(stored))
        body += stored + bytes(padded - len(stored))
        cursor += padded
        header_flags |= SECTION_HEADER_FLAG.get(section.type, HeaderFlag(0))
    if debug:
        header_flags |= HeaderFlag.DEBUG_BUILD
    file_size = data_offset + len(body)
    blob = pack_header(flags=int(header_flags), file_size=file_size,
                       table_offset=table_offset, section_count=len(ordered))
    blob += bytes(table) + bytes(data_offset - table_offset - len(table)) + bytes(body)
    if len(blob) != file_size:
        raise BinaryWriteError("internal error: computed file size mismatch")
    return blob


@dataclass
class DatHeader:
    magic: bytes
    format_version: int
    schema_version: int
    header_size: int
    flags: int
    file_size: int
    section_table_offset: int
    section_count: int
    section_entry_size: int
    minimum_runtime_version: int
    header_crc32: int


@dataclass
class ParsedSection:
    type: str
    flags: int
    offset: int
    stored_size: int
    raw_size: int
    crc32: int
    raw: Optional[bytes]
    status: str  # "OK" or a description of the problem

    @property
    def compression(self) -> int:
        return self.flags & COMPRESSION_MASK


@dataclass
class ParsedDat:
    header: DatHeader
    header_crc_ok: bool
    sections: list[ParsedSection]
    problems: list[str]

    @property
    def is_valid(self) -> bool:
        return (self.header_crc_ok and not self.problems
                and all(s.status == "OK" for s in self.sections))

    def get(self, section_type: str) -> Optional[ParsedSection]:
        return next((s for s in self.sections if s.type == section_type), None)


def parse_dat(data: bytes) -> ParsedDat:
    """Parse and verify a DAT v1 file without executing any of its content."""
    if len(data) < HEADER_SIZE:
        raise DatFormatError("file is shorter than the 64-byte header")
    (magic, fmt, schema, hsize, res0, flags, fsize, toff, count, esize, minrt,
     hcrc, res1) = HEADER_STRUCT.unpack_from(data, 0)
    if magic != DAT_MAGIC:
        raise DatFormatError(f"bad magic {magic!r}, expected {DAT_MAGIC!r}")
    if fmt != DAT_FORMAT_VERSION:
        raise DatFormatError(f"unsupported DAT format version {fmt}")
    if hsize != HEADER_SIZE:
        raise DatFormatError(f"unexpected header size {hsize}")
    if esize != SECTION_ENTRY_SIZE:
        raise DatFormatError(f"unexpected section entry size {esize}")
    problems: list[str] = []
    zeroed = data[:48] + bytes(4) + data[52:HEADER_SIZE]
    header_crc_ok = crc32(zeroed) == hcrc
    if fsize != len(data):
        problems.append(f"header file_size {fsize} != actual size {len(data)}")
    if res0 != 0 or res1 != bytes(12):
        problems.append("reserved header bytes are not zero")
    if toff < HEADER_SIZE or toff + count * esize > len(data):
        raise DatFormatError("section table lies outside the file")
    header = DatHeader(magic, fmt, schema, hsize, flags, fsize, toff, count,
                       esize, minrt, hcrc)
    sections: list[ParsedSection] = []
    for index in range(count):
        (tbytes, sflags, offset, stored, raw_size, scrc, r0,
         r1) = SECTION_STRUCT.unpack_from(data, toff + index * esize)
        stype = tbytes.decode("ascii", errors="replace")
        raw: Optional[bytes] = None
        status = "OK"
        if r0 != 0 or r1 != bytes(8):
            problems.append(f"section {stype}: reserved bytes are not zero")
        if offset % ALIGNMENT:
            problems.append(f"section {stype}: offset {offset} is not 16-byte aligned")
        if offset + stored > len(data):
            status = "OUT OF BOUNDS"
        else:
            blob = data[offset:offset + stored]
            comp = sflags & COMPRESSION_MASK
            try:
                if comp == COMPRESSION_NONE:
                    raw = blob
                elif comp == COMPRESSION_ZSTD:
                    raw = zstd_decompress(blob, raw_size)
                else:
                    status = f"UNKNOWN COMPRESSION {comp}"
            except zstandard.ZstdError as exc:
                status = f"DECOMPRESSION FAILED ({exc})"
            if raw is not None:
                if len(raw) != raw_size:
                    status, raw = f"RAW SIZE MISMATCH ({len(raw)} != {raw_size})", None
                elif crc32(raw) != scrc:
                    status = "CRC32 MISMATCH"
        sections.append(ParsedSection(stype, sflags, offset, stored, raw_size,
                                      scrc, raw, status))
    return ParsedDat(header, header_crc_ok, sections, problems)


def write_atomic(path: Path, payload: bytes) -> None:
    """Write through a temporary file in the target directory, then rename."""
    path = Path(path)
    tmp_name: Optional[str] = None
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        fd, tmp_name = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".",
                                        suffix=".tmp")
        with os.fdopen(fd, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(tmp_name, 0o644)
        os.replace(tmp_name, path)
    except OSError as exc:
        if tmp_name is not None:
            try:
                os.unlink(tmp_name)
            except OSError:
                pass  # temp file already gone; the original error is what matters
        raise BinaryWriteError(f"cannot write '{path}': {exc}") from exc
