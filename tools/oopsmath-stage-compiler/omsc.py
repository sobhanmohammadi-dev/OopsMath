from __future__ import annotations

import argparse
import binascii
import copy
import dataclasses
import enum
import hashlib
import json
import os
import re
import struct
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath
from typing import Any, Callable, Mapping, Optional, Sequence

try:
    import jsonschema
    import msgpack
    import yaml
    import zstandard
except ImportError as _exc:  # pragma: no cover - environment problem
    sys.stderr.write(
        f"ERROR: missing Python dependency ({_exc}).\n"
        "Install with: pip install PyYAML jsonschema msgpack zstandard\n"
    )
    raise SystemExit(4)

# ===========================================================================
# Constants
# ===========================================================================

COMPILER_VERSION = "1.0.0"

EXIT_OK = 0
EXIT_VALIDATION = 2
EXIT_ARGS = 3
EXIT_IO = 4
EXIT_WRITE = 5

DAT_MAGIC = b"OOPSMDAT"
DAT_FORMAT_VERSION = 1
STAGE_SCHEMA_VERSION = 1
HEADER_SIZE = 64
SECTION_ENTRY_SIZE = 48
ALIGNMENT = 16
MIN_RUNTIME_VERSION = 0x0001_0000  # 1.0.0 encoded as major<<16 | minor<<8 | patch

# Header layout (little endian, no padding):
#  0 magic[8] | 8 u16 format | 10 u16 schema | 12 u16 header_size | 14 u16 reserved
# 16 u32 flags | 20 u64 file_size | 28 u64 section_table_offset | 36 u32 count
# 40 u32 entry_size | 44 u32 min_runtime | 48 u32 header_crc32 | 52 reserved[12]
HEADER_STRUCT = struct.Struct("<8sHHHHIQQIIII12s")
# Section entry (48 bytes): type[4] | u32 flags | u64 offset | u64 stored |
# u64 raw | u32 crc32 | u32 reserved | reserved[8]
SECTION_STRUCT = struct.Struct("<4sIQQQII8s")
assert HEADER_STRUCT.size == HEADER_SIZE and SECTION_STRUCT.size == SECTION_ENTRY_SIZE

# World binary: magic, version, chunk_size, size xyz, palette_count, chunk_count
WORLD_MAGIC = b"OWLD"
WORLD_VERSION = 1
WORLD_CHUNK_SIZE = 16
WORLD_CHUNK_VOLUME = WORLD_CHUNK_SIZE ** 3
WORLD_HEADER = struct.Struct("<4sHHIIIII")
# Chunk record: chunk x,y,z (u16) | encoding u8 (0 dense, 1 RLE) | payload size u32
WORLD_CHUNK_RECORD = struct.Struct("<HHHBI")
MAX_WORLD_AXIS = 1_000_000  # keeps chunk coordinates inside u16

SECTION_ORDER = ("META", "STAG", "WRLD", "LOCL", "DOCS", "ASIX", "ASDT")
MSGPACK_SECTIONS = frozenset({"META", "STAG", "LOCL", "DOCS", "ASIX"})


class HeaderFlag(enum.IntFlag):
    HAS_WORLD = 1 << 0
    HAS_LOCALIZATION = 1 << 1
    HAS_DOCUMENTS = 1 << 2
    HAS_CUSTOM_ASSETS = 1 << 3
    HAS_COMPRESSED_SECTIONS = 1 << 4
    DEBUG_BUILD = 1 << 5


COMPRESSION_MASK = 0b11
COMPRESSION_NONE = 0
COMPRESSION_ZSTD = 1
SECTION_CRITICAL = 1 << 8
SECTION_STREAMABLE = 1 << 9
SECTION_BINARY = 1 << 10

SECTION_BASE_FLAGS = {
    "META": SECTION_STREAMABLE,
    "STAG": SECTION_CRITICAL,
    "WRLD": SECTION_CRITICAL | SECTION_BINARY,
    "LOCL": SECTION_CRITICAL,
    "DOCS": 0,
    "ASIX": SECTION_CRITICAL,
    "ASDT": SECTION_CRITICAL | SECTION_BINARY,
}
# ASDT stays raw so that ASIX offsets address the GLB bytes directly.
SECTION_COMPRESSIBLE = {name: name != "ASDT" for name in SECTION_ORDER}
SECTION_HEADER_FLAG = {
    "WRLD": HeaderFlag.HAS_WORLD,
    "LOCL": HeaderFlag.HAS_LOCALIZATION,
    "DOCS": HeaderFlag.HAS_DOCUMENTS,
    "ASIX": HeaderFlag.HAS_CUSTOM_ASSETS,
}

ID_PATTERN = r"^[A-Za-z0-9_][A-Za-z0-9_.\-]*$"
LOC_KEY_PATTERN = r"^[A-Za-z][A-Za-z0-9_.\-]*$"
LOCALE_RE = re.compile(r"^[A-Za-z]{2,3}(?:[_-][A-Za-z0-9]+)*$")
PALETTE_KEY_PATTERN = r"^(?:[1-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])$"

MAX_YAML_BYTES = 8 * 1024 * 1024
MAX_TEXT_FILE_BYTES = 16 * 1024 * 1024
MAX_BINARY_FILE_BYTES = 512 * 1024 * 1024
MAX_YAML_NODES = 2_000_000
MAX_YAML_DEPTH = 100
MAX_CONDITION_DEPTH = 64
MAX_VOX_VOXELS = 20_000_000

GLB_MAGIC = b"glTF"
GLB_JSON_CHUNK = 0x4E4F534A
GLB_MIME = "model/gltf-binary"

CONDITION_FORMS = ("all", "any", "not", "task_completed", "target_completed",
                   "structure_stable", "physics_test_passed", "compare")
COMPARE_OPERATORS = ("==", "!=", ">", ">=", "<", "<=")
AIR_BLOCK = "air"


# ===========================================================================
# Exceptions and diagnostics
# ===========================================================================

class OopsMathError(Exception):
    """Base class for all compiler errors that map to an exit code."""
    exit_code = EXIT_VALIDATION


class StageValidationError(OopsMathError):
    def __init__(self, errors: Sequence[str]) -> None:
        super().__init__("\n".join(f"ERROR: {e}" for e in errors))
        self.errors = list(errors)


class ConfigError(OopsMathError):
    exit_code = EXIT_ARGS


class DatFormatError(OopsMathError):
    exit_code = EXIT_VALIDATION


class BinaryWriteError(OopsMathError):
    exit_code = EXIT_WRITE


class StageCompileError(OopsMathError):
    exit_code = EXIT_VALIDATION


class UnsafePathError(ValueError):
    pass


class VoxFormatError(ValueError):
    pass


class SourceDataError(ValueError):
    pass


@dataclass
class Diagnostics:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def error(self, message: str) -> None:
        self.errors.append(message)

    def warn(self, message: str) -> None:
        self.warnings.append(message)

    @property
    def ok(self) -> bool:
        return not self.errors


# ===========================================================================
# Embedded Stage Schema v1 (JSON Schema Draft 2020-12)
# ===========================================================================

def _build_stage_schema() -> dict[str, Any]:
    """Build the embedded Stage Schema v1 (source of truth for structure)."""

    def ref(name: str) -> dict[str, Any]:
        return {"$ref": f"#/$defs/{name}"}

    def obj(props: dict[str, Any], required: Sequence[str] = (),
            extra: bool = False) -> dict[str, Any]:
        schema: dict[str, Any] = {"type": "object", "properties": props,
                                  "additionalProperties": extra}
        if required:
            schema["required"] = list(required)
        return schema

    def arr(items: Any, **kw: Any) -> dict[str, Any]:
        return {"type": "array", "items": items, **kw}

    boolean = {"type": "boolean"}
    string = {"type": "string"}
    free_object = {"type": "object"}
    ident, loc = ref("id"), ref("loc_key")
    vec3n, vec3i, region = ref("vec3_num"), ref("vec3_int"), ref("region")
    nn_num, nn_int, pos_num = ref("nonneg_number"), ref("nonneg_int"), ref("pos_number")

    defs = {
        "id": {"type": "string", "pattern": ID_PATTERN, "maxLength": 128},
        "loc_key": {"type": "string", "pattern": LOC_KEY_PATTERN, "maxLength": 256},
        "relpath": {"type": "string", "minLength": 1, "maxLength": 512},
        "vec3_num": arr({"type": "number"}, minItems=3, maxItems=3),
        "vec3_int": arr({"type": "integer"}, minItems=3, maxItems=3),
        "vec3_size": arr({"type": "integer", "minimum": 1, "maximum": MAX_WORLD_AXIS},
                         minItems=3, maxItems=3),
        "region": obj({"min": vec3i, "max": vec3i}, required=("min", "max")),
        "nonneg_number": {"type": "number", "minimum": 0},
        "nonneg_int": {"type": "integer", "minimum": 0},
        "pos_number": {"type": "number", "exclusiveMinimum": 0},
        "number_map": {"type": "object", "additionalProperties": {"type": "number"}},
        "count_map": {"type": "object", "propertyNames": ident,
                      "additionalProperties": nn_int},
        "transform": obj({
            "position": vec3n, "rotation": vec3n,
            "scale": {"anyOf": [vec3n, pos_num]},
        }),
    }
    relpath = ref("relpath")

    stage = obj({
        "id": ident, "title": loc, "description": loc,
        "difficulty": {"enum": ["tutorial", "easy", "medium", "hard", "expert"]},
        "level": {"type": "integer", "minimum": 1},
        "requirements": obj({
            "player_level": {"type": "integer", "minimum": 1},
            "stages": arr(ident, uniqueItems=True),
        }),
        "tags": arr(string),
        "metadata": free_object,
    }, required=("id", "title"))

    story = obj({"dialogue": arr(obj({
        "id": ident, "speaker": string, "text": loc, "condition": free_object,
    }, required=("id", "speaker", "text")))})

    learning = obj({
        "question": loc,
        "curriculum": obj({
            "grade": obj({"min": {"type": "integer", "minimum": 7, "maximum": 9},
                          "max": {"type": "integer", "minimum": 7, "maximum": 9}},
                         required=("min", "max")),
            "topics": arr(ident, minItems=1, uniqueItems=True),
        }, required=("grade", "topics")),
        "lesson": obj({"markdown": relpath}),
        "tutorial_stage": obj({"enabled": boolean, "stage_id": ident}),
        "solution": obj({
            "explanation": loc, "markdown": relpath,
            "demonstration": obj({
                "enabled": boolean, "replay_tasks": boolean,
                "show_calculations": boolean, "show_construction": boolean,
                "simulate_physics": boolean,
            }),
        }),
        "hints": arr(obj({"id": ident, "text": loc, "markdown": relpath},
                         required=("id", "text"))),
    }, required=("question", "curriculum"))

    player = obj({
        "spawn": obj({"position": vec3n, "rotation": vec3n}),
        "initial_state": free_object,
    })
    camera = obj({"mode": {"enum": ["orthographic"]}, "position": vec3n,
                  "target": vec3n, "zoom": pos_num})

    world = obj({
        "bounds": obj({"size": ref("vec3_size"), "origin": vec3i}, required=("size",)),
        "environment": obj({"preset": string, "weather": string,
                            "time_of_day": string, "seed": {"type": "integer"}}),
        "terrain": obj({"type": string, "preset": string, "height": {"type": "number"}}),
        "source": obj({
            "voxel_file": relpath,
            "palette": {"type": "object",
                        "propertyNames": {"type": "string", "pattern": PALETTE_KEY_PATTERN},
                        "additionalProperties": ident},
            "default_block": ident,
            "offset": vec3i,
        }),
        "initial_structure": obj({"blocks": arr(obj({
            "id": ident, "block": ident, "position": vec3i,
            "rotation": vec3i, "state": free_object,
        }, required=("id", "block", "position")))}),
    }, required=("bounds",))

    assets = obj({"custom": arr(obj({
        "id": ident, "file": relpath,
        "type": {"enum": ["environment", "prop", "construction_piece",
                          "structure", "machinery", "interactive"]},
        "transform": ref("transform"),
        "behavior": obj({"placeable": boolean, "removable": boolean,
                         "mass_kg": nn_num, "structural_role": string,
                         "collider": string, "snap_profile": string}),
    }, required=("id", "file", "type")))})

    constraints = obj({
        "max_total_blocks": nn_int, "max_height": nn_int,
        "max_depth_below_ground": nn_int,
        "allowed_regions": arr(region), "forbidden_regions": arr(region),
        "foundation": obj({"required": boolean, "max_depth": nn_int,
                           "allowed_roles": arr(string)}),
        "structural_roles": obj({"required": arr(string), "min_support_count": nn_int,
                                 "max_support_count": nn_int,
                                 "require_path_to_ground": boolean}),
        "overhang": obj({"allowed": boolean, "max_length": nn_num}),
        "placement": obj({"require_support": boolean, "allow_floating": boolean,
                          "snap_to_grid": boolean, "rotation_allowed": boolean}),
        "material_limits": {"type": "object", "propertyNames": ident,
                            "additionalProperties": obj({"min": nn_int, "max": nn_int})},
    })
    construction = obj({
        "allowed_blocks": arr(obj({"id": ident, "min_count": nn_int, "max_count": nn_int},
                                  required=("id",))),
        "allowed_tools": arr(obj({"id": ident, "max_durability": nn_num},
                                 required=("id",))),
        "starting_inventory": obj({"blocks": ref("count_map"), "tools": ref("count_map")}),
        "targets": arr(obj({
            "id": ident,
            "type": {"enum": ["block_group", "wall", "floor", "roof", "room", "house",
                              "bridge", "building", "custom"]},
            "region": region, "required_block": ident,
            "dimensions": obj({"width": pos_num, "height": pos_num, "depth": pos_num,
                               "tolerance": nn_num}),
            "required_roles": arr(string),
            "constraints": free_object,
        }, required=("id", "type"))),
        "constraints": constraints,
        "validation": obj({k: boolean for k in (
            "dimensions", "target_geometry", "material_limits",
            "support_graph", "architecture", "block_roles")}),
    })

    economy = obj({
        "budget": nn_num,
        "rules": obj({"allow_overdraft": boolean,
                      "waste_percentage": {"type": "number", "minimum": 0, "maximum": 100},
                      "must_finish_within_budget": boolean}),
        "multipliers": ref("number_map"),
    }, required=("budget",))

    physics = obj({
        "enabled": boolean,
        "gravity": pos_num,
        "structural": obj({"stability_required": boolean, "collapse_enabled": boolean,
                           "ground_support_required": boolean,
                           "max_displacement": nn_num, "max_rotation": nn_num}),
        "tests": arr(obj({
            "id": ident,
            "type": {"enum": ["static_load", "moving_load", "earthquake", "wind", "impact"]},
            "duration_seconds": nn_num,
            "load": obj({
                "mass_kg": nn_num, "force_n": nn_num,
                "target": obj({
                    "type": {"enum": ["point", "surface", "region", "structure", "body"]},
                    "id": ident, "position": vec3n, "region": region,
                }),
                "distribution": {"enum": ["point", "uniform", "moving_point",
                                          "uniform_surface"]},
                "direction": vec3n,
            }),
            "environment": free_object, "acceptance": free_object,
        }, required=("id", "type"))),
    }, required=("enabled",))

    objectives = obj({
        "tasks": arr(obj({
            "id": ident,
            "type": {"enum": ["math", "construction", "purchase", "inspection",
                              "dialogue", "custom"]},
            "depends_on": arr(ident),
            "optional": boolean,
            "math": obj({
                "type": ident, "generator": free_object, "expression": string,
                "answer": obj({
                    "mode": {"enum": ["exact", "tolerance", "expression",
                                      "multiple_choice", "custom"]},
                    "value": {}, "tolerance": nn_num, "expression": string,
                }, required=("mode",)),
            }, required=("type",)),
            "target": ident, "data": free_object,
        }, required=("id", "type")), minItems=1),
        "completion": free_object,
        "failure": free_object,
        "checkpoints": arr(obj({"id": ident, "after_task": ident, "autosave": boolean},
                               required=("id",))),
        "time_limit_seconds": nn_num,
        "scoring": obj({"enabled": boolean, "maximum": nn_num,
                        "criteria": ref("number_map")}),
    }, required=("tasks", "completion"))

    events = arr(obj({
        "id": ident,
        "trigger": obj({"type": {"type": "string", "minLength": 1},
                        "condition": free_object}, required=("type",)),
        "actions": arr(free_object),
    }, required=("id", "trigger", "actions")))

    id_list = arr(ident)
    rewards = obj({
        "money": nn_num, "xp": nn_int,
        "unlocks": obj({"blocks": id_list, "tools": id_list,
                        "stages": id_list, "assets": id_list}),
    })
    audio = obj({"music": ident, "ambience": ident,
                 "sounds": {"type": "object", "additionalProperties": ident}})
    localization = obj({"ftl": relpath, "namespace": loc})

    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "OopsMath Stage Schema v1",
        "type": "object",
        "$defs": defs,
        "properties": {
            "schema_version": {"type": "integer", "const": STAGE_SCHEMA_VERSION},
            "stage": stage, "story": story, "learning": learning, "player": player,
            "camera": camera, "world": world, "assets": assets,
            "construction": construction, "economy": economy, "physics": physics,
            "objectives": objectives, "events": events, "rewards": rewards,
            "audio": audio, "localization": localization,
            "extensions": free_object,  # future extension namespace, preserved verbatim
        },
        "required": ["schema_version", "stage", "learning", "world",
                     "construction", "objectives", "rewards"],
        "additionalProperties": False,
    }


STAGE_SCHEMA: dict[str, Any] = _build_stage_schema()


# ===========================================================================
# Small helpers: CRC, alignment, MessagePack, Zstandard
# ===========================================================================

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


# ===========================================================================
# DAT writer and reader
# ===========================================================================

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


# ===========================================================================
# Path safety
# ===========================================================================

def normalize_rel_path(rel: str) -> str:
    """Validate a source-relative path purely lexically; return POSIX form."""
    if not isinstance(rel, str) or not rel:
        raise UnsafePathError("path must be a non-empty string")
    if "\x00" in rel:
        raise UnsafePathError("path contains a NUL character")
    if "\\" in rel:
        raise UnsafePathError("backslashes are not allowed; use '/' separators")
    if rel.startswith("/") or re.match(r"^[A-Za-z]:", rel):
        raise UnsafePathError("absolute paths are not allowed")
    parts = PurePosixPath(rel).parts
    if not parts:
        raise UnsafePathError("path is empty")
    if ".." in parts:
        raise UnsafePathError("parent directory traversal ('..') is not allowed")
    return "/".join(parts)


def resolve_safe_path(stage_root: Path, rel: str) -> tuple[Path, str]:
    """Return (filesystem path, normalised relative path) inside stage_root."""
    normalized = normalize_rel_path(rel)
    candidate = stage_root.joinpath(*normalized.split("/"))
    try:
        candidate.resolve().relative_to(stage_root.resolve())
    except ValueError as exc:
        raise UnsafePathError("path resolves outside the stage directory "
                              "(symbolic link escape?)") from exc
    return candidate, normalized


# ===========================================================================
# YAML loading
# ===========================================================================

class _UniqueKeyLoader(yaml.SafeLoader):
    """SafeLoader that rejects duplicate mapping keys."""


def _construct_unique_mapping(loader: _UniqueKeyLoader, node: yaml.MappingNode,
                              deep: bool = False) -> dict[Any, Any]:
    loader.flatten_mapping(node)
    seen: set[Any] = set()
    for key_node, _value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        try:
            duplicate = key in seen
        except TypeError as exc:
            raise yaml.constructor.ConstructorError(
                None, None, "unhashable mapping key", key_node.start_mark) from exc
        if duplicate:
            raise yaml.constructor.ConstructorError(
                None, None, f"duplicate key {key!r}", key_node.start_mark)
        seen.add(key)
    return yaml.SafeLoader.construct_mapping(loader, node, deep)


_UniqueKeyLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
                                 _construct_unique_mapping)


@dataclass
class _Budget:
    remaining: int


def _join_path(base: str, key: str) -> str:
    return key if base in ("", "<root>") else f"{base}.{key}"


def _normalise(value: Any, path: str, budget: _Budget, depth: int) -> Any:
    """Convert loaded YAML into JSON-like data (string keys, finite numbers)."""
    budget.remaining -= 1
    if budget.remaining < 0:
        raise SourceDataError("document is too large or expands too much (alias bomb?)")
    if depth > MAX_YAML_DEPTH:
        raise SourceDataError(f"{path}: nesting is too deep")
    if isinstance(value, dict):
        out: dict[str, Any] = {}
        for key, item in value.items():
            if isinstance(key, bool) or not isinstance(key, (str, int)):
                raise SourceDataError(f"{path}: mapping key {key!r} must be a string or integer")
            skey = str(key)
            if skey in out:
                raise SourceDataError(f"{path}: duplicate key '{skey}'")
            out[skey] = _normalise(item, _join_path(path, skey), budget, depth + 1)
        return out
    if isinstance(value, list):
        return [_normalise(v, f"{path}[{i}]", budget, depth + 1) for i, v in enumerate(value)]
    if value is None or isinstance(value, (bool, str)):
        return value
    if isinstance(value, int):
        if not -(2 ** 63) <= value < 2 ** 64:
            raise SourceDataError(f"{path}: integer {value} is out of range")
        return value
    if isinstance(value, float):
        if value != value or value in (float("inf"), float("-inf")):
            raise SourceDataError(f"{path}: non-finite number is not allowed")
        return value
    raise SourceDataError(f"{path}: unsupported YAML value of type {type(value).__name__} "
                          f"({value!r}); quote it if it should be a string")


def _read_limited(path: Path, limit: int) -> bytes:
    size = path.stat().st_size
    if size > limit:
        raise SourceDataError(f"file is too large ({size} bytes, limit {limit})")
    return path.read_bytes()


def load_stage_yaml(stage_dir: Path, diag: Diagnostics) -> Optional[dict[str, Any]]:
    """Layer 1: read and parse stage.yaml into normalised JSON-like data."""
    if not stage_dir.is_dir():
        diag.error(f"stage directory '{stage_dir}' does not exist or is not a directory")
        return None
    try:
        path, _ = resolve_safe_path(stage_dir, "stage.yaml")
    except UnsafePathError as exc:
        diag.error(f"stage.yaml: {exc}")
        return None
    if not path.is_file():
        diag.error("stage.yaml is missing from the stage directory")
        return None
    try:
        raw = _read_limited(path, MAX_YAML_BYTES)
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        diag.error("stage.yaml is not valid UTF-8")
        return None
    except SourceDataError as exc:
        diag.error(f"stage.yaml: {exc}")
        return None
    if text.startswith("\ufeff"):
        text = text[1:]  # a BOM is legal in YAML streams
    try:
        document = yaml.load(text, Loader=_UniqueKeyLoader)  # noqa: S506 - SafeLoader subclass
    except yaml.YAMLError as exc:
        mark = getattr(exc, "problem_mark", None)
        where = f" (line {mark.line + 1}, column {mark.column + 1})" if mark else ""
        problem = getattr(exc, "problem", None) or str(exc)
        diag.error(f"stage.yaml{where}: invalid YAML: {problem}")
        return None
    if not isinstance(document, dict):
        diag.error("stage.yaml: the root must be a mapping")
        return None
    try:
        return _normalise(document, "<root>", _Budget(MAX_YAML_NODES), 0)
    except SourceDataError as exc:
        diag.error(f"stage.yaml: {exc}")
        return None


def peek_stage_id(stage_dir: Path) -> Optional[str]:
    """Best-effort stage ID lookup used by build-all (no diagnostics)."""
    data = load_stage_yaml(stage_dir, Diagnostics())
    stage = data.get("stage") if data else None
    ident = stage.get("id") if isinstance(stage, dict) else None
    return ident if isinstance(ident, str) else None


# ===========================================================================
# Registries (manifest and base localization)
# ===========================================================================

@dataclass(frozen=True)
class Registry:
    """Built-in game IDs.  A ``None`` category means 'not validated'."""
    blocks: Optional[frozenset[str]] = None
    tools: Optional[frozenset[str]] = None
    assets: Optional[frozenset[str]] = None
    audio: Optional[frozenset[str]] = None


@dataclass(frozen=True)
class CompileOptions:
    manifest: Optional[Registry] = None
    base_keys: Optional[frozenset[str]] = None
    strict: bool = False
    debug: bool = False
    known_stage_ids: Optional[frozenset[str]] = None


def _registry_ids(value: Any, name: str) -> frozenset[str]:
    ids: list[str] = []
    if isinstance(value, dict):
        ids = [str(k) for k in value]
    elif isinstance(value, list):
        for item in value:
            if isinstance(item, str):
                ids.append(item)
            elif isinstance(item, dict) and isinstance(item.get("id"), str):
                ids.append(item["id"])
            else:
                raise ConfigError(f"manifest section '{name}' has an entry without an ID: {item!r}")
    else:
        raise ConfigError(f"manifest section '{name}' must be a list or a mapping")
    return frozenset(ids)


def load_manifest(path: Path) -> Registry:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise ConfigError(f"cannot read manifest '{path}': {exc}") from exc
    try:
        doc = json.loads(text) if path.suffix.lower() == ".json" else yaml.safe_load(text)
    except (ValueError, yaml.YAMLError) as exc:
        raise ConfigError(f"manifest '{path}' is not valid JSON/YAML: {exc}") from exc
    if not isinstance(doc, dict):
        raise ConfigError("manifest root must be a mapping")
    fields = {name: (_registry_ids(doc[name], name) if name in doc else None)
              for name in ("blocks", "tools", "assets", "audio")}
    return Registry(**fields)


def load_base_locales(directory: Path) -> frozenset[str]:
    if not directory.is_dir():
        raise ConfigError(f"base locale directory '{directory}' does not exist")
    keys: set[str] = set()
    diag = Diagnostics()
    for file in sorted(directory.rglob("*.ftl"), key=lambda p: p.as_posix()):
        try:
            text = file.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            raise ConfigError(f"cannot read base locale '{file}': {exc}") from exc
        keys |= parse_ftl(text, file.relative_to(directory).as_posix(), diag)
    if diag.errors:
        raise ConfigError("base locale files are malformed:\n" +
                          "\n".join(f"  {e}" for e in diag.errors))
    return frozenset(keys)


# ===========================================================================
# Lightweight Fluent (FTL) validator
# ===========================================================================

_FTL_ENTRY = re.compile(r"^(-?)([A-Za-z][A-Za-z0-9_.\-]*)[ \t]*=(.*)$")
_FTL_ATTRIBUTE = re.compile(r"^[ \t]+\.([A-Za-z][A-Za-z0-9_\-]*)[ \t]*=(.*)$")
_FTL_COMMENT = re.compile(r"^#{1,3}( .*)?$")


@dataclass
class _FtlEntry:
    key: str
    line: int
    segments: list[list[str]]
    has_attributes: bool = False


def _check_placeables(text: str) -> Optional[str]:
    """Check '{...}' balance; string literals are only legal inside placeables."""
    depth, in_string, i = 0, False, 0
    while i < len(text):
        ch = text[i]
        if in_string:
            if ch == "\\":
                i += 1
            elif ch == '"':
                in_string = False
        elif ch == '"' and depth > 0:
            in_string = True
        elif ch == "{":
            depth += 1
        elif ch == "}":
            if depth == 0:
                return "unmatched '}' (use {\"}\"} for a literal brace)"
            depth -= 1
        i += 1
    if in_string:
        return "unterminated string literal in placeable"
    if depth:
        return "unclosed '{' placeable"
    return None


def parse_ftl(text: str, rel_path: str, diag: Diagnostics) -> frozenset[str]:
    """Validate basic Fluent syntax; return the set of message keys."""
    messages: dict[str, int] = {}
    terms: dict[str, int] = {}
    if "\x00" in text:
        diag.error(f"{rel_path}: file contains NUL characters")
    if text.startswith("\ufeff"):
        diag.error(f"{rel_path}: UTF-8 byte-order mark is not allowed")
    current: Optional[_FtlEntry] = None

    def close(entry: Optional[_FtlEntry]) -> None:
        if entry is None:
            return
        if not "\n".join(entry.segments[0]).strip() and not entry.has_attributes:
            diag.error(f"{rel_path}:{entry.line}: '{entry.key}' has neither a value nor attributes")
        for segment in entry.segments:
            problem = _check_placeables("\n".join(segment))
            if problem:
                diag.error(f"{rel_path}:{entry.line}: in '{entry.key}': {problem}")

    for number, line in enumerate(text.replace("\r\n", "\n").split("\n"), start=1):
        if not line.strip():
            continue
        if line[0] in " \t":
            if current is None:
                diag.error(f"{rel_path}:{number}: indented text does not belong to any entry")
                continue
            attribute = _FTL_ATTRIBUTE.match(line)
            if attribute:
                current.has_attributes = True
                current.segments.append([attribute.group(2)])
            else:
                current.segments[-1].append(line)
        elif line[0] == "#":
            close(current)
            current = None
            if not _FTL_COMMENT.match(line):
                diag.error(f"{rel_path}:{number}: malformed comment (expected '# text')")
        else:
            close(current)
            current = None
            match = _FTL_ENTRY.match(line)
            if not match:
                diag.error(f"{rel_path}:{number}: malformed entry, expected 'key = value'")
                continue
            is_term, key = match.group(1) == "-", match.group(2)
            registry = terms if is_term else messages
            kind = "term" if is_term else "message"
            if key in registry:
                diag.error(f"{rel_path}:{number}: duplicate {kind} '{key}' "
                           f"(first defined at line {registry[key]})")
            else:
                registry[key] = number
            current = _FtlEntry(key, number, [[match.group(3)]])
    close(current)
    return frozenset(messages)


# ===========================================================================
# MagicaVoxel (.vox) parser
# ===========================================================================

class _VoxReader:
    def __init__(self, buf: bytes) -> None:
        self.buf, self.pos = buf, 0

    def i32(self) -> int:
        try:
            (value,) = struct.unpack_from("<i", self.buf, self.pos)
        except struct.error as exc:
            raise VoxFormatError("truncated chunk") from exc
        self.pos += 4
        return value

    def string(self) -> str:
        length = self.i32()
        if length < 0 or self.pos + length > len(self.buf):
            raise VoxFormatError("invalid string length in chunk")
        text = self.buf[self.pos:self.pos + length].decode("utf-8", errors="replace")
        self.pos += length
        return text

    def dict(self) -> dict[str, str]:
        count = self.i32()
        if not 0 <= count <= 4096:
            raise VoxFormatError("invalid dictionary size in chunk")
        return {self.string(): self.string() for _ in range(count)}


def _vox_rotation(value: Optional[str]) -> list[list[int]]:
    """Decode the packed '_r' rotation byte into a signed permutation matrix."""
    try:
        packed = int(value) if value is not None else 4  # 4 == identity
    except ValueError as exc:
        raise VoxFormatError(f"invalid rotation value '{value}'") from exc
    first, second = packed & 3, (packed >> 2) & 3
    if first == 3 or second == 3 or first == second:
        raise VoxFormatError(f"invalid rotation value {packed}")
    third = 3 - first - second
    rows = [[0, 0, 0] for _ in range(3)]
    for row, column, bit in ((0, first, 4), (1, second, 5), (2, third, 6)):
        rows[row][column] = -1 if packed & (1 << bit) else 1
    return rows


def _vox_translation(value: Optional[str]) -> tuple[int, int, int]:
    try:
        parts = [int(p) for p in (value or "0 0 0").split()]
    except ValueError as exc:
        raise VoxFormatError(f"invalid translation '{value}'") from exc
    if len(parts) != 3:
        raise VoxFormatError(f"invalid translation '{value}'")
    return parts[0], parts[1], parts[2]


def _mat_mul(a: list[list[int]], b: list[list[int]]) -> list[list[int]]:
    return [[sum(a[r][k] * b[k][c] for k in range(3)) for c in range(3)] for r in range(3)]


def _mat_vec(m: list[list[int]], v: Sequence[int]) -> tuple[int, int, int]:
    return (sum(m[0][k] * v[k] for k in range(3)),
            sum(m[1][k] * v[k] for k in range(3)),
            sum(m[2][k] * v[k] for k in range(3)))


def parse_vox(data: bytes) -> list[tuple[int, int, int, int]]:
    """Parse a VOX file into flattened voxels (x, y, z, colour_index).

    Scene-graph transforms are applied and the result is rebased so that the
    minimum voxel corner is (0, 0, 0) in VOX (Z-up) space.
    """
    if len(data) < 20 or data[:4] != b"VOX ":
        raise VoxFormatError("missing 'VOX ' signature")
    (version,) = struct.unpack_from("<I", data, 4)
    if not 150 <= version < 300:
        raise VoxFormatError(f"unsupported VOX version {version}")
    cid, content_size, children_size = struct.unpack_from("<4sII", data, 8)
    if cid != b"MAIN":
        raise VoxFormatError("first chunk is not MAIN")
    pos = 20 + content_size
    end = pos + children_size
    if end != len(data):
        raise VoxFormatError("MAIN chunk size does not match the file size")

    models: list[tuple[tuple[int, int, int], list[tuple[int, int, int, int]]]] = []
    nodes: dict[int, tuple[Any, ...]] = {}
    pending: Optional[tuple[int, int, int]] = None
    total = 0
    while pos < end:
        if pos + 12 > end:
            raise VoxFormatError("truncated chunk header")
        cid, size, child_size = struct.unpack_from("<4sII", data, pos)
        body_start = pos + 12
        body_end = body_start + size
        if body_end + child_size > end:
            raise VoxFormatError(f"chunk '{cid.decode('ascii', 'replace')}' exceeds the file")
        body = data[body_start:body_end]
        pos = body_end + child_size
        if cid == b"SIZE":
            if size != 12:
                raise VoxFormatError("SIZE chunk must be 12 bytes")
            pending = struct.unpack("<iii", body)  # type: ignore[assignment]
            if min(pending) <= 0:  # type: ignore[arg-type]
                raise VoxFormatError("model dimensions must be positive")
        elif cid == b"XYZI":
            if pending is None:
                raise VoxFormatError("XYZI chunk without a preceding SIZE chunk")
            if size < 4:
                raise VoxFormatError("XYZI chunk too small")
            (count,) = struct.unpack_from("<I", body, 0)
            if size != 4 + 4 * count:
                raise VoxFormatError("XYZI voxel count does not match chunk size")
            total += count
            if total > MAX_VOX_VOXELS:
                raise VoxFormatError(f"more than {MAX_VOX_VOXELS} voxels")
            voxels = list(struct.iter_unpack("<BBBB", body[4:]))
            for x, y, z, colour in voxels:
                if x >= pending[0] or y >= pending[1] or z >= pending[2]:
                    raise VoxFormatError("voxel lies outside its model's SIZE")
                if colour == 0:
                    raise VoxFormatError("voxel uses colour index 0")
            models.append((pending, voxels))
            pending = None
        elif cid in (b"nTRN", b"nGRP", b"nSHP"):
            reader = _VoxReader(body)
            node_id = reader.i32()
            if node_id in nodes:
                raise VoxFormatError(f"duplicate scene node {node_id}")
            attrs = reader.dict()
            if cid == b"nTRN":
                child = reader.i32()
                reader.i32()  # reserved
                reader.i32()  # layer id
                frame_count = reader.i32()
                if frame_count < 1 or frame_count > 100_000:
                    raise VoxFormatError("invalid transform frame count")
                frames = [reader.dict() for _ in range(frame_count)]
                nodes[node_id] = ("T", attrs, child, frames[0])
            elif cid == b"nGRP":
                count = reader.i32()
                if not 0 <= count <= 1_000_000:
                    raise VoxFormatError("invalid group child count")
                nodes[node_id] = ("G", [reader.i32() for _ in range(count)])
            else:
                count = reader.i32()
                if not 0 <= count <= 1_000_000:
                    raise VoxFormatError("invalid shape model count")
                ids = []
                for _ in range(count):
                    ids.append(reader.i32())
                    reader.dict()
                nodes[node_id] = ("S", ids)
        # RGBA, MATL, LAYR, rOBJ, rCAM, NOTE, IMAP ... are not needed.

    result: list[tuple[int, int, int, int]] = []
    if not nodes:
        if len(models) != 1:
            raise VoxFormatError("expected exactly one model when no scene graph is present")
        result = list(models[0][1])
    else:
        identity = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]

        def walk(node_id: int, rot: list[list[int]], trans: tuple[int, int, int],
                 depth: int) -> None:
            if depth > 64:
                raise VoxFormatError("scene graph is nested too deeply (cycle?)")
            node = nodes.get(node_id)
            if node is None:
                raise VoxFormatError(f"scene graph references missing node {node_id}")
            if node[0] == "T":
                _, attrs, child, frame = node
                if attrs.get("_hidden") == "1":
                    return
                local_rot = _vox_rotation(frame.get("_r"))
                local_t = _vox_translation(frame.get("_t"))
                moved = _mat_vec(rot, local_t)
                walk(child, _mat_mul(rot, local_rot),
                     (moved[0] + trans[0], moved[1] + trans[1], moved[2] + trans[2]),
                     depth + 1)
            elif node[0] == "G":
                for child in node[1]:
                    walk(child, rot, trans, depth + 1)
            else:
                for model_id in node[1]:
                    if not 0 <= model_id < len(models):
                        raise VoxFormatError(f"shape references missing model {model_id}")
                    size, voxels = models[model_id]
                    pivot = (size[0] // 2, size[1] // 2, size[2] // 2)
                    for x, y, z, colour in voxels:
                        qx, qy, qz = _mat_vec(rot, (x - pivot[0], y - pivot[1], z - pivot[2]))
                        result.append((qx + trans[0], qy + trans[1], qz + trans[2], colour))
                        if len(result) > MAX_VOX_VOXELS:
                            raise VoxFormatError(f"more than {MAX_VOX_VOXELS} voxels")

        walk(0, identity, (0, 0, 0), 0)
    if not result:
        raise VoxFormatError("the file contains no voxels")
    mins = [min(v[axis] for v in result) for axis in range(3)]
    return [(x - mins[0], y - mins[1], z - mins[2], c) for x, y, z, c in result]


# ===========================================================================
# World binary (WRLD) encoder / decoder
# ===========================================================================

Voxel = tuple[int, int, int]


def _rle_encode(cells: Sequence[int], wide: bool) -> bytes:
    entry = struct.Struct("<HH" if wide else "<HB")
    out = bytearray()
    run_value, run_len = cells[0], 0
    for value in cells:
        if value == run_value:
            run_len += 1
        else:
            out += entry.pack(run_len, run_value)
            run_value, run_len = value, 1
    out += entry.pack(run_len, run_value)
    return bytes(out)


def encode_world(size: Voxel, voxels: Mapping[Voxel, str]) -> bytes:
    """Encode bounds-local voxels as OopsMath Binary Voxel World v1.

    Layout: header | palette (u16 length + UTF-8 per entry, index 0 = "air") |
    chunk records.  Cell order inside a chunk is x fastest, then y, then z.
    """
    palette = [AIR_BLOCK] + sorted(set(voxels.values()))
    if len(palette) > 0xFFFF:
        raise StageCompileError("too many distinct blocks in the world palette")
    index = {name: i for i, name in enumerate(palette)}
    wide = len(palette) > 256
    chunks: dict[Voxel, dict[int, int]] = {}
    for (x, y, z), name in voxels.items():
        key = (x // WORLD_CHUNK_SIZE, y // WORLD_CHUNK_SIZE, z // WORLD_CHUNK_SIZE)
        cell = ((z % 16) * 16 + (y % 16)) * 16 + (x % 16)
        chunks.setdefault(key, {})[cell] = index[name]
    records = bytearray()
    for key in sorted(chunks):
        cells = [0] * WORLD_CHUNK_VOLUME
        for cell, palette_index in chunks[key].items():
            cells[cell] = palette_index
        dense = struct.pack(f"<{WORLD_CHUNK_VOLUME}H", *cells) if wide else bytes(cells)
        rle = _rle_encode(cells, wide)
        encoding, payload = (1, rle) if len(rle) < len(dense) else (0, dense)
        records += WORLD_CHUNK_RECORD.pack(*key, encoding, len(payload)) + payload
    header = WORLD_HEADER.pack(WORLD_MAGIC, WORLD_VERSION, WORLD_CHUNK_SIZE,
                               size[0], size[1], size[2], len(palette), len(chunks))
    palette_bytes = b"".join(struct.pack("<H", len(b)) + b
                             for b in (n.encode("utf-8") for n in palette))
    return header + palette_bytes + bytes(records)


def decode_world(blob: bytes) -> tuple[Voxel, list[str], dict[Voxel, str]]:
    """Decode WRLD (used by self-test and inspect)."""
    try:
        (magic, version, chunk, sx, sy, sz, pal_count,
         chunk_count) = WORLD_HEADER.unpack_from(blob, 0)
        if magic != WORLD_MAGIC or version != WORLD_VERSION or chunk != WORLD_CHUNK_SIZE:
            raise DatFormatError("unsupported WRLD header")
        pos = WORLD_HEADER.size
        palette: list[str] = []
        for _ in range(pal_count):
            (length,) = struct.unpack_from("<H", blob, pos)
            pos += 2
            palette.append(blob[pos:pos + length].decode("utf-8"))
            pos += length
        wide = pal_count > 256
        voxels: dict[Voxel, str] = {}
        for _ in range(chunk_count):
            cx, cy, cz, encoding, size = WORLD_CHUNK_RECORD.unpack_from(blob, pos)
            pos += WORLD_CHUNK_RECORD.size
            payload = blob[pos:pos + size]
            pos += size
            if encoding == 0:
                cells = (list(struct.unpack(f"<{WORLD_CHUNK_VOLUME}H", payload)) if wide
                         else list(payload))
            elif encoding == 1:
                cells = []
                for run, value in struct.iter_unpack("<HH" if wide else "<HB", payload):
                    cells.extend([value] * run)
            else:
                raise DatFormatError(f"unknown chunk encoding {encoding}")
            if len(cells) != WORLD_CHUNK_VOLUME:
                raise DatFormatError("chunk does not contain 4096 cells")
            for cell, value in enumerate(cells):
                if value:
                    x, y, z = cell % 16, (cell // 16) % 16, cell // 256
                    voxels[(cx * 16 + x, cy * 16 + y, cz * 16 + z)] = palette[value]
        return (sx, sy, sz), palette, voxels
    except (struct.error, UnicodeDecodeError, IndexError) as exc:
        raise DatFormatError(f"corrupt WRLD section: {exc}") from exc


# ===========================================================================
# Stage source model (output of validation)
# ===========================================================================

@dataclass(frozen=True)
class LocaleFile:
    locale: str
    rel_path: str
    raw: bytes
    keys: frozenset[str]


@dataclass(frozen=True)
class DocFile:
    rel_path: str
    text: str


@dataclass(frozen=True)
class AssetFile:
    asset_id: str
    rel_path: str
    data: bytes
    sha256: str


@dataclass(frozen=True)
class WorldData:
    size: Voxel
    origin: Voxel
    voxels: dict[Voxel, str]


@dataclass
class StageSource:
    data: dict[str, Any]
    locales: list[LocaleFile]
    docs: dict[str, DocFile]
    assets: list[AssetFile]
    world: Optional[WorldData]


@dataclass
class ValidationResult:
    source: Optional[StageSource]
    diag: Diagnostics
    passed: list[str]
    stage_id: Optional[str]


# ===========================================================================
# Validation layer 2: schema
# ===========================================================================

def format_path(parts: Sequence[Any]) -> str:
    out = ""
    for part in parts:
        if isinstance(part, int):
            out += f"[{part}]"
        else:
            out += ("." if out else "") + str(part)
    return out or "<root>"


def validate_schema(data: dict[str, Any], diag: Diagnostics) -> None:
    validator = jsonschema.Draft202012Validator(STAGE_SCHEMA)
    errors = sorted(validator.iter_errors(data),
                    key=lambda e: [str(p) for p in e.absolute_path])
    for err in errors:
        message = err.message if len(err.message) <= 240 else err.message[:237] + "..."
        diag.error(f"{format_path(list(err.absolute_path))}: {message}")


# ===========================================================================
# Validation layer 3: references
# ===========================================================================

@dataclass
class RefIndex:
    task_ids: set[str]
    target_ids: set[str]
    test_ids: set[str]


def _unique_ids(items: Sequence[Mapping[str, Any]], path: str, label: str,
                diag: Diagnostics) -> set[str]:
    first: dict[str, int] = {}
    for i, item in enumerate(items):
        ident = item["id"]
        if ident in first:
            diag.error(f"{path}[{i}].id duplicates {label} id '{ident}' "
                       f"(first declared at {path}[{first[ident]}])")
        else:
            first[ident] = i
    return set(first)


def _check_region(region: Mapping[str, Any], path: str, diag: Diagnostics) -> None:
    for axis, name in enumerate("xyz"):
        if region["min"][axis] > region["max"][axis]:
            diag.error(f"{path} has min greater than max on the {name} axis")


def validate_condition(cond: Any, path: str, index: RefIndex, diag: Diagnostics,
                       depth: int = 0) -> None:
    """Validate the structure and references of a runtime condition (never evaluates it)."""
    if depth > MAX_CONDITION_DEPTH:
        diag.error(f"{path}: condition is nested deeper than {MAX_CONDITION_DEPTH} levels")
        return
    if not isinstance(cond, dict):
        diag.error(f"{path}: a condition must be an object")
        return
    if len(cond) != 1:
        diag.error(f"{path}: a condition must contain exactly one form "
                   f"({', '.join(CONDITION_FORMS)}), found {sorted(cond)}")
        return
    ((form, arg),) = cond.items()
    here = f"{path}.{form}"
    if form in ("all", "any"):
        if not isinstance(arg, list) or not arg:
            diag.error(f"{here} must be a non-empty list of conditions")
            return
        for i, sub in enumerate(arg):
            validate_condition(sub, f"{here}[{i}]", index, diag, depth + 1)
    elif form == "not":
        validate_condition(arg, here, index, diag, depth + 1)
    elif form in ("task_completed", "target_completed", "physics_test_passed"):
        known, label = {"task_completed": (index.task_ids, "task"),
                        "target_completed": (index.target_ids, "construction target"),
                        "physics_test_passed": (index.test_ids, "physics test")}[form]
        if not isinstance(arg, str):
            diag.error(f"{here} must be an ID string")
        elif arg not in known:
            diag.error(f"{here} references unknown {label} '{arg}'")
    elif form == "structure_stable":
        if not isinstance(arg, (bool, dict)):
            diag.error(f"{here} must be a boolean or an object")
    elif form == "compare":
        if not isinstance(arg, dict):
            diag.error(f"{here} must be an object with metric, operator and value")
            return
        extra = sorted(set(arg) - {"metric", "operator", "value"})
        if extra:
            diag.error(f"{here} has unexpected properties {extra}")
        if not isinstance(arg.get("metric"), str) or not arg.get("metric"):
            diag.error(f"{here}.metric must be a non-empty string")
        if arg.get("operator") not in COMPARE_OPERATORS:
            diag.error(f"{here}.operator must be one of {', '.join(COMPARE_OPERATORS)}")
        if "value" not in arg:
            diag.error(f"{here}.value is required")
    else:
        diag.error(f"{path}: unknown condition form '{form}' "
                   f"(expected one of {', '.join(CONDITION_FORMS)})")


def _check_registered(kind: str, known: Optional[frozenset[str]], value: str,
                      path: str, diag: Diagnostics) -> None:
    if known is not None and value not in known:
        diag.error(f"{path} references unknown {kind} '{value}'")


def _task_cycle(tasks: Sequence[Mapping[str, Any]]) -> list[str]:
    """Return task IDs that sit on (or behind) a dependency cycle (Kahn's algorithm)."""
    ids = {t["id"] for t in tasks}
    pending = {t["id"]: {d for d in t.get("depends_on", []) if d in ids and d != t["id"]}
               for t in tasks}
    progressed = True
    while progressed and pending:
        ready = [tid for tid, deps in pending.items() if not deps]
        progressed = bool(ready)
        for tid in ready:
            del pending[tid]
        for deps in pending.values():
            deps.difference_update(ready)
    return sorted(pending)


def validate_references(data: dict[str, Any], opts: CompileOptions, diag: Diagnostics) -> None:
    reg = opts.manifest or Registry()
    stage, learning, world = data["stage"], data["learning"], data["world"]
    construction, objectives, rewards = data["construction"], data["objectives"], data["rewards"]
    stage_id = stage["id"]

    _unique_ids(data.get("story", {}).get("dialogue", []), "story.dialogue", "dialogue", diag)
    _unique_ids(learning.get("hints", []), "learning.hints", "hint", diag)
    instances = world.get("initial_structure", {}).get("blocks", [])
    _unique_ids(instances, "world.initial_structure.blocks", "block instance", diag)
    custom = data.get("assets", {}).get("custom", [])
    asset_ids = _unique_ids(custom, "assets.custom", "asset", diag)
    targets = construction.get("targets", [])
    tests = data.get("physics", {}).get("tests", [])
    index = RefIndex(
        task_ids=_unique_ids(objectives["tasks"], "objectives.tasks", "task", diag),
        target_ids=_unique_ids(targets, "construction.targets", "target", diag),
        test_ids=_unique_ids(tests, "physics.tests", "physics test", diag))
    _unique_ids(objectives.get("checkpoints", []), "objectives.checkpoints", "checkpoint", diag)
    _unique_ids(data.get("events", []), "events", "event", diag)
    allowed_blocks = construction.get("allowed_blocks", [])
    allowed_tools = construction.get("allowed_tools", [])
    allowed_block_ids = _unique_ids(allowed_blocks, "construction.allowed_blocks",
                                    "allowed block", diag)
    allowed_tool_ids = _unique_ids(allowed_tools, "construction.allowed_tools",
                                   "allowed tool", diag)

    # --- stage ID references ------------------------------------------------
    def stage_ref(value: str, path: str, forbid_self: bool) -> None:
        if value == stage_id:
            (diag.error if forbid_self else diag.warn)(f"{path} references the stage itself ('{value}')")
        elif opts.known_stage_ids is not None and value not in opts.known_stage_ids:
            diag.warn(f"{path} references stage '{value}' which was not found among the stages being built")

    for i, ref_id in enumerate(stage.get("requirements", {}).get("stages", [])):
        stage_ref(ref_id, f"stage.requirements.stages[{i}]", True)
    tutorial = learning.get("tutorial_stage", {})
    if tutorial.get("enabled") and "stage_id" not in tutorial:
        diag.error("learning.tutorial_stage.stage_id is required when the tutorial stage is enabled")
    if "stage_id" in tutorial:
        stage_ref(tutorial["stage_id"], "learning.tutorial_stage.stage_id", True)
    unlocks = rewards.get("unlocks", {})
    for i, ref_id in enumerate(unlocks.get("stages", [])):
        stage_ref(ref_id, f"rewards.unlocks.stages[{i}]", False)

    grade = learning["curriculum"]["grade"]
    if grade["min"] > grade["max"]:
        diag.error("learning.curriculum.grade.min must not exceed grade.max")

    # --- world ---------------------------------------------------------------
    bounds = world["bounds"]
    size, origin = bounds["size"], bounds.get("origin", [0, 0, 0])
    seen_positions: dict[tuple[int, ...], int] = {}
    for i, block in enumerate(instances):
        base = f"world.initial_structure.blocks[{i}]"
        pos = tuple(block["position"])
        if block["block"] == AIR_BLOCK:
            diag.error(f"{base}.block: 'air' is reserved and cannot be placed")
        if any(not origin[a] <= pos[a] < origin[a] + size[a] for a in range(3)):
            diag.error(f"{base}.position {list(pos)} is outside the world bounds")
        if pos in seen_positions:
            diag.error(f"{base}.position {list(pos)} is already used by "
                       f"world.initial_structure.blocks[{seen_positions[pos]}]")
        else:
            seen_positions[pos] = i

    # --- construction --------------------------------------------------------
    for i, entry in enumerate(allowed_blocks):
        if entry.get("min_count", 0) > entry.get("max_count", entry.get("min_count", 0)):
            diag.error(f"construction.allowed_blocks[{i}].min_count exceeds max_count")
    constraints = construction.get("constraints", {})
    for name, limit in constraints.get("material_limits", {}).items():
        if limit.get("min", 0) > limit.get("max", limit.get("min", 0)):
            diag.error(f"construction.constraints.material_limits.{name}: min exceeds max")
    for i, tgt in enumerate(targets):
        if "region" in tgt:
            _check_region(tgt["region"], f"construction.targets[{i}].region", diag)
    for group in ("allowed_regions", "forbidden_regions"):
        for i, region in enumerate(constraints.get(group, [])):
            _check_region(region, f"construction.constraints.{group}[{i}]", diag)

    inventory = construction.get("starting_inventory", {})
    if allowed_blocks:
        for name in inventory.get("blocks", {}):
            if name not in allowed_block_ids:
                diag.warn(f"construction.starting_inventory.blocks.{name} is not listed in allowed_blocks")
        for i, tgt in enumerate(targets):
            rb = tgt.get("required_block")
            if rb and rb not in allowed_block_ids:
                diag.warn(f"construction.targets[{i}].required_block '{rb}' is not listed in allowed_blocks")
    if allowed_tools:
        for name in inventory.get("tools", {}):
            if name not in allowed_tool_ids:
                diag.warn(f"construction.starting_inventory.tools.{name} is not listed in allowed_tools")

    # --- built-in block / tool / asset registries ------------------------------
    block_refs = [(f"construction.allowed_blocks[{i}].id", b["id"]) for i, b in enumerate(allowed_blocks)]
    block_refs += [(f"construction.starting_inventory.blocks.{k}", k) for k in inventory.get("blocks", {})]
    block_refs += [(f"construction.targets[{i}].required_block", t["required_block"])
                   for i, t in enumerate(targets) if "required_block" in t]
    block_refs += [(f"construction.constraints.material_limits.{k}", k)
                   for k in constraints.get("material_limits", {})]
    block_refs += [(f"world.initial_structure.blocks[{i}].block", b["block"]) for i, b in enumerate(instances)]
    block_refs += [(f"rewards.unlocks.blocks[{i}]", b) for i, b in enumerate(unlocks.get("blocks", []))]
    for path, value in block_refs:
        _check_registered("block", reg.blocks, value, path, diag)
    tool_refs = [(f"construction.allowed_tools[{i}].id", t["id"]) for i, t in enumerate(allowed_tools)]
    tool_refs += [(f"construction.starting_inventory.tools.{k}", k) for k in inventory.get("tools", {})]
    tool_refs += [(f"rewards.unlocks.tools[{i}]", t) for i, t in enumerate(unlocks.get("tools", []))]
    for path, value in tool_refs:
        _check_registered("tool", reg.tools, value, path, diag)

    if reg.assets is not None:
        known_assets = reg.assets | asset_ids
        for i, value in enumerate(unlocks.get("assets", [])):
            _check_registered("asset", known_assets, value, f"rewards.unlocks.assets[{i}]", diag)
    audio = data.get("audio", {})
    audio_pool = [p for p in (reg.audio, reg.assets) if p is not None]
    if audio_pool:
        known_audio = frozenset().union(*audio_pool) | asset_ids
        audio_refs = [(f"audio.{k}", audio[k]) for k in ("music", "ambience") if k in audio]
        audio_refs += [(f"audio.sounds.{k}", v) for k, v in audio.get("sounds", {}).items()]
        for path, value in audio_refs:
            _check_registered("audio asset", known_audio, value, path, diag)

    # --- physics ---------------------------------------------------------------
    physics = data.get("physics", {})
    if tests and not physics.get("enabled", False):
        diag.warn("physics.tests are declared but physics.enabled is false")
    for i, test in enumerate(tests):
        load = test.get("load", {})
        target = load.get("target", {})
        base = f"physics.tests[{i}].load.target"
        if "id" in target:
            pool = index.target_ids | asset_ids | {b["id"] for b in instances}
            if target["id"] not in pool:
                (diag.warn if target.get("type") == "body" else diag.error)(
                    f"{base}.id references unknown construction target, asset or block instance '{target['id']}'")
        if "region" in target:
            _check_region(target["region"], f"{base}.region", diag)

    # --- objectives ------------------------------------------------------------
    for i, task in enumerate(objectives["tasks"]):
        base = f"objectives.tasks[{i}]"
        for j, dep in enumerate(task.get("depends_on", [])):
            if dep == task["id"]:
                diag.error(f"{base}.depends_on[{j}] makes the task depend on itself")
            elif dep not in index.task_ids:
                diag.error(f"{base}.depends_on[{j}] references unknown task '{dep}'")
        if "target" in task and task["target"] not in index.target_ids:
            diag.error(f"{base}.target references unknown construction target '{task['target']}'")
        if task["type"] == "construction" and "target" not in task:
            diag.warn(f"{base} is a construction task without a target")
        math = task.get("math")
        if task["type"] == "math" and math is None:
            diag.error(f"{base}.math is required for math tasks")
        elif math is not None:
            if task["type"] != "math":
                diag.warn(f"{base}.math is declared on a '{task['type']}' task")
            _check_math_answer(math, f"{base}.math", diag)
    cyc = _task_cycle(objectives["tasks"])
    if cyc:
        diag.error(f"objectives.tasks contain a dependency cycle involving: {', '.join(cyc)}")
    for i, cp in enumerate(objectives.get("checkpoints", [])):
        if "after_task" in cp and cp["after_task"] not in index.task_ids:
            diag.error(f"objectives.checkpoints[{i}].after_task references unknown task '{cp['after_task']}'")

    validate_condition(objectives["completion"], "objectives.completion", index, diag)
    if "failure" in objectives:
        validate_condition(objectives["failure"], "objectives.failure", index, diag)
    for i, event in enumerate(data.get("events", [])):
        if "condition" in event["trigger"]:
            validate_condition(event["trigger"]["condition"],
                               f"events[{i}].trigger.condition", index, diag)
    for i, line in enumerate(data.get("story", {}).get("dialogue", [])):
        if "condition" in line:
            validate_condition(line["condition"], f"story.dialogue[{i}].condition", index, diag)


def _check_math_answer(math: Mapping[str, Any], path: str, diag: Diagnostics) -> None:
    answer = math.get("answer")
    if answer is None:
        return
    mode, generated = answer["mode"], "generator" in math
    if mode in ("exact", "tolerance", "multiple_choice") and "value" not in answer and not generated:
        diag.error(f"{path}.answer.value is required for mode '{mode}' without a generator")
    if mode == "tolerance" and "tolerance" not in answer and not generated:
        diag.error(f"{path}.answer.tolerance is required for mode 'tolerance' without a generator")
    if mode == "expression" and "expression" not in answer and "expression" not in math:
        diag.error(f"{path}.answer.expression (or {path}.expression) is required for mode 'expression'")


# ===========================================================================
# Validation layer 4: files (assets, world, documents, localization)
# ===========================================================================

def _resolve_file(stage_dir: Path, rel: str, yaml_path: str, diag: Diagnostics,
                  *, extensions: Sequence[str], label: str,
                  limit: int) -> Optional[tuple[Path, str]]:
    try:
        path, normalized = resolve_safe_path(stage_dir, rel)
    except UnsafePathError as exc:
        diag.error(f"{yaml_path}: unsafe path '{rel}': {exc}")
        return None
    if not path.is_file():
        diag.error(f"{yaml_path} references missing file '{normalized}'")
        return None
    if path.suffix.lower() not in extensions:
        diag.error(f"{yaml_path}: '{normalized}' must have a {'/'.join(extensions)} extension ({label})")
        return None
    try:
        if path.stat().st_size > limit:
            diag.error(f"{normalized}: file is larger than the {limit}-byte limit")
            return None
    except OSError as exc:
        diag.error(f"{normalized}: cannot be read: {exc}")
        return None
    return path, normalized


def validate_glb(data: bytes, rel: str, diag: Diagnostics) -> bool:
    if len(data) < 20 or data[:4] != GLB_MAGIC:
        diag.error(f"{rel}: not a binary glTF (missing 'glTF' magic)")
        return False
    version, length = struct.unpack_from("<II", data, 4)
    if version != 2:
        diag.error(f"{rel}: unsupported GLB version {version} (expected 2)")
        return False
    if length != len(data):
        diag.error(f"{rel}: GLB header length {length} does not match file size {len(data)}")
        return False
    _chunk_len, chunk_type = struct.unpack_from("<II", data, 12)
    if chunk_type != GLB_JSON_CHUNK:
        diag.error(f"{rel}: first GLB chunk is not JSON")
        return False
    return True


def load_custom_assets(stage_dir: Path, data: dict[str, Any],
                       diag: Diagnostics) -> list[AssetFile]:
    assets: list[AssetFile] = []
    for i, entry in enumerate(data.get("assets", {}).get("custom", [])):
        found = _resolve_file(stage_dir, entry["file"], f"assets.custom[{i}].file", diag,
                              extensions=(".glb",), label="GLB asset", limit=MAX_BINARY_FILE_BYTES)
        if found is None:
            continue
        path, normalized = found
        try:
            blob = path.read_bytes()
        except OSError as exc:
            diag.error(f"{normalized}: cannot be read: {exc}")
            continue
        if validate_glb(blob, normalized, diag):
            assets.append(AssetFile(entry["id"], normalized, blob, hashlib.sha256(blob).hexdigest()))
    return sorted(assets, key=lambda a: a.asset_id)


def build_world(stage_dir: Path, data: dict[str, Any], opts: CompileOptions,
                diag: Diagnostics) -> Optional[WorldData]:
    """Compile the VOX source and YAML initial blocks into bounds-local voxels."""
    world = data["world"]
    size = tuple(world["bounds"]["size"])
    origin = tuple(world["bounds"].get("origin", [0, 0, 0]))
    source = world.get("source", {})
    instances = world.get("initial_structure", {}).get("blocks", [])
    voxels: dict[Voxel, str] = {}

    def in_bounds(p: Voxel) -> bool:
        return all(0 <= p[a] < size[a] for a in range(3))

    if "voxel_file" in source:
        found = _resolve_file(stage_dir, source["voxel_file"], "world.source.voxel_file", diag,
                              extensions=(".vox",), label="MagicaVoxel file",
                              limit=MAX_BINARY_FILE_BYTES)
        if found is not None:
            path, normalized = found
            try:
                raw_voxels = parse_vox(path.read_bytes())
            except OSError as exc:
                diag.error(f"{normalized}: cannot be read: {exc}")
                raw_voxels = []
            except VoxFormatError as exc:
                diag.error(f"{normalized}: invalid VOX file: {exc}")
                raw_voxels = []
            palette = source.get("palette", {})
            default = source.get("default_block")
            registry = (opts.manifest or Registry()).blocks
            for name in sorted(set(palette.values()) | ({default} if default else set())):
                if name == AIR_BLOCK:
                    diag.error("world.source: 'air' is reserved and cannot be a palette block")
                _check_registered("block", registry, name, "world.source.palette", diag)
            unmapped = sorted({c for *_p, c in raw_voxels if str(c) not in palette} if default is None else set())
            if unmapped:
                diag.error(f"{normalized} uses palette indices {unmapped[:12]}"
                           f"{'...' if len(unmapped) > 12 else ''} that have no entry in "
                           "world.source.palette and no world.source.default_block is set")
            offset = source.get("offset", list(origin))
            outside = 0
            first_outside: Optional[Voxel] = None
            for vx, vy, vz, colour in raw_voxels:
                block = palette.get(str(colour), default)
                if block is None:
                    continue
                # VOX is Z-up; OopsMath is Y-up: (x, y, z) -> (x, z, y).
                absolute = (vx + offset[0], vz + offset[1], vy + offset[2])
                local = (absolute[0] - origin[0], absolute[1] - origin[1], absolute[2] - origin[2])
                if in_bounds(local):
                    voxels[local] = block
                else:
                    outside += 1
                    first_outside = first_outside or absolute
            if outside:
                diag.error(f"{normalized}: {outside} voxel(s) fall outside world.bounds "
                           f"(first at {list(first_outside)}); adjust world.source.offset or the bounds")
    elif "palette" in source or "default_block" in source or "offset" in source:
        diag.warn("world.source defines palette/offset settings but no voxel_file")

    overridden = 0
    for block in instances:
        local = tuple(block["position"][a] - origin[a] for a in range(3))
        if not in_bounds(local):
            continue  # already reported as a reference error
        if local in voxels:
            overridden += 1
        voxels[local] = block["block"]
    if overridden:
        diag.warn(f"{overridden} initial_structure block(s) override voxels from the VOX file")
    if not voxels:
        return None
    return WorldData(size, origin, voxels)  # type: ignore[arg-type]


def collect_markdown_refs(data: dict[str, Any]) -> list[tuple[str, str]]:
    learning = data["learning"]
    refs: list[tuple[str, str]] = []
    if "markdown" in learning.get("lesson", {}):
        refs.append(("learning.lesson.markdown", learning["lesson"]["markdown"]))
    if "markdown" in learning.get("solution", {}):
        refs.append(("learning.solution.markdown", learning["solution"]["markdown"]))
    for i, hint in enumerate(learning.get("hints", [])):
        if "markdown" in hint:
            refs.append((f"learning.hints[{i}].markdown", hint["markdown"]))
    return refs


def load_documents(stage_dir: Path, data: dict[str, Any], diag: Diagnostics) -> dict[str, DocFile]:
    docs: dict[str, DocFile] = {}
    for yaml_path, rel in collect_markdown_refs(data):
        found = _resolve_file(stage_dir, rel, yaml_path, diag, extensions=(".md", ".markdown"),
                              label="Markdown", limit=MAX_TEXT_FILE_BYTES)
        if found is None:
            continue
        path, normalized = found
        if normalized in docs:
            continue
        try:
            text = path.read_bytes().decode("utf-8")
        except UnicodeDecodeError:
            diag.error(f"{normalized}: not valid UTF-8")
            continue
        except OSError as exc:
            diag.error(f"{normalized}: cannot be read: {exc}")
            continue
        if "\x00" in text:
            diag.error(f"{normalized}: contains NUL characters")
            continue
        docs[normalized] = DocFile(normalized, text)
    return docs


def discover_locale_files(stage_dir: Path, data: dict[str, Any],
                          diag: Diagnostics) -> list[tuple[str, Path, str]]:
    """Return (locale, filesystem path, relative path) for every stage FTL file."""
    configured = data.get("localization", {}).get("ftl")
    rel_dir_or_file = configured if configured is not None else "localization"
    try:
        path, normalized = resolve_safe_path(stage_dir, rel_dir_or_file)
    except UnsafePathError as exc:
        diag.error(f"localization.ftl: unsafe path '{rel_dir_or_file}': {exc}")
        return []
    candidates: list[tuple[Path, str]] = []
    if path.is_file():
        if path.suffix.lower() != ".ftl":
            diag.error(f"localization.ftl: '{normalized}' must be an .ftl file or a directory")
            return []
        candidates = [(path, normalized)]
    elif path.is_dir():
        for child in sorted(path.glob("*.ftl"), key=lambda p: p.name):
            candidates.append((child, f"{normalized}/{child.name}"))
        if configured is not None and not candidates:
            diag.error(f"localization.ftl: directory '{normalized}' contains no .ftl files")
    elif configured is not None:
        diag.error(f"localization.ftl references missing file or directory '{normalized}'")
        return []
    found: list[tuple[str, Path, str]] = []
    seen: dict[str, str] = {}
    for child, rel in candidates:
        try:
            resolve_safe_path(stage_dir, rel)
        except UnsafePathError as exc:
            diag.error(f"{rel}: {exc}")
            continue
        locale = child.stem
        if not LOCALE_RE.match(locale):
            diag.error(f"{rel}: '{locale}' is not a valid locale name (expected e.g. 'fa' or 'en-US')")
            continue
        if locale in seen:
            diag.error(f"{rel}: locale '{locale}' is already provided by '{seen[locale]}'")
            continue
        seen[locale] = rel
        found.append((locale, child, rel))
    return sorted(found, key=lambda item: item[0])


def collect_localization_refs(data: dict[str, Any]) -> list[tuple[str, str]]:
    stage, learning = data["stage"], data["learning"]
    refs = [("stage.title", stage["title"])]
    if "description" in stage:
        refs.append(("stage.description", stage["description"]))
    for i, line in enumerate(data.get("story", {}).get("dialogue", [])):
        refs.append((f"story.dialogue[{i}].text", line["text"]))
    refs.append(("learning.question", learning["question"]))
    if "explanation" in learning.get("solution", {}):
        refs.append(("learning.solution.explanation", learning["solution"]["explanation"]))
    for i, hint in enumerate(learning.get("hints", [])):
        refs.append((f"learning.hints[{i}].text", hint["text"]))
    return refs


def load_and_validate_localization(stage_dir: Path, data: dict[str, Any], opts: CompileOptions,
                                   diag: Diagnostics) -> list[LocaleFile]:
    locales: list[LocaleFile] = []
    for locale, path, rel in discover_locale_files(stage_dir, data, diag):
        try:
            raw = _read_limited(path, MAX_TEXT_FILE_BYTES)
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            diag.error(f"{rel}: not valid UTF-8")
            continue
        except (OSError, SourceDataError) as exc:
            diag.error(f"{rel}: cannot be read: {exc}")
            continue
        locales.append(LocaleFile(locale, rel, raw, parse_ftl(text, rel, diag)))

    base = opts.base_keys
    namespace = data.get("localization", {}).get("namespace")
    unverifiable = 0
    for yaml_path, key in collect_localization_refs(data):
        if base is not None and key in base:
            continue
        if not locales and base is None:
            unverifiable += 1
            continue
        missing_in = [loc.locale for loc in locales if key not in loc.keys]
        if locales and not missing_in:
            continue
        if not locales or len(missing_in) == len(locales):
            hint = "" if base is not None else \
                " (no --base-locales were given; pass them if this key is global)"
            diag.error(f"localization key '{key}' is missing from stage FTL and base localization "
                       f"(referenced at {yaml_path}){hint}")
        else:
            diag.warn(f"localization key '{key}' (referenced at {yaml_path}) is missing "
                      f"from locale(s): {', '.join(missing_in)}")
    if unverifiable:
        diag.warn(f"{unverifiable} localization key reference(s) could not be verified: "
                  "the stage has no FTL files and no --base-locales were given")
    for loc in locales:
        for key in sorted(loc.keys):
            if base is not None and key in base:
                diag.warn(f"{loc.rel_path}: stage-local key '{key}' shadows a base game key")
            if namespace and not (key == namespace or key.startswith(namespace + ".")):
                diag.warn(f"{loc.rel_path}: key '{key}' is outside the declared namespace '{namespace}'")
    return locales


# ===========================================================================
# Validation driver
# ===========================================================================

def validate_stage(stage_dir: Path, opts: CompileOptions) -> ValidationResult:
    diag, passed = Diagnostics(), []
    data = load_stage_yaml(stage_dir, diag)
    if data is None:
        return ValidationResult(None, diag, passed, None)
    stage = data.get("stage")
    stage_id = stage.get("id") if isinstance(stage, dict) and isinstance(stage.get("id"), str) else None
    passed.append("Stage parsed")

    validate_schema(data, diag)
    if not diag.ok:
        return ValidationResult(None, diag, passed, stage_id)
    passed.append("Schema valid")

    def layer(label: str, action: Callable[[], Any]) -> Any:
        before = len(diag.errors)
        result = action()
        if len(diag.errors) == before:
            passed.append(label)
        return result

    layer("References valid", lambda: validate_references(data, opts, diag))
    assets = layer("Assets valid", lambda: (load_custom_assets(stage_dir, data, diag),
                                            build_world(stage_dir, data, opts, diag)))
    locales = layer("Localization valid",
                    lambda: load_and_validate_localization(stage_dir, data, opts, diag))
    docs = layer("Documentation valid", lambda: load_documents(stage_dir, data, diag))

    if opts.strict and diag.warnings:
        diag.errors.extend(f"(strict) {w}" for w in diag.warnings)
        diag.warnings.clear()
    source = None
    if diag.ok:
        source = StageSource(data, locales, docs, assets[0], assets[1])
    return ValidationResult(source, diag, passed, stage_id)


# ===========================================================================
# Compilation: source -> sections -> DAT
# ===========================================================================

def build_stag(source: StageSource) -> dict[str, Any]:
    stag = copy.deepcopy(source.data)
    learning = stag["learning"]
    holders = [learning.get("lesson"), learning.get("solution"), *learning.get("hints", [])]
    for holder in holders:
        if isinstance(holder, dict) and "markdown" in holder:
            holder["markdown"] = normalize_rel_path(holder["markdown"])  # becomes a DOCS ID
    world = stag["world"]
    world["bounds"].setdefault("origin", [0, 0, 0])
    voxel_source = world.pop("source", None)
    if voxel_source and "voxel_file" in voxel_source:
        world["source"] = {"voxel": True}
    if "assets" in stag:
        stag["assets"] = {"custom": [{"id": a["id"], "type": a["type"]}
                                     for a in source.data["assets"].get("custom", [])]}
    loc_cfg = stag.pop("localization", {})
    new_loc: dict[str, Any] = {}
    if "namespace" in loc_cfg:
        new_loc["namespace"] = loc_cfg["namespace"]
    if source.locales:
        new_loc["locales"] = [loc.locale for loc in source.locales]
    if new_loc:
        stag["localization"] = new_loc
    return stag


def build_asset_sections(source: StageSource) -> tuple[bytes, bytes]:
    """Return (ASIX MessagePack payload, ASDT raw bytes); identical files share bytes."""
    declared = {a["id"]: a for a in source.data["assets"]["custom"]}
    data_blob = bytearray()
    placed: dict[str, tuple[int, int]] = {}
    entries: list[dict[str, Any]] = []
    for asset in source.assets:  # already sorted by ID
        if asset.sha256 not in placed:
            placed[asset.sha256] = (len(data_blob), len(asset.data))
            data_blob += asset.data
        offset, size = placed[asset.sha256]
        meta = declared[asset.asset_id]
        entry: dict[str, Any] = {"id": asset.asset_id, "type": meta["type"], "mime": GLB_MIME,
                                 "offset": offset, "size": size, "sha256": asset.sha256}
        for optional in ("transform", "behavior"):
            if optional in meta:
                entry[optional] = meta[optional]
        entries.append(entry)
    return pack_msgpack({"version": 1, "assets": entries}), bytes(data_blob)


def compile_sections(source: StageSource) -> list[Section]:
    data = source.data
    stag = build_stag(source)
    body: list[Section] = [Section("STAG", pack_msgpack(stag))]
    if source.world is not None:
        body.append(Section("WRLD", encode_world(source.world.size, source.world.voxels)))
    if source.locales:
        body.append(Section("LOCL", pack_msgpack({loc.locale: loc.raw for loc in source.locales})))
    if source.docs:
        documents = [{"id": rel, "mime": "text/markdown; charset=utf-8", "text": doc.text}
                     for rel, doc in sorted(source.docs.items())]
        body.append(Section("DOCS", pack_msgpack({"version": 1, "documents": documents})))
    if source.assets:
        asix, asdt = build_asset_sections(source)
        body += [Section("ASIX", asix), Section("ASDT", asdt)]

    digest = hashlib.sha256()
    for section in body:
        digest.update(section.type.encode("ascii") + len(section.payload).to_bytes(8, "little"))
        digest.update(section.payload)
    stage, learning = data["stage"], data["learning"]
    meta: dict[str, Any] = {
        "format": DAT_MAGIC.decode("ascii"), "format_version": DAT_FORMAT_VERSION,
        "schema_version": STAGE_SCHEMA_VERSION, "compiler": f"oopsmath_stage/{COMPILER_VERSION}",
        "stage_id": stage["id"], "title": stage["title"],
        "grade_min": learning["curriculum"]["grade"]["min"],
        "grade_max": learning["curriculum"]["grade"]["max"],
        "topics": learning["curriculum"]["topics"],
        "locales": [loc.locale for loc in source.locales],
        "sections": ["META"] + [s.type for s in body],
        "content_sha256": digest.hexdigest(),
    }
    for optional in ("description", "difficulty", "level", "tags"):
        if optional in stage:
            meta[optional] = stage[optional]
    return [Section("META", pack_msgpack(meta))] + body


def compile_stage(source: StageSource, *, debug: bool = False) -> bytes:
    """Produce DAT bytes and verify them before returning (layer 6)."""
    blob = build_dat(compile_sections(source), debug=debug)
    parsed = parse_dat(blob)
    if not parsed.is_valid:
        raise BinaryWriteError("self-verification of the generated DAT failed: " +
                               "; ".join(parsed.problems + [f"{s.type}: {s.status}"
                                                            for s in parsed.sections if s.status != "OK"]))
    return blob


# ===========================================================================
# Command line interface
# ===========================================================================

class _ArgumentParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:  # type: ignore[override]
        self.print_usage(sys.stderr)
        self.exit(EXIT_ARGS, f"{self.prog}: error: {message}\n")


def _configure_stdio() -> None:
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            try:
                reconfigure(encoding="utf-8", errors="replace")
            except ValueError:
                pass  # stream cannot be reconfigured; keep its defaults


def _options_from_args(args: argparse.Namespace) -> CompileOptions:
    manifest = load_manifest(Path(args.manifest)) if args.manifest else None
    base = load_base_locales(Path(args.base_locales)) if args.base_locales else None
    return CompileOptions(manifest=manifest, base_keys=base, strict=args.strict,
                          debug=getattr(args, "debug", False))


def _report(result: ValidationResult) -> None:
    for line in result.passed:
        print(f"\u2714 {line}")
    for warning in result.diag.warnings:
        print(f"WARNING: {warning}", file=sys.stderr)
    for error in result.diag.errors:
        print(f"ERROR: {error}", file=sys.stderr)


def cmd_validate(args: argparse.Namespace) -> int:
    result = validate_stage(Path(args.stage_dir), _options_from_args(args))
    _report(result)
    return EXIT_OK if result.diag.ok else EXIT_VALIDATION


def _build_one(stage_dir: Path, output: Optional[Path], opts: CompileOptions) -> int:
    result = validate_stage(stage_dir, opts)
    _report(result)
    if not result.diag.ok or result.source is None:
        return EXIT_VALIDATION
    blob = compile_stage(result.source, debug=opts.debug)
    print("\u2714 Stage compiled")
    target = output if output is not None else Path(f"{result.stage_id}.dat")
    if target.is_dir():
        target = target / f"{result.stage_id}.dat"
    write_atomic(target, blob)
    print(f"Output: {target}")
    return EXIT_OK


def cmd_build(args: argparse.Namespace) -> int:
    output = Path(args.output) if args.output else None
    return _build_one(Path(args.stage_dir), output, _options_from_args(args))


def discover_stage_dirs(root: Path) -> list[Path]:
    if (root / "stage.yaml").is_file():
        return [root]
    found = sorted((p.parent for p in root.rglob("stage.yaml")),
                   key=lambda p: p.relative_to(root).as_posix())
    return [d for d in found if not any(o != d and o in d.parents for o in found)]


def cmd_build_all(args: argparse.Namespace) -> int:
    root = Path(args.root)
    if not root.is_dir():
        raise ConfigError(f"'{root}' is not a directory")
    opts = _options_from_args(args)
    stage_dirs = discover_stage_dirs(root)
    if not stage_dirs:
        print(f"ERROR: no stage directories (containing stage.yaml) found under '{root}'",
              file=sys.stderr)
        return EXIT_VALIDATION
    ids = {d: peek_stage_id(d) for d in stage_dirs}
    counts: dict[str, int] = {}
    for ident in ids.values():
        if ident:
            counts[ident] = counts.get(ident, 0) + 1
    opts = dataclasses.replace(opts, known_stage_ids=frozenset(counts))
    out_dir = Path(args.output_dir)
    failed = 0
    for stage_dir in stage_dirs:
        print(f"== {stage_dir} ==")
        ident = ids[stage_dir]
        if ident and counts[ident] > 1:
            print(f"ERROR: stage id '{ident}' is used by more than one stage directory",
                  file=sys.stderr)
            failed += 1
            continue
        if _build_one(stage_dir, out_dir, opts) != EXIT_OK:
            failed += 1
    print(f"Built {len(stage_dirs) - failed} of {len(stage_dirs)} stage(s); {failed} failed.")
    return EXIT_VALIDATION if failed else EXIT_OK


_HEADER_FLAG_LABELS = [(f.name, int(f)) for f in HeaderFlag]


def _jsonable(value: Any) -> Any:
    if isinstance(value, bytes):
        return f"<{len(value)} bytes>"
    if isinstance(value, dict):
        return {k: _jsonable(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_jsonable(v) for v in value]
    return value


def cmd_inspect(args: argparse.Namespace) -> int:
    path = Path(args.file)
    parsed = parse_dat(path.read_bytes())
    h = parsed.header
    flag_names = [name for name, bit in _HEADER_FLAG_LABELS if h.flags & bit] or ["none"]
    print(f"File:             {path}")
    print(f"Magic:            {h.magic.decode('ascii', 'replace')}")
    print(f"Format version:   {h.format_version}")
    print(f"Schema version:   {h.schema_version}")
    print(f"Header size:      {h.header_size}")
    print(f"File size:        {h.file_size}")
    print(f"Section count:    {h.section_count}")
    major, minor, patch = (h.minimum_runtime_version >> 16) & 0xFFFF, \
        (h.minimum_runtime_version >> 8) & 0xFF, h.minimum_runtime_version & 0xFF
    print(f"Runtime version:  {major}.{minor}.{patch} (0x{h.minimum_runtime_version:08X})")
    print(f"Flags:            0x{h.flags:08X} ({' | '.join(flag_names)})")
    print(f"Header CRC32:     0x{h.header_crc32:08X} {'OK' if parsed.header_crc_ok else 'MISMATCH'}")
    print("Sections:")
    print(f"  {'TYPE':<5}{'OFFSET':>10}{'STORED':>12}{'RAW':>12}  {'COMPRESSION':<12}"
          f"{'FLAGS':<24}{'CRC32':<12}STATUS")
    for s in parsed.sections:
        flags = [n for n, bit in (("CRITICAL", SECTION_CRITICAL), ("STREAMABLE", SECTION_STREAMABLE),
                                  ("BINARY", SECTION_BINARY)) if s.flags & bit]
        comp = {COMPRESSION_NONE: "none", COMPRESSION_ZSTD: "zstd"}.get(s.compression, "unknown")
        print(f"  {s.type:<5}{s.offset:>10}{s.stored_size:>12}{s.raw_size:>12}  {comp:<12}"
              f"{','.join(flags) or '-':<24}0x{s.crc32:08X}  {s.status}")
    for problem in parsed.problems:
        print(f"PROBLEM: {problem}", file=sys.stderr)
    if args.dump:
        section = parsed.get(args.dump.upper())
        if section is None or section.raw is None:
            print(f"ERROR: section '{args.dump}' is not present or not readable", file=sys.stderr)
            return EXIT_VALIDATION
        if section.type in MSGPACK_SECTIONS:
            print(json.dumps(_jsonable(unpack_msgpack(section.raw)), ensure_ascii=False, indent=2))
        elif section.type == "WRLD":
            size, palette, voxels = decode_world(section.raw)
            print(json.dumps({"size": list(size), "palette": palette,
                              "non_air_voxels": len(voxels)}, indent=2))
        else:
            print(f"<{len(section.raw)} raw bytes>")
    return EXIT_OK if parsed.is_valid else EXIT_VALIDATION


def build_cli() -> argparse.ArgumentParser:
    parser = _ArgumentParser(prog="omsc.py",
                             description="OopsMath Stage Compiler (DAT v1)")
    sub = parser.add_subparsers(dest="command", required=True, parser_class=_ArgumentParser)

    def common(p: argparse.ArgumentParser) -> None:
        p.add_argument("--manifest", metavar="FILE", help="built-in registry (JSON or YAML)")
        p.add_argument("--base-locales", metavar="DIR", help="directory of global game FTL files")
        p.add_argument("--strict", action="store_true", help="treat warnings as errors")

    p = sub.add_parser("validate", help="validate a stage without writing a DAT")
    p.add_argument("stage_dir")
    common(p)
    p.set_defaults(handler=cmd_validate)

    p = sub.add_parser("build", help="validate and compile one stage")
    p.add_argument("stage_dir")
    p.add_argument("-o", "--output", metavar="FILE", help="output file (default <stage_id>.dat)")
    p.add_argument("--debug", action="store_true", help="set the DEBUG_BUILD header flag")
    common(p)
    p.set_defaults(handler=cmd_build)

    p = sub.add_parser("build-all", help="discover and compile every stage under a directory")
    p.add_argument("root")
    p.add_argument("--output-dir", metavar="DIR", default=".", help="output directory")
    p.add_argument("--debug", action="store_true", help="set the DEBUG_BUILD header flag")
    common(p)
    p.set_defaults(handler=cmd_build_all)

    p = sub.add_parser("inspect", help="inspect a DAT file")
    p.add_argument("file")
    p.add_argument("--dump", metavar="SECTION", help="decode and print one section (e.g. STAG)")
    p.set_defaults(handler=cmd_inspect)

    p = sub.add_parser("self-test", help="run the built-in self tests")
    p.set_defaults(handler=lambda _args: run_self_test())
    return parser


# ===========================================================================
# Self test
# ===========================================================================

_SELF_TEST_YAML = """\
schema_version: 1
stage:
  id: test_stage
  title: stage.test.title
  description: stage.test.description
  difficulty: tutorial
  level: 1
learning:
  question: stage.test.question
  curriculum:
    grade: {min: 7, max: 8}
    topics: [area]
world:
  bounds: {size: [16, 8, 16]}
construction:
  allowed_blocks:
    - {id: brick, min_count: 0, max_count: 10}
  targets:
    - id: wall_a
      type: wall
      region: {min: [0, 0, 0], max: [3, 2, 0]}
objectives:
  tasks:
    - id: t_math
      type: math
      math: {type: area, answer: {mode: exact, value: 12}}
    - {id: t_build, type: construction, depends_on: [t_math], target: wall_a}
  completion:
    all: [{task_completed: t_math}, {target_completed: wall_a}]
rewards:
  money: 100
  xp: 10
"""

_SELF_TEST_FTL = ("# test strings\nstage.test.title = Test stage\n"
                  "stage.test.description = A test.\nstage.test.question = How many bricks?\n")


def _expect(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def _make_glb(payload: bytes = b'{"asset":{"version":"2.0"}}') -> bytes:
    padded = payload + b" " * (-len(payload) % 4)
    body = struct.pack("<II", len(padded), GLB_JSON_CHUNK) + padded
    return GLB_MAGIC + struct.pack("<II", 2, 12 + len(body)) + body


def _make_vox(voxels: Sequence[tuple[int, int, int, int]],
              size: tuple[int, int, int] = (4, 4, 4)) -> bytes:
    def chunk(cid: bytes, content: bytes) -> bytes:
        return cid + struct.pack("<II", len(content), 0) + content
    xyzi = struct.pack("<I", len(voxels)) + b"".join(struct.pack("<BBBB", *v) for v in voxels)
    children = chunk(b"SIZE", struct.pack("<iii", *size)) + chunk(b"XYZI", xyzi)
    return b"VOX " + struct.pack("<I", 150) + b"MAIN" + struct.pack("<II", 0, len(children)) + children


def _write_stage(root: Path, doc: dict[str, Any], files: Optional[Mapping[str, Any]] = None) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    (root / "stage.yaml").write_text(yaml.safe_dump(doc, sort_keys=False), encoding="utf-8")
    for rel, content in (files or {}).items():
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content if isinstance(content, bytes) else content.encode("utf-8"))
    return root


def _compile_dir(stage_dir: Path, opts: Optional[CompileOptions] = None) -> bytes:
    result = validate_stage(stage_dir, opts or CompileOptions())
    if not result.diag.ok or result.source is None:
        raise AssertionError("unexpected validation errors: " + "; ".join(result.diag.errors))
    return compile_stage(result.source)


def _st_schema() -> None:
    jsonschema.Draft202012Validator.check_schema(STAGE_SCHEMA)


def _st_header_roundtrip() -> None:
    blob = build_dat([Section("META", pack_msgpack({"a": 1})), Section("STAG", pack_msgpack({"b": 2}))])
    parsed = parse_dat(blob)
    h = parsed.header
    _expect(blob[:8] == DAT_MAGIC and h.header_size == 64 and h.section_entry_size == 48, "header fields")
    _expect(h.file_size == len(blob) and len(blob) % 16 == 0, "file size / alignment")
    _expect(parsed.header_crc_ok and parsed.is_valid, "header CRC")


def _st_section_directory() -> None:
    sections = [Section("META", b"m" * 40), Section("STAG", b"s" * 50), Section("ASIX", b"x"),
                Section("ASDT", bytes(range(200)))]
    parsed = parse_dat(build_dat(sections))
    _expect([s.type for s in parsed.sections] == ["META", "STAG", "ASIX", "ASDT"], "section order")
    for original, got in zip(sections, parsed.sections):
        _expect(got.raw == original.payload and got.offset % 16 == 0, f"{original.type} round trip")


def _st_crc() -> None:
    blob = bytearray(build_dat([Section("META", b"meta"), Section("STAG", b"stag-data-1234567890")]))
    parsed = parse_dat(bytes(blob))
    target = parsed.sections[1]
    blob[target.offset] ^= 0xFF
    _expect(parse_dat(bytes(blob)).sections[1].status == "CRC32 MISMATCH", "section corruption undetected")
    blob2 = bytearray(build_dat([Section("META", b"meta"), Section("STAG", b"stag")]))
    blob2[16] ^= 0x01
    _expect(not parse_dat(bytes(blob2)).header_crc_ok, "header corruption undetected")


def _st_zstd() -> None:
    data = b"OopsMath " * 500
    packed = zstd_compress(data)
    _expect(len(packed) < len(data) and zstd_decompress(packed, len(data)) == data, "zstd round trip")
    blob = build_dat([Section("META", data), Section("STAG", b"tiny")])
    parsed = parse_dat(blob)
    _expect(parsed.sections[0].compression == COMPRESSION_ZSTD, "compressible section not compressed")
    _expect(parsed.sections[1].compression == COMPRESSION_NONE, "incompressible section was compressed")
    _expect(bool(parsed.header.flags & HeaderFlag.HAS_COMPRESSED_SECTIONS), "compressed flag")


def _st_msgpack() -> None:
    a = pack_msgpack({"b": [1, 2.5, None], "a": {"y": b"\x00\x01", "x": True}})
    b = pack_msgpack({"a": {"x": True, "y": b"\x00\x01"}, "b": [1, 2.5, None]})
    _expect(a == b, "msgpack is not canonical")
    _expect(unpack_msgpack(a)["a"]["y"] == b"\x00\x01", "msgpack round trip")


def _st_minimal_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        stage = _write_stage(Path(tmp) / "s", yaml.safe_load(_SELF_TEST_YAML))
        result = validate_stage(stage, CompileOptions())
        _expect(result.diag.ok, f"minimal stage rejected: {result.diag.errors}")
        parsed = parse_dat(_compile_dir(stage))
        _expect([s.type for s in parsed.sections] == ["META", "STAG"], "optional sections must be omitted")
        _expect(unpack_msgpack(parsed.get("META").raw)["stage_id"] == "test_stage", "META content")
        _expect(not validate_stage(stage, CompileOptions(strict=True)).diag.ok, "strict mode must fail on warnings")


def _st_deterministic() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        files = {"localization/en.ftl": _SELF_TEST_FTL, "localization/fa.ftl": _SELF_TEST_FTL}
        first = _compile_dir(_write_stage(Path(tmp) / "one", doc, files))
        second = _compile_dir(_write_stage(Path(tmp) / "deep" / "two", doc, files))
        _expect(first == second, "output differs between identical sources")
        _expect(first == _compile_dir(Path(tmp) / "one"), "output differs between runs")


def _st_ftl_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        files = {"localization/en.ftl": _SELF_TEST_FTL, "localization/fa.ftl": _SELF_TEST_FTL}
        stage = _write_stage(Path(tmp) / "s", yaml.safe_load(_SELF_TEST_YAML), files)
        result = validate_stage(stage, CompileOptions(strict=True))
        _expect(result.diag.ok, f"FTL stage rejected: {result.diag.errors}")
        parsed = parse_dat(_compile_dir(stage))
        locl = unpack_msgpack(parsed.get("LOCL").raw)
        _expect(sorted(locl) == ["en", "fa"] and locl["en"] == _SELF_TEST_FTL.encode(), "LOCL content")
        bad = _write_stage(Path(tmp) / "bad", yaml.safe_load(_SELF_TEST_YAML),
                           {"localization/en.ftl": "stage.test.title = Only title\n"})
        _expect(any("stage.test.question" in e for e in validate_stage(bad, CompileOptions()).diag.errors),
                "missing localization key not reported")
        diag = Diagnostics()
        parse_ftl("a = 1\na = 2\ngarbage line\nb = { oops\n", "x.ftl", diag)
        _expect(len(diag.errors) == 3, f"expected 3 FTL errors, got {diag.errors}")


def _st_markdown_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["learning"]["lesson"] = {"markdown": "lesson.md"}
        doc["learning"]["hints"] = [{"id": "h1", "text": "stage.test.title", "markdown": "./lesson.md"}]
        stage = _write_stage(Path(tmp) / "s", doc, {"lesson.md": "# Area\n\u0645\u0633\u0627\u062d\u062a\n"})
        parsed = parse_dat(_compile_dir(stage))
        docs = unpack_msgpack(parsed.get("DOCS").raw)["documents"]
        _expect(len(docs) == 1 and docs[0]["id"] == "lesson.md", "markdown must be deduplicated by path")
        _expect(docs[0]["text"] == "# Area\n\u0645\u0633\u0627\u062d\u062a\n", "markdown must be preserved exactly")
        stag = unpack_msgpack(parsed.get("STAG").raw)
        _expect(stag["learning"]["hints"][0]["markdown"] == "lesson.md", "STAG must reference the document ID")


def _st_glb_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["assets"] = {"custom": [{"id": "beam", "file": "assets/special_beam.glb", "type": "prop",
                                     "behavior": {"mass_kg": 12.5}}]}
        glb = _make_glb()
        stage = _write_stage(Path(tmp) / "s", doc, {"assets/special_beam.glb": glb})
        parsed = parse_dat(_compile_dir(stage))
        entry = unpack_msgpack(parsed.get("ASIX").raw)["assets"][0]
        asdt = parsed.get("ASDT").raw
        _expect(asdt[entry["offset"]:entry["offset"] + entry["size"]] == glb, "ASDT must hold exact GLB bytes")
        _expect(entry["sha256"] == hashlib.sha256(glb).hexdigest(), "ASIX sha256")
        _expect(parsed.get("ASDT").compression == COMPRESSION_NONE, "ASDT must stay raw")
        bad = _write_stage(Path(tmp) / "bad", doc, {"assets/special_beam.glb": b"not a glb at all......"})
        _expect(not validate_stage(bad, CompileOptions()).diag.ok, "invalid GLB accepted")


def _st_vox_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["world"]["source"] = {"voxel_file": "world.vox", "palette": {"1": "brick", "2": "stone"}}
        vox = _make_vox([(0, 0, 0, 1), (1, 0, 0, 2), (1, 1, 1, 2)])
        stage = _write_stage(Path(tmp) / "s", doc, {"world.vox": vox})
        parsed = parse_dat(_compile_dir(stage))
        size, palette, voxels = decode_world(parsed.get("WRLD").raw)
        _expect(size == (16, 8, 16) and palette == ["air", "brick", "stone"], "WRLD header/palette")
        _expect(voxels == {(0, 0, 0): "brick", (1, 0, 0): "stone", (1, 1, 1): "stone"}, f"voxels {voxels}")
        unmapped = dict(doc, world=dict(doc["world"], source={"voxel_file": "world.vox", "palette": {"1": "brick"}}))
        bad = _write_stage(Path(tmp) / "bad", unmapped, {"world.vox": vox})
        _expect(any("no entry in world.source.palette" in e for e in validate_stage(bad, CompileOptions()).diag.errors),
                "unmapped palette index must be an error")


def _st_invalid_reference() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["objectives"]["tasks"][1]["depends_on"] = ["nope"]
        doc["objectives"]["completion"] = {"task_completed": "ghost"}
        errors = validate_stage(_write_stage(Path(tmp) / "s", doc), CompileOptions()).diag.errors
        _expect(any("unknown task 'nope'" in e for e in errors), f"missing dependency error: {errors}")
        _expect(any("unknown task 'ghost'" in e for e in errors), f"missing condition error: {errors}")
        doc2 = yaml.safe_load(_SELF_TEST_YAML)
        doc2["construction"]["allowed_blocks"][0]["id"] = "foo"
        opts = CompileOptions(manifest=Registry(blocks=frozenset({"brick"})))
        errors2 = validate_stage(_write_stage(Path(tmp) / "t", doc2), opts).diag.errors
        _expect(any("unknown block 'foo'" in e for e in errors2), f"manifest check failed: {errors2}")
        result = validate_stage(_write_stage(Path(tmp) / "u", doc2), opts)
        _expect(result.source is None, "invalid stage must not produce a source")


def _st_unsafe_paths() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        for bad_path in ("../outside.md", "/etc/passwd", "C:/x.md", "a\\b.md"):
            doc = yaml.safe_load(_SELF_TEST_YAML)
            doc["learning"]["lesson"] = {"markdown": bad_path}
            errors = validate_stage(_write_stage(Path(tmp) / "s", doc), CompileOptions()).diag.errors
            _expect(any("unsafe path" in e for e in errors), f"'{bad_path}' accepted: {errors}")
        outside = Path(tmp) / "outside.md"
        outside.write_text("secret", encoding="utf-8")
        stage = Path(tmp) / "link"
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["learning"]["lesson"] = {"markdown": "link.md"}
        _write_stage(stage, doc)
        try:
            (stage / "link.md").symlink_to(outside)
        except (OSError, NotImplementedError):
            return  # symlinks unavailable on this platform
        errors = validate_stage(stage, CompileOptions()).diag.errors
        _expect(any("outside the stage directory" in e for e in errors), "symlink escape accepted")


def run_self_test() -> int:
    tests: list[tuple[str, Callable[[], None]]] = [
        ("embedded schema is a valid Draft 2020-12 schema", _st_schema),
        ("header round trip", _st_header_roundtrip),
        ("section directory round trip", _st_section_directory),
        ("CRC validation", _st_crc),
        ("Zstandard round trip and policy", _st_zstd),
        ("MessagePack round trip and canonical order", _st_msgpack),
        ("minimal YAML-only stage", _st_minimal_stage),
        ("deterministic DAT generation", _st_deterministic),
        ("stage with FTL", _st_ftl_stage),
        ("stage with Markdown", _st_markdown_stage),
        ("stage with custom GLB", _st_glb_stage),
        ("stage with VOX world", _st_vox_stage),
        ("invalid reference rejection", _st_invalid_reference),
        ("unsafe path rejection", _st_unsafe_paths),
    ]
    failures = 0
    for name, test in tests:
        try:
            test()
        except Exception as exc:  # report every failure, never hide it
            failures += 1
            print(f"FAIL  {name}: {type(exc).__name__}: {exc}")
        else:
            print(f"PASS  {name}")
    print(f"{len(tests) - failures}/{len(tests)} tests passed")
    return 1 if failures else EXIT_OK


# ===========================================================================
# Entry point
# ===========================================================================

def main(argv: Optional[Sequence[str]] = None) -> int:
    _configure_stdio()
    args = build_cli().parse_args(argv)
    try:
        return int(args.handler(args))
    except OopsMathError as exc:
        print(f"ERROR: {exc}" if not isinstance(exc, StageValidationError) else str(exc),
              file=sys.stderr)
        return exc.exit_code
    except OSError as exc:
        print(f"ERROR: I/O failure: {exc}", file=sys.stderr)
        return EXIT_IO


if __name__ == "__main__":
    sys.exit(main())
