"""Built-in registries (manifest, base localization) and compile options."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

import yaml

from .errors import ConfigError, Diagnostics
from .ftl import parse_ftl


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
