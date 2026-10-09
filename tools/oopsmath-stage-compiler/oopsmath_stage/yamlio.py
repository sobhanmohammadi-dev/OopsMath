"""Layer 1: read stage.yaml into normalised JSON-like data."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

import yaml

from .constants import MAX_YAML_BYTES, MAX_YAML_DEPTH, MAX_YAML_NODES
from .errors import Diagnostics, SourceDataError, UnsafePathError
from .paths import resolve_safe_path


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


def read_limited(path: Path, limit: int) -> bytes:
    size = path.stat().st_size
    if size > limit:
        raise SourceDataError(f"file is too large ({size} bytes, limit {limit})")
    return path.read_bytes()


def load_stage_yaml(stage_dir: Path, diag: Diagnostics) -> Optional[dict[str, Any]]:
    """Read and parse stage.yaml into normalised JSON-like data."""
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
        raw = read_limited(path, MAX_YAML_BYTES)
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
