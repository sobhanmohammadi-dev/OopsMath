"""Compiler 1.1: block-count objectives, math prompts and challenge prerequisites."""

import copy
import shutil
from pathlib import Path

import msgpack
import pytest
import yaml

from oopsmath_stage.compiler import compile_stage
from oopsmath_stage.dat import parse_dat
from oopsmath_stage.registry import CompileOptions
from oopsmath_stage.validation.driver import validate_stage

EXAMPLE = Path(__file__).resolve().parents[1] / "examples" / "001_first_wall"


def _stage(tmp_path, edit=None):
    target = tmp_path / "stage"
    shutil.copytree(EXAMPLE, target)
    path = target / "stage.yaml"
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if edit:
        edit(data)
    path.write_text(yaml.safe_dump(data, sort_keys=False, allow_unicode=True), encoding="utf-8")
    return target


def _errors(tmp_path, edit=None):
    return validate_stage(_stage(tmp_path, edit), CompileOptions()).diag.errors


def _logs_task(data):
    return next(t for t in data["objectives"]["tasks"] if t["id"] == "t_logs")


def test_example_is_valid_and_has_no_warnings(tmp_path):
    result = validate_stage(_stage(tmp_path), CompileOptions())
    assert result.diag.errors == [] and result.diag.warnings == []


def test_example_round_trips_through_the_dat(tmp_path):
    result = validate_stage(_stage(tmp_path), CompileOptions())
    blob = compile_stage(result.source)
    parsed = parse_dat(blob)
    assert parsed.is_valid and parsed.header.schema_version == 1 and parsed.header.format_version == 1
    stag = next(s for s in parsed.sections if s.type == "STAG")
    stage = msgpack.unpackb(stag.raw, raw=False)
    task = next(t for t in stage["objectives"]["tasks"] if t["id"] == "t_logs")
    assert task["data"] == {
        "kind": "place_block", "block": "block.wood_log", "count": 12,
        "title": "stage.001_first_wall.objective.logs", "requires_challenge": "t_math",
    }
    assert stage["construction"]["starting_inventory"]["blocks"] == {"block.wood_log": 12}


def test_math_prompt_is_accepted_and_its_key_must_exist(tmp_path):
    def with_prompt(d):
        d["objectives"]["tasks"][0]["math"]["prompt"] = "stage.001_first_wall.question"
    assert _errors(tmp_path, with_prompt) == []

    def missing_key(d):
        d["objectives"]["tasks"][0]["math"]["prompt"] = "stage.001_first_wall.nope"
    assert any("stage.001_first_wall.nope" in e for e in _errors(tmp_path / "b", missing_key))


@pytest.mark.parametrize("change", [
    lambda t: t["data"].update(kind="bogus"),
    lambda t: t["data"].update(count=0),
    lambda t: t["data"].update(count=1.5),
    lambda t: t["data"].pop("block"),
    lambda t: t["data"].pop("count"),
    lambda t: t["data"].update(requires_challenge="t_build"),      # not a math task
    lambda t: t["data"].update(requires_challenge="ghost"),         # unknown task
    lambda t: t["data"].update(title="stage.001_first_wall.missing"),
])
def test_invalid_block_count_objectives_are_rejected(tmp_path, change):
    assert _errors(tmp_path, lambda d: change(_logs_task(d))) != []


def test_block_must_be_allowed_by_the_stage(tmp_path):
    def other_block(d):
        _logs_task(d)["data"]["block"] = "block.stone"
    errors = _errors(tmp_path, other_block)
    assert any("allowed_blocks" in e for e in errors)


def test_duplicate_task_ids_are_still_rejected(tmp_path):
    def dup(d):
        d["objectives"]["tasks"].append(copy.deepcopy(_logs_task(d)))
    assert _errors(tmp_path, dup) != []


def test_legacy_stage_without_the_new_fields_still_validates(tmp_path):
    def strip(d):
        d["objectives"]["tasks"] = [t for t in d["objectives"]["tasks"] if t["id"] != "t_logs"]
        d["objectives"]["completion"]["all"] = [
            c for c in d["objectives"]["completion"]["all"] if c.get("task_completed") != "t_logs"]
    assert _errors(tmp_path, strip) == []


def test_other_construction_data_is_left_alone(tmp_path):
    """`data` stays free-form for tasks that do not use `kind`."""
    def custom(d):
        d["objectives"]["tasks"].append(
            {"id": "t_free", "type": "construction", "target": "wall_a", "data": {"anything": [1, 2]}})
    assert _errors(tmp_path, custom) == []



# ----------------------------------------------------------- blueprints (1.2)

def _blueprint(data):
    return data["construction"]["blueprints"][0]


def test_example_blueprint_is_valid_and_survives_the_dat(tmp_path):
    result = validate_stage(_stage(tmp_path), CompileOptions())
    assert result.diag.errors == [] and result.diag.warnings == []
    parsed = parse_dat(compile_stage(result.source))
    stag = next(s for s in parsed.sections if s.type == "STAG")
    stage = msgpack.unpackb(stag.raw, raw=False)
    blueprint = stage["construction"]["blueprints"][0]
    assert blueprint["id"] == "bp_wall_a" and len(blueprint["blocks"]) == 8
    assert blueprint["blocks"][0] == {"block": "block.wood_log", "position": [0, 0, 0]}


def test_stages_without_blueprints_still_validate(tmp_path):
    assert _errors(tmp_path, lambda d: d["construction"].pop("blueprints")) == []


@pytest.mark.parametrize("change", [
    lambda b: b["blocks"][0].update(rotation=45),                   # not a quarter turn
    lambda b: b["blocks"][0].update(position=[0, 0]),                # not a cell
    lambda b: b["blocks"][0].update(position=[99, 0, 0]),            # outside the world
    lambda b: b["blocks"][0].update(position=[-1, 0, 0]),
    lambda b: b["blocks"][1].update(position=[0, 0, 0]),            # same anchor cell twice
    lambda b: b["blocks"][0].pop("block"),
    lambda b: b.update(blocks=[]),                                   # empty plan
    lambda b: b.update(title="stage.001_first_wall.missing"),        # unknown translation key
    lambda b: b["blocks"][0].update(block="block.stone"),            # not allowed in the stage
    lambda b: b.pop("id"),
])
def test_invalid_blueprints_are_rejected(tmp_path, change):
    assert _errors(tmp_path, lambda d: change(_blueprint(d))) != []


def test_duplicate_blueprint_ids_are_rejected(tmp_path):
    def dup(d):
        d["construction"]["blueprints"].append(copy.deepcopy(_blueprint(d)))
    assert any("bp_wall_a" in e for e in _errors(tmp_path, dup))
