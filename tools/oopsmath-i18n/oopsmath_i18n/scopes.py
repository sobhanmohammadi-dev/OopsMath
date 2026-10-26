"""Scopes: the core game, or one level (stage)."""
from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path

LEVEL_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]*$")
SOURCE_LANG = "en-US"


@dataclass(frozen=True, order=True)
class Scope:
    kind: str  # "core" | "level"
    id: str = ""

    @property
    def name(self) -> str:
        return "core" if self.kind == "core" else f"level:{self.id}"

    @property
    def ftl_kind(self) -> str:
        return self.kind

    def ftl_rel(self) -> str:
        return "core.ftl" if self.kind == "core" else f"levels/{self.id}.ftl"

    def po_rel(self, suffix: str = ".po") -> str:
        return "core" + suffix if self.kind == "core" else f"levels/{self.id}{suffix}"


def core() -> Scope:
    return Scope("core")


def level(level_id: str) -> Scope:
    if not LEVEL_ID.match(level_id):
        raise ValueError(f"'{level_id}' is not a valid level id")
    return Scope("level", level_id)


@dataclass
class Layout:
    """Where everything lives, relative to the repository root."""
    root: Path

    @property
    def i18n(self) -> Path:
        return self.root / "assets" / "i18n"

    @property
    def work(self) -> Path:
        return self.root / "translations"

    @property
    def fonts(self) -> Path:
        return self.root / "assets" / "fonts"

    # ---- level registry -------------------------------------------------
    # The English text of a level belongs to the stage source (it is compiled
    # into the stage). The registry only records where each stage lives; the
    # text is never copied.
    @property
    def registry(self) -> Path:
        return self.work / "levels.json"

    def levels(self) -> dict[str, dict]:
        if not self.registry.is_file():
            return {}
        try:
            data = json.loads(self.registry.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}
        return data if isinstance(data, dict) else {}

    def save_levels(self, levels: dict[str, dict]) -> None:
        self.registry.parent.mkdir(parents=True, exist_ok=True)
        self.registry.write_text(json.dumps(levels, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
                                 encoding="utf-8")

    def source_ftl(self, scope: Scope) -> Path:
        """The English source file of a scope."""
        if scope.kind == "core":
            return self.i18n / SOURCE_LANG / "core.ftl"
        entry = self.levels().get(scope.id)
        if entry is None:
            return self.root / "<level not registered>" / f"{scope.id}.ftl"
        return self.root / entry["dir"] / entry.get("ftl", "localization") / f"{SOURCE_LANG}.ftl"

    def lang_ftl(self, lang: str, scope: Scope) -> Path:
        return self.i18n / lang / scope.ftl_rel()

    def pot(self, scope: Scope) -> Path:
        return self.work / "templates" / scope.po_rel(".pot")

    def po(self, lang: str, scope: Scope) -> Path:
        return self.work / lang / scope.po_rel(".po")

    def build(self) -> Path:
        return self.work / "build"


def find_root(start: Path) -> Path:
    for p in [start, *start.parents]:
        if (p / "assets" / "i18n").is_dir() or (p / "Cargo.toml").is_file() and (p / "assets").is_dir():
            return p
    return start
