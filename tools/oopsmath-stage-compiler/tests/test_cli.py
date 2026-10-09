"""CLI smoke tests (exit codes and output)."""

import json
import shutil
from pathlib import Path

from oopsmath_stage.cli import main

EXAMPLE = Path(__file__).resolve().parent.parent / "examples" / "001_first_wall"


def test_validate_ok(capsys):
    assert main(["--no-color", "validate", str(EXAMPLE), "--strict"]) == 0
    assert "Result: valid" in capsys.readouterr().out


def test_validate_json(capsys):
    assert main(["validate", str(EXAMPLE), "--format", "json"]) == 0
    assert json.loads(capsys.readouterr().out)["ok"] is True


def test_build_and_inspect(tmp_path, capsys):
    out = tmp_path / "x.dat"
    assert main(["build", str(EXAMPLE), "-o", str(out)]) == 0
    assert main(["inspect", str(out), "--dump", "STAG"]) == 0
    assert "001_first_wall" in capsys.readouterr().out


def test_build_all_creates_output_dir(tmp_path):
    assert main(["build-all", str(EXAMPLE.parent), "--output-dir", str(tmp_path / "new" / "dir")]) == 0
    assert (tmp_path / "new" / "dir" / "001_first_wall.dat").is_file()


def test_invalid_stage_exit_code(tmp_path):
    bad = tmp_path / "bad"
    shutil.copytree(EXAMPLE, bad)
    (bad / "stage.yaml").write_text("schema_version: 1\n", encoding="utf-8")
    assert main(["validate", str(bad)]) == 2


def test_bad_arguments():
    try:
        main(["validate"])
    except SystemExit as exc:
        assert exc.code == 3
