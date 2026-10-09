"""Stage source model (the output of validation)."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional

from .errors import Diagnostics
from .world import Voxel


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
