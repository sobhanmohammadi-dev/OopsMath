"""Validation layer 2: JSON-Schema structure."""

from __future__ import annotations

from typing import Any, Sequence

import jsonschema

from ..errors import Diagnostics
from ..schema import STAGE_SCHEMA


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
