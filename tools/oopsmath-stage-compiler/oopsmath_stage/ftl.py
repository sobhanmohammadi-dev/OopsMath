"""Lightweight Fluent (FTL) syntax validator."""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Optional

from .errors import Diagnostics

_FTL_ENTRY = re.compile(r"^(-?)([A-Za-z][A-Za-z0-9_.\-]*)[ \t]*=(.*)$")
_FTL_ATTRIBUTE = re.compile(r"^[ \t]+\.([A-Za-z][A-Za-z0-9_\-]*)[ \t]*=(.*)$")
_FTL_COMMENT = re.compile(r"^#{1,3}( .*)?$")


@dataclass
class _FtlEntry:
    key: str
    line: int
    segments: list[list[str]]
    has_attributes: bool = False


def _check_placeables(text: str) -> Optional[str]:
    """Check '{...}' balance; string literals are only legal inside placeables."""
    depth, in_string, i = 0, False, 0
    while i < len(text):
        ch = text[i]
        if in_string:
            if ch == "\\":
                i += 1
            elif ch == '"':
                in_string = False
        elif ch == '"' and depth > 0:
            in_string = True
        elif ch == "{":
            depth += 1
        elif ch == "}":
            if depth == 0:
                return "unmatched '}' (use {\"}\"} for a literal brace)"
            depth -= 1
        i += 1
    if in_string:
        return "unterminated string literal in placeable"
    if depth:
        return "unclosed '{' placeable"
    return None


def parse_ftl(text: str, rel_path: str, diag: Diagnostics) -> frozenset[str]:
    """Validate basic Fluent syntax; return the set of message keys."""
    messages: dict[str, int] = {}
    terms: dict[str, int] = {}
    if "\x00" in text:
        diag.error(f"{rel_path}: file contains NUL characters")
    if text.startswith("\ufeff"):
        diag.error(f"{rel_path}: UTF-8 byte-order mark is not allowed")
    current: Optional[_FtlEntry] = None

    def close(entry: Optional[_FtlEntry]) -> None:
        if entry is None:
            return
        if not "\n".join(entry.segments[0]).strip() and not entry.has_attributes:
            diag.error(f"{rel_path}:{entry.line}: '{entry.key}' has neither a value nor attributes")
        for segment in entry.segments:
            problem = _check_placeables("\n".join(segment))
            if problem:
                diag.error(f"{rel_path}:{entry.line}: in '{entry.key}': {problem}")

    for number, line in enumerate(text.replace("\r\n", "\n").split("\n"), start=1):
        if not line.strip():
            continue
        if line[0] in " \t":
            if current is None:
                diag.error(f"{rel_path}:{number}: indented text does not belong to any entry")
                continue
            attribute = _FTL_ATTRIBUTE.match(line)
            if attribute:
                current.has_attributes = True
                current.segments.append([attribute.group(2)])
            else:
                current.segments[-1].append(line)
        elif line[0] == "#":
            close(current)
            current = None
            if not _FTL_COMMENT.match(line):
                diag.error(f"{rel_path}:{number}: malformed comment (expected '# text')")
        else:
            close(current)
            current = None
            match = _FTL_ENTRY.match(line)
            if not match:
                diag.error(f"{rel_path}:{number}: malformed entry, expected 'key = value'")
                continue
            is_term, key = match.group(1) == "-", match.group(2)
            registry = terms if is_term else messages
            kind = "term" if is_term else "message"
            if key in registry:
                diag.error(f"{rel_path}:{number}: duplicate {kind} '{key}' "
                           f"(first defined at line {registry[key]})")
            else:
                registry[key] = number
            current = _FtlEntry(key, number, [[match.group(3)]])
    close(current)
    return frozenset(messages)
