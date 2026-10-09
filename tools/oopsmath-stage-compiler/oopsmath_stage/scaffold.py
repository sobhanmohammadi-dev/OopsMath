"""`omsc new`: write a small, valid starter stage."""

from __future__ import annotations

import re
from pathlib import Path

from .errors import ConfigError

_ID_RE = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_.\-]*$")

_STAGE_YAML = """\
schema_version: 1

stage:
  id: __ID__
  title: stage.__ID__.title
  description: stage.__ID__.description
  difficulty: tutorial
  level: 1

learning:
  question: stage.__ID__.question
  curriculum:
    grade: {min: 7, max: 8}
    topics: [multiplication]
  lesson:
    markdown: lesson.md

world:
  bounds:
    size: [16, 8, 16]
  environment:
    preset: meadow

construction:
  allowed_blocks:
    - {id: brick, min_count: 0, max_count: 20}
  allowed_tools:
    - {id: hammer}
  starting_inventory:
    tools: {hammer: 1}
  targets:
    - id: wall_a
      type: wall
      region: {min: [0, 0, 0], max: [3, 2, 0]}
      required_block: brick

economy:
  budget: 100

objectives:
  tasks:
    - id: t_math
      type: math
      math:
        type: multiplication
        answer: {mode: exact, value: 12}
    - {id: t_build, type: construction, depends_on: [t_math], target: wall_a}
  completion:
    all:
      - task_completed: t_math
      - target_completed: wall_a

rewards:
  money: 100
  xp: 10

localization:
  ftl: localization
  namespace: stage.__ID__
"""

_FTL = {
    "en-US": ("# Strings for stage __ID__\n"
              "stage.__ID__.title = My first stage\n"
              "stage.__ID__.description = Build a wall.\n"
              "stage.__ID__.question = How many bricks do we need?\n"),
    "fa": ("# Strings for stage __ID__\n"
           "stage.__ID__.title = \u0645\u0631\u062d\u0644\u0647 \u0627\u0648\u0644\n"
           "stage.__ID__.description = \u06cc\u06a9 \u062f\u06cc\u0648\u0627\u0631 \u0628\u0633\u0627\u0632.\n"
           "stage.__ID__.question = \u0686\u0646\u062f \u0622\u062c\u0631 \u0644\u0627\u0632\u0645 \u0627\u0633\u062a\u061f\n"),
}

_LESSON = "# Lesson\n\nExplain the idea here. A wall 4 bricks wide and 3 bricks high needs 4 x 3 = 12 bricks.\n"


def create_stage(directory: Path, stage_id: str) -> list[Path]:
    """Create a starter stage in ``directory`` (must not already contain stage.yaml)."""
    if not _ID_RE.match(stage_id) or len(stage_id) > 128:
        raise ConfigError(f"'{stage_id}' is not a valid stage id "
                          "(letters, digits, '_', '.', '-'; must not start with '.' or '-')")
    if (directory / "stage.yaml").exists():
        raise ConfigError(f"'{directory / 'stage.yaml'}' already exists; refusing to overwrite")
    files = {"stage.yaml": _STAGE_YAML, "lesson.md": _LESSON}
    files.update({f"localization/{loc}.ftl": text for loc, text in _FTL.items()})
    created: list[Path] = []
    for rel, text in files.items():
        target = directory / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text.replace("__ID__", stage_id), encoding="utf-8", newline="\n")
        created.append(target)
    return created
