"""Entry-point shim: turns a missing third-party dependency into a friendly error."""

from __future__ import annotations

import sys

EXIT_MISSING_DEPENDENCY = 4


def run() -> None:
    try:
        from .cli import main
    except ImportError as exc:  # pragma: no cover - environment problem
        sys.stderr.write(
            f"ERROR: missing Python dependency ({exc}).\n"
            "Install with: pip install PyYAML jsonschema msgpack zstandard\n"
        )
        raise SystemExit(EXIT_MISSING_DEPENDENCY)
    sys.exit(main())
