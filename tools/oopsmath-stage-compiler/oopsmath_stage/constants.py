"""Format constants.  Everything here defines the DAT v1 binary layout - do not change."""

from __future__ import annotations

import enum
import re
import struct

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
