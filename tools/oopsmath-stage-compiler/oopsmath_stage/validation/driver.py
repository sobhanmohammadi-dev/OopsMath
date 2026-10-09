"""Validation driver: runs every layer in order and builds the StageSource."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Callable

from ..errors import Diagnostics
from ..model import StageSource, ValidationResult
from ..registry import CompileOptions
from ..yamlio import load_stage_yaml
from .files import build_world, load_custom_assets, load_documents
from .localization import load_and_validate_localization
from .references import validate_references
from .schema_check import validate_schema


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
