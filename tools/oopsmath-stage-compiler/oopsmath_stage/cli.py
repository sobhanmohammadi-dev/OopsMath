"""Command line interface for OMSC."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Optional, Sequence

from . import __version__
from .constants import (
    COMPRESSION_NONE, COMPRESSION_ZSTD, EXIT_ARGS, EXIT_IO, EXIT_OK, EXIT_VALIDATION,
    MSGPACK_SECTIONS, SECTION_BINARY, SECTION_CRITICAL, SECTION_STREAMABLE, HeaderFlag,
)
from .errors import ConfigError, OopsMathError, StageValidationError
from .model import ValidationResult
from .ui import Style, human_size, table
from .util import unpack_msgpack
from .workflow import (
    BuildOutcome, build_all, build_one, discover_stage_dirs, make_options, read_dat,
)
from .world import decode_world

PROG = "omsc"

EPILOG = """\
examples:
  omsc validate examples/001_first_wall
  omsc build examples/001_first_wall -o build/001.dat
  omsc build-all examples --output-dir build/stages --strict
  omsc inspect build/001.dat --dump STAG
  omsc new stages/002_second_wall
  omsc tui examples

exit codes: 0 ok | 2 validation error / corrupt DAT | 3 bad arguments
            4 I/O error | 5 binary write error
"""


class _ArgumentParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:  # type: ignore[override]
        self.print_usage(sys.stderr)
        self.exit(EXIT_ARGS, f"{self.prog}: error: {message}\n")


def _configure_stdio() -> None:
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            try:
                reconfigure(encoding="utf-8", errors="replace")
            except ValueError:
                pass  # stream cannot be reconfigured; keep its defaults


# --------------------------------------------------------------------------- output

class _Out:
    """Bundles the two styled streams and the --quiet flag."""

    def __init__(self, args: argparse.Namespace) -> None:
        color = False if getattr(args, "no_color", False) else None
        self.out = Style(sys.stdout, color)
        self.err = Style(sys.stderr, color)
        self.quiet = bool(getattr(args, "quiet", False))

    def say(self, text: str = "") -> None:
        if not self.quiet:
            print(text)


def _print_diagnostics(o: _Out, result: ValidationResult, *, show_passed: bool = True) -> None:
    if show_passed:
        for line in result.passed:
            o.say(o.out.ok(line))
    for warning in result.diag.warnings:
        print(o.err.warn(warning), file=sys.stderr)
    for error in result.diag.errors:
        print(o.err.error(error), file=sys.stderr)


def _summary_line(o: _Out, ok: bool, errors: int, warnings: int) -> str:
    counts = f"{errors} error(s), {warnings} warning(s)"
    if ok:
        return o.out.paint(f"Result: valid ({counts})", "green", "bold")
    return o.out.paint(f"Result: FAILED ({counts})", "red", "bold")


def _result_json(stage_dir: Path, result: ValidationResult) -> dict[str, Any]:
    return {"stage_dir": str(stage_dir), "stage_id": result.stage_id, "ok": result.diag.ok,
            "passed": result.passed, "errors": result.diag.errors,
            "warnings": result.diag.warnings}


# ------------------------------------------------------------------------- commands

def cmd_validate(args: argparse.Namespace) -> int:
    o = _Out(args)
    opts = make_options(manifest=args.manifest, base_locales=args.base_locales, strict=args.strict)
    root = Path(args.stage_dir)
    dirs = [root] if (root / "stage.yaml").is_file() else (discover_stage_dirs(root) if root.is_dir() else [])
    dirs = dirs or [root]
    exit_code = EXIT_OK
    reports: list[dict[str, Any]] = []
    for stage_dir in dirs:
        outcome = build_one(stage_dir, None, opts, write=False)
        result = outcome.result
        assert result is not None
        if args.format == "json":
            reports.append(_result_json(stage_dir, result))
        else:
            if len(dirs) > 1:
                o.say(o.out.heading(f"== {stage_dir} =="))
            _print_diagnostics(o, result)
            o.say(_summary_line(o, result.diag.ok, len(result.diag.errors), len(result.diag.warnings)))
        if not result.diag.ok:
            exit_code = EXIT_VALIDATION
    if args.format == "json":
        print(json.dumps(reports[0] if len(reports) == 1 else reports, indent=2, ensure_ascii=False))
    return exit_code


def _report_build(o: _Out, outcome: BuildOutcome) -> int:
    if outcome.skipped_reason:
        print(o.err.error(outcome.skipped_reason), file=sys.stderr)
        return EXIT_VALIDATION
    assert outcome.result is not None
    _print_diagnostics(o, outcome.result)
    if not outcome.result.diag.ok:
        return EXIT_VALIDATION
    if outcome.error:
        print(o.err.error(outcome.error), file=sys.stderr)
        return EXIT_VALIDATION
    o.say(o.out.ok("Stage compiled"))
    o.say(f"Output: {outcome.output}  {o.out.dim('(' + human_size(outcome.size) + ')')}")
    return EXIT_OK


def cmd_build(args: argparse.Namespace) -> int:
    o = _Out(args)
    opts = make_options(manifest=args.manifest, base_locales=args.base_locales,
                        strict=args.strict, debug=args.debug)
    outcome = build_one(Path(args.stage_dir), Path(args.output) if args.output else None, opts)
    return _report_build(o, outcome)


def cmd_build_all(args: argparse.Namespace) -> int:
    o = _Out(args)
    opts = make_options(manifest=args.manifest, base_locales=args.base_locales,
                        strict=args.strict, debug=args.debug)
    root = Path(args.root)
    if root.is_dir() and not discover_stage_dirs(root):
        print(o.err.error(f"no stage directories (containing stage.yaml) found under '{root}'"),
              file=sys.stderr)
        return EXIT_VALIDATION
    failed = 0

    def progress(outcome: BuildOutcome) -> None:
        nonlocal failed
        o.say(o.out.heading(f"== {outcome.stage_dir} =="))
        if _report_build(o, outcome) != EXIT_OK:
            failed += 1

    outcomes = build_all(root, Path(args.output_dir), opts, on_progress=progress)
    total = len(outcomes)
    line = f"Built {total - failed} of {total} stage(s); {failed} failed."
    print(o.out.paint(line, "red" if failed else "green", "bold"))
    return EXIT_VALIDATION if failed else EXIT_OK


def cmd_inspect(args: argparse.Namespace) -> int:
    o = _Out(args)
    st = o.out
    path = Path(args.file)
    parsed = read_dat(path)
    h = parsed.header
    flag_names = [f.name for f in HeaderFlag if h.flags & int(f)] or ["none"]
    major, minor, patch = ((h.minimum_runtime_version >> 16) & 0xFFFF,
                           (h.minimum_runtime_version >> 8) & 0xFF,
                           h.minimum_runtime_version & 0xFF)
    crc_state = st.paint("OK", "green") if parsed.header_crc_ok else st.paint("MISMATCH", "red", "bold")
    fields = [
        ("File", str(path)), ("Magic", h.magic.decode("ascii", "replace")),
        ("Format version", str(h.format_version)), ("Schema version", str(h.schema_version)),
        ("Header size", str(h.header_size)), ("File size", str(h.file_size)),
        ("Section count", str(h.section_count)),
        ("Runtime version", f"{major}.{minor}.{patch} (0x{h.minimum_runtime_version:08X})"),
        ("Flags", f"0x{h.flags:08X} ({' | '.join(flag_names)})"),
        ("Header CRC32", f"0x{h.header_crc32:08X} {crc_state}"),
    ]
    for label, value in fields:
        print(f"{(label + ':'):<18}{value}")
    print("Sections:")
    rows = []
    for s in parsed.sections:
        flags = [n for n, bit in (("CRITICAL", SECTION_CRITICAL), ("STREAMABLE", SECTION_STREAMABLE),
                                  ("BINARY", SECTION_BINARY)) if s.flags & bit]
        comp = {COMPRESSION_NONE: "none", COMPRESSION_ZSTD: "zstd"}.get(s.compression, "unknown")
        rows.append([s.type, str(s.offset), str(s.stored_size), str(s.raw_size), comp,
                     ",".join(flags) or "-", f"0x{s.crc32:08X}", s.status])
    lines = table(rows, ["TYPE", "OFFSET", "STORED", "RAW", "COMPRESSION", "FLAGS", "CRC32", "STATUS"],
                  right=(1, 2, 3))
    print(st.paint(lines[0], "bold"))
    for line, s in zip(lines[1:], parsed.sections):
        print(line if s.status == "OK" else st.paint(line, "red"))
    for problem in parsed.problems:
        print(o.err.error(problem), file=sys.stderr)
    if args.dump:
        section = parsed.get(args.dump.upper())
        if section is None or section.raw is None:
            print(o.err.error(f"section '{args.dump}' is not present or not readable"), file=sys.stderr)
            return EXIT_VALIDATION
        print(st.rule())
        print(dump_section(section.type, section.raw))
    return EXIT_OK if parsed.is_valid else EXIT_VALIDATION


def _jsonable(value: Any) -> Any:
    if isinstance(value, bytes):
        return f"<{len(value)} bytes>"
    if isinstance(value, dict):
        return {k: _jsonable(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_jsonable(v) for v in value]
    return value


def dump_section(section_type: str, raw: bytes) -> str:
    """Human-readable dump of one decoded section (shared with the TUI)."""
    if section_type in MSGPACK_SECTIONS:
        return json.dumps(_jsonable(unpack_msgpack(raw)), ensure_ascii=False, indent=2)
    if section_type == "WRLD":
        size, palette, voxels = decode_world(raw)
        return json.dumps({"size": list(size), "palette": palette,
                           "non_air_voxels": len(voxels)}, indent=2)
    return f"<{len(raw)} raw bytes>"


def cmd_new(args: argparse.Namespace) -> int:
    from .scaffold import create_stage
    o = _Out(args)
    directory = Path(args.directory)
    stage_id = args.id or directory.name
    created = create_stage(directory, stage_id)
    for file in created:
        o.say(o.out.ok(f"created {file}"))
    o.say(f"\nNext: {PROG} validate {directory}")
    return EXIT_OK


def cmd_self_test(args: argparse.Namespace) -> int:
    from .selftest import run_self_test
    return run_self_test(_Out(args).out)


def cmd_tui(args: argparse.Namespace) -> int:
    try:
        from .tui import run_tui
    except ImportError:
        print("ERROR: the TUI needs the 'textual' package.\n"
              "Install with: pip install textual", file=sys.stderr)
        return EXIT_IO
    opts = make_options(manifest=args.manifest, base_locales=args.base_locales, strict=args.strict)
    return run_tui(Path(args.root), opts, Path(args.output_dir))


# ------------------------------------------------------------------------- parser

def build_cli() -> argparse.ArgumentParser:
    parser = _ArgumentParser(prog=PROG, description="OopsMath Stage Compiler (DAT v1 / Stage Schema v1)",
                             epilog=EPILOG, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--version", action="version", version=f"{PROG} {__version__}")
    parser.add_argument("--no-color", action="store_true", help="disable coloured output")
    sub = parser.add_subparsers(dest="command", metavar="COMMAND", required=True,
                                parser_class=_ArgumentParser)

    def common(p: argparse.ArgumentParser) -> None:
        p.add_argument("--manifest", metavar="FILE", help="built-in registry (JSON or YAML)")
        p.add_argument("--base-locales", metavar="DIR", help="directory of global game FTL files")
        p.add_argument("--strict", action="store_true", help="treat warnings as errors")

    p = sub.add_parser("validate", help="validate a stage (or every stage under a directory)")
    p.add_argument("stage_dir")
    p.add_argument("--format", choices=("text", "json"), default="text",
                   help="output format (json prints one machine-readable report)")
    p.add_argument("-q", "--quiet", action="store_true", help="only print warnings and errors")
    common(p)
    p.set_defaults(handler=cmd_validate)

    p = sub.add_parser("build", help="validate and compile one stage")
    p.add_argument("stage_dir")
    p.add_argument("-o", "--output", metavar="FILE", help="output file or directory (default <stage_id>.dat)")
    p.add_argument("--debug", action="store_true", help="set the DEBUG_BUILD header flag")
    p.add_argument("-q", "--quiet", action="store_true", help="only print warnings and errors")
    common(p)
    p.set_defaults(handler=cmd_build)

    p = sub.add_parser("build-all", help="discover and compile every stage under a directory")
    p.add_argument("root")
    p.add_argument("--output-dir", metavar="DIR", default=".", help="output directory (default .)")
    p.add_argument("--debug", action="store_true", help="set the DEBUG_BUILD header flag")
    p.add_argument("-q", "--quiet", action="store_true", help="only print warnings and errors")
    common(p)
    p.set_defaults(handler=cmd_build_all)

    p = sub.add_parser("inspect", help="inspect a DAT file")
    p.add_argument("file")
    p.add_argument("--dump", metavar="SECTION", help="decode and print one section (e.g. STAG)")
    p.set_defaults(handler=cmd_inspect)

    p = sub.add_parser("new", help="create a starter stage directory")
    p.add_argument("directory", help="where to create the stage")
    p.add_argument("--id", help="stage id (default: the directory name)")
    p.set_defaults(handler=cmd_new)

    p = sub.add_parser("tui", help="open the interactive terminal UI')")
    p.add_argument("root", nargs="?", default=".", help="directory containing stages (default .)")
    p.add_argument("--output-dir", metavar="DIR", default="build", help="where builds go (default build)")
    common(p)
    p.set_defaults(handler=cmd_tui)

    p = sub.add_parser("self-test", help="run the built-in self tests")
    p.set_defaults(handler=cmd_self_test)
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    _configure_stdio()
    args = build_cli().parse_args(argv)
    try:
        return int(args.handler(args))
    except OopsMathError as exc:
        print(f"ERROR: {exc}" if not isinstance(exc, StageValidationError) else str(exc),
              file=sys.stderr)
        return exc.exit_code
    except BrokenPipeError:  # e.g. `omsc inspect ... | head`
        try:
            sys.stdout = None  # type: ignore[assignment]  # silence the interpreter-exit flush error
        except Exception:
            pass
        return EXIT_OK
    except KeyboardInterrupt:
        print("Interrupted.", file=sys.stderr)
        return 130
    except OSError as exc:
        print(f"ERROR: I/O failure: {exc}", file=sys.stderr)
        return EXIT_IO
