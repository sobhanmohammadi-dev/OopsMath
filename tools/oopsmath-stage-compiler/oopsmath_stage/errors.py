"""Exception hierarchy and the Diagnostics collector."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Sequence

from .constants import EXIT_ARGS, EXIT_VALIDATION, EXIT_WRITE


class OopsMathError(Exception):
    """Base class for all compiler errors that map to an exit code."""
    exit_code = EXIT_VALIDATION


class StageValidationError(OopsMathError):
    def __init__(self, errors: Sequence[str]) -> None:
        super().__init__("\n".join(f"ERROR: {e}" for e in errors))
        self.errors = list(errors)


class ConfigError(OopsMathError):
    exit_code = EXIT_ARGS


class DatFormatError(OopsMathError):
    exit_code = EXIT_VALIDATION


class BinaryWriteError(OopsMathError):
    exit_code = EXIT_WRITE


class StageCompileError(OopsMathError):
    exit_code = EXIT_VALIDATION


class UnsafePathError(ValueError):
    pass


class VoxFormatError(ValueError):
    pass


class SourceDataError(ValueError):
    pass


@dataclass
class Diagnostics:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def error(self, message: str) -> None:
        self.errors.append(message)

    def warn(self, message: str) -> None:
        self.warnings.append(message)

    @property
    def ok(self) -> bool:
        return not self.errors
