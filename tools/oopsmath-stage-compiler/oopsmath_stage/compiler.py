"""Compilation: validated source -> sections -> DAT bytes."""

from __future__ import annotations

import copy
import hashlib
from typing import Any

from .constants import (
    COMPILER_VERSION, DAT_FORMAT_VERSION, DAT_MAGIC, GLB_MIME, STAGE_SCHEMA_VERSION,
)
from .dat import Section, build_dat, parse_dat
from .errors import BinaryWriteError
from .model import StageSource
from .paths import normalize_rel_path
from .util import pack_msgpack
from .world import encode_world


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
    """Produce DAT bytes and verify them before returning."""
    blob = build_dat(compile_sections(source), debug=debug)
    parsed = parse_dat(blob)
    if not parsed.is_valid:
        raise BinaryWriteError("self-verification of the generated DAT failed: " +
                               "; ".join(parsed.problems + [f"{s.type}: {s.status}"
                                                            for s in parsed.sections if s.status != "OK"]))
    return blob
