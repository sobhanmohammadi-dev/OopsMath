"""Writes the readable `stage.schema.yaml` from the embedded schema.

`schema.py` is the source of truth (it is what the compiler validates against);
the YAML file is generated documentation. Regenerate after any schema change:

    python -m oopsmath_stage.schema_export > stage.schema.yaml

The self test `stage.schema.yaml matches the embedded schema` fails on drift.
"""

from __future__ import annotations

import sys

import yaml

from .schema import STAGE_SCHEMA

HEADER = (
    "# GENERATED from oopsmath_stage/schema.py (the source of truth).\n"
    "# Do not edit by hand. Regenerate: python -m oopsmath_stage.schema_export > stage.schema.yaml\n"
)


def render() -> str:
    return HEADER + yaml.safe_dump(STAGE_SCHEMA, sort_keys=False, allow_unicode=True, width=100)


if __name__ == "__main__":
    sys.stdout.write(render())
