"""Lexical and filesystem path safety for stage-relative paths."""

from __future__ import annotations

import re
from pathlib import Path, PurePosixPath

from .errors import UnsafePathError


def normalize_rel_path(rel: str) -> str:
    """Validate a source-relative path purely lexically; return POSIX form."""
    if not isinstance(rel, str) or not rel:
        raise UnsafePathError("path must be a non-empty string")
    if "\x00" in rel:
        raise UnsafePathError("path contains a NUL character")
    if "\\" in rel:
        raise UnsafePathError("backslashes are not allowed; use '/' separators")
    if rel.startswith("/") or re.match(r"^[A-Za-z]:", rel):
        raise UnsafePathError("absolute paths are not allowed")
    parts = PurePosixPath(rel).parts
    if not parts:
        raise UnsafePathError("path is empty")
    if ".." in parts:
        raise UnsafePathError("parent directory traversal ('..') is not allowed")
    return "/".join(parts)


def resolve_safe_path(stage_root: Path, rel: str) -> tuple[Path, str]:
    """Return (filesystem path, normalised relative path) inside stage_root."""
    normalized = normalize_rel_path(rel)
    candidate = stage_root.joinpath(*normalized.split("/"))
    try:
        candidate.resolve().relative_to(stage_root.resolve())
    except ValueError as exc:
        raise UnsafePathError("path resolves outside the stage directory "
                              "(symbolic link escape?)") from exc
    return candidate, normalized
