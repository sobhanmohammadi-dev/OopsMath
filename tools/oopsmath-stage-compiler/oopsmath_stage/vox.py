"""MagicaVoxel (.vox) parser with scene-graph flattening."""

from __future__ import annotations

import struct
from typing import Any, Optional, Sequence

from .constants import MAX_VOX_VOXELS
from .errors import VoxFormatError


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
