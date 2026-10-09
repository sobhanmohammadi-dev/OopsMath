"""High-level operations shared by the CLI and the TUI (no printing, no exiting)."""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from .compiler import compile_stage
from .dat import ParsedDat, parse_dat, write_atomic
from .errors import ConfigError, Diagnostics, OopsMathError
from .model import ValidationResult
from .registry import CompileOptions, load_base_locales, load_manifest
from .validation import validate_stage
from .yamlio import peek_stage_id


@dataclass
class BuildOutcome:
    stage_dir: Path
    result: Optional[ValidationResult] = None
    output: Optional[Path] = None
    size: int = 0
    error: Optional[str] = None            # fatal error that is not a validation error
    skipped_reason: Optional[str] = None   # e.g. duplicate stage id

    @property
    def stage_id(self) -> Optional[str]:
        return self.result.stage_id if self.result else None

    @property
    def diag(self) -> Diagnostics:
        return self.result.diag if self.result else Diagnostics()

    @property
    def ok(self) -> bool:
        return (self.error is None and self.skipped_reason is None
                and self.result is not None and self.result.diag.ok)


def make_options(*, manifest: Optional[str] = None, base_locales: Optional[str] = None,
                 strict: bool = False, debug: bool = False) -> CompileOptions:
    return CompileOptions(
        manifest=load_manifest(Path(manifest)) if manifest else None,
        base_keys=load_base_locales(Path(base_locales)) if base_locales else None,
        strict=strict, debug=debug)


def resolve_output(output: Optional[Path], stage_id: str, *, as_dir: bool = False) -> Path:
    """``as_dir`` forces ``output`` to be treated as a directory even if it does not exist yet."""
    if output is None:
        return Path(f"{stage_id}.dat")
    if as_dir or output.is_dir():
        return output / f"{stage_id}.dat"
    return output


def build_one(stage_dir: Path, output: Optional[Path], opts: CompileOptions,
              *, write: bool = True, output_is_dir: bool = False) -> BuildOutcome:
    """Validate and compile one stage; optionally write the .dat file."""
    outcome = BuildOutcome(stage_dir=stage_dir)
    outcome.result = validate_stage(stage_dir, opts)
    if not outcome.result.diag.ok or outcome.result.source is None:
        return outcome
    try:
        blob = compile_stage(outcome.result.source, debug=opts.debug)
        outcome.size = len(blob)
        if write:
            target = resolve_output(output, outcome.result.stage_id or stage_dir.name,
                                    as_dir=output_is_dir)
            write_atomic(target, blob)
            outcome.output = target
    except OopsMathError as exc:
        outcome.error = str(exc)
    return outcome


def discover_stage_dirs(root: Path) -> list[Path]:
    if (root / "stage.yaml").is_file():
        return [root]
    found = sorted((p.parent for p in root.rglob("stage.yaml")),
                   key=lambda p: p.relative_to(root).as_posix())
    return [d for d in found if not any(o != d and o in d.parents for o in found)]


def with_known_stage_ids(stage_dirs: list[Path], opts: CompileOptions
                         ) -> tuple[CompileOptions, dict[Path, Optional[str]], dict[str, int]]:
    ids = {d: peek_stage_id(d) for d in stage_dirs}
    counts: dict[str, int] = {}
    for ident in ids.values():
        if ident:
            counts[ident] = counts.get(ident, 0) + 1
    return dataclasses.replace(opts, known_stage_ids=frozenset(counts)), ids, counts


def build_all(root: Path, out_dir: Path, opts: CompileOptions, *, write: bool = True,
              on_progress=None) -> list[BuildOutcome]:
    if not root.is_dir():
        raise ConfigError(f"'{root}' is not a directory")
    dirs = discover_stage_dirs(root)
    opts, ids, counts = with_known_stage_ids(dirs, opts)
    outcomes: list[BuildOutcome] = []
    for stage_dir in dirs:
        ident = ids[stage_dir]
        if ident and counts[ident] > 1:
            outcome = BuildOutcome(stage_dir, skipped_reason=
                                   f"stage id '{ident}' is used by more than one stage directory")
        else:
            outcome = build_one(stage_dir, out_dir, opts, write=write, output_is_dir=True)
        outcomes.append(outcome)
        if on_progress:
            on_progress(outcome)
    return outcomes


def read_dat(path: Path) -> ParsedDat:
    return parse_dat(path.read_bytes())
