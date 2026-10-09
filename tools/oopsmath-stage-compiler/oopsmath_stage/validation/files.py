"""Validation layer 4a: files - custom assets, VOX world, Markdown documents."""

from __future__ import annotations

import hashlib
import struct
from pathlib import Path
from typing import Any, Optional, Sequence

from ..constants import (
    AIR_BLOCK, GLB_JSON_CHUNK, GLB_MAGIC, MAX_BINARY_FILE_BYTES, MAX_TEXT_FILE_BYTES,
)
from ..errors import Diagnostics, UnsafePathError, VoxFormatError
from ..model import AssetFile, DocFile, WorldData
from ..paths import resolve_safe_path
from ..registry import CompileOptions, Registry
from ..vox import parse_vox
from ..world import Voxel
from .references import check_registered


def resolve_file(stage_dir: Path, rel: str, yaml_path: str, diag: Diagnostics,
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
        found = resolve_file(stage_dir, entry["file"], f"assets.custom[{i}].file", diag,
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
        found = resolve_file(stage_dir, source["voxel_file"], "world.source.voxel_file", diag,
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
                check_registered("block", registry, name, "world.source.palette", diag)
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
        found = resolve_file(stage_dir, rel, yaml_path, diag, extensions=(".md", ".markdown"),
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
