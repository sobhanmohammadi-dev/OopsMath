"""Terminal presentation helpers: colour, symbols, tables (stdlib only)."""

from __future__ import annotations

import os
import sys
from typing import Iterable, Optional, Sequence, TextIO


def _enable_windows_vt() -> bool:
    if os.name != "nt":
        return True
    try:  # pragma: no cover - Windows only
        import ctypes
        kernel32 = ctypes.windll.kernel32  # type: ignore[attr-defined]
        handle = kernel32.GetStdHandle(-11)
        mode = ctypes.c_uint32()
        if not kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
            return False
        return bool(kernel32.SetConsoleMode(handle, mode.value | 0x0004))
    except Exception:
        return False


_CODES = {"red": "31", "green": "32", "yellow": "33", "blue": "34", "magenta": "35",
          "cyan": "36", "gray": "90", "bold": "1", "dim": "2"}


class Style:
    """Colour/symbol policy for one output stream."""

    def __init__(self, stream: Optional[TextIO] = None, color: Optional[bool] = None) -> None:
        stream = stream or sys.stdout
        if color is None:
            color = (hasattr(stream, "isatty") and stream.isatty()
                     and "NO_COLOR" not in os.environ
                     and os.environ.get("TERM") != "dumb" and _enable_windows_vt())
        self.color = bool(color)
        encoding = (getattr(stream, "encoding", None) or "ascii").lower()
        self.unicode = "utf" in encoding
        self.ok_sym = "\u2714" if self.unicode else "OK"
        self.err_sym = "\u2718" if self.unicode else "X"
        self.warn_sym = "\u26a0" if self.unicode else "!"
        self.arrow = "\u2192" if self.unicode else "->"
        self.rule_char = "\u2500" if self.unicode else "-"

    def paint(self, text: str, *styles: str) -> str:
        if not self.color or not styles:
            return text
        codes = ";".join(_CODES[s] for s in styles)
        return f"\x1b[{codes}m{text}\x1b[0m"

    def ok(self, text: str) -> str:
        return f"{self.paint(self.ok_sym, 'green')} {text}"

    def error(self, text: str) -> str:
        return f"{self.paint('ERROR:', 'red', 'bold')} {text}"

    def warn(self, text: str) -> str:
        return f"{self.paint('WARNING:', 'yellow', 'bold')} {text}"

    def heading(self, text: str) -> str:
        return self.paint(text, "bold", "cyan")

    def dim(self, text: str) -> str:
        return self.paint(text, "dim")

    def rule(self, width: int = 60) -> str:
        return self.paint(self.rule_char * width, "gray")


def human_size(n: int) -> str:
    size = float(n)
    for unit in ("B", "KiB", "MiB", "GiB"):
        if size < 1024 or unit == "GiB":
            return f"{int(size)} {unit}" if unit == "B" else f"{size:.1f} {unit}"
        size /= 1024
    return f"{n} B"


def table(rows: Sequence[Sequence[str]], header: Sequence[str],
          right: Iterable[int] = ()) -> list[str]:
    """Render a plain aligned table (no colour codes inside cells)."""
    right_set = set(right)
    widths = [max(len(str(r[i])) for r in [header, *rows]) for i in range(len(header))]

    def fmt(row: Sequence[str]) -> str:
        cells = [str(c).rjust(widths[i]) if i in right_set else str(c).ljust(widths[i])
                 for i, c in enumerate(row)]
        return "  " + "  ".join(cells).rstrip()

    return [fmt(header), *(fmt(r) for r in rows)]
