"""Binary voxel world (WRLD) encoder / decoder."""

from __future__ import annotations

import struct
from typing import Mapping, Sequence

from .constants import (
    AIR_BLOCK, WORLD_CHUNK_RECORD, WORLD_CHUNK_SIZE, WORLD_CHUNK_VOLUME, WORLD_HEADER,
    WORLD_MAGIC, WORLD_VERSION,
)
from .errors import DatFormatError, StageCompileError

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
