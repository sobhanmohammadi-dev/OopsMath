"""Baking language-pack translations into stages (see docs/LOCALIZATION.md)."""

import shutil
from pathlib import Path

import pytest

from oopsmath_stage.cli import main
from oopsmath_stage.util import unpack_msgpack
from oopsmath_stage.workflow import build_one, make_options, read_dat

EXAMPLE = Path(__file__).resolve().parent.parent / "examples" / "001_first_wall"
SOURCE = (EXAMPLE / "localization" / "en-US.ftl").read_text(encoding="utf-8")
STAGE_ID = "001_first_wall"


def english_keys():
    return [l.split("=")[0].strip() for l in SOURCE.splitlines() if "=" in l and not l.startswith("#")]


@pytest.fixture()
def repo(tmp_path):
    """A mini repository: a stage with only its English source, plus assets/i18n."""
    stage = tmp_path / "stages_src" / STAGE_ID
    shutil.copytree(EXAMPLE, stage)
    (stage / "localization" / "fa.ftl").unlink(missing_ok=True)
    packs = tmp_path / "assets" / "i18n"
    (packs / "fa" / "levels").mkdir(parents=True)
    return stage, packs


def pack_file(packs, lang, text):
    path = packs / lang / "levels" / f"{STAGE_ID}.ftl"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def build(stage, out=None, **options):
    return build_one(stage, out, make_options(**options), write=out is not None)


def locl(path):
    return {k: bytes(v).decode("utf-8") for k, v in unpack_msgpack(read_dat(path).get("LOCL").raw).items()}


def test_packs_are_found_in_a_parent_folder_and_baked(repo, tmp_path):
    stage, packs = repo
    keys = english_keys()
    pack_file(packs, "fa", f"{keys[0]} = ترجمه\n")
    out = tmp_path / "out.dat"
    outcome = build(stage, out)
    assert outcome.result.diag.ok, outcome.result.diag.errors
    assert any(f"baked language-pack translations: fa (1/{len(keys)})" in n for n in outcome.result.diag.notes)
    assert sorted(locl(out)) == ["en-US", "fa"]
    assert f"{keys[0]} = ترجمه" in locl(out)["fa"]
    meta = unpack_msgpack(read_dat(out).get("META").raw)
    assert meta["locales"] == ["en-US", "fa"]


def test_partial_translations_never_warn_so_strict_builds_pass(repo):
    stage, packs = repo
    pack_file(packs, "fa", f"{english_keys()[0]} = x\n")
    assert main(["--no-color", "validate", str(stage), "--strict"]) == 0


def test_unknown_keys_and_syntax_errors_are_reported(repo):
    stage, packs = repo
    pack_file(packs, "fa", "stage.001_first_wall.nonsense = x\n")
    outcome = build(stage)
    assert any("not in the English source" in w for w in outcome.result.diag.warnings)
    pack_file(packs, "fa", "this is not ftl\n")
    assert not build(stage).result.diag.ok


def test_no_i18n_and_an_explicit_folder(repo, tmp_path):
    stage, packs = repo
    key = english_keys()[0]
    pack_file(packs, "fa", f"{key} = x\n")
    assert not build(stage, no_i18n=True).result.diag.notes
    elsewhere = tmp_path / "elsewhere"
    pack_file(elsewhere, "de", f"{key} = y\n")
    notes = " ".join(build(stage, i18n=str(elsewhere)).result.diag.notes)
    assert "de (1/" in notes and "fa (" not in notes
    missing = build(stage, i18n=str(tmp_path / "nope"))
    assert any("not a directory" in e for e in missing.result.diag.errors)


def test_a_pack_wins_over_a_translation_left_in_the_stage_source(repo, tmp_path):
    stage, packs = repo
    key = english_keys()[0]
    (stage / "localization" / "fa.ftl").write_text(f"{key} = old\n", encoding="utf-8")
    pack_file(packs, "fa", f"{key} = new\n")
    out = tmp_path / "x.dat"
    outcome = build(stage, out)
    assert outcome.result.diag.ok
    assert any("the language pack wins per key" in n for n in outcome.result.diag.notes)
    assert locl(out)["fa"].rstrip().endswith(f"{key} = new")  # the game keeps the last definition


def test_the_english_source_is_never_replaced_by_a_pack(repo, tmp_path):
    stage, packs = repo
    pack_file(packs, "en-US", f"{english_keys()[0]} = HACK\n")
    out = tmp_path / "x.dat"
    build(stage, out)
    assert "HACK" not in locl(out)["en-US"]


def test_a_stage_without_packs_builds_exactly_as_before(repo, tmp_path):
    stage, packs = repo
    shutil.rmtree(packs.parent)  # no assets/i18n anywhere above the stage
    out = tmp_path / "x.dat"
    outcome = build(stage, out)
    assert outcome.result.diag.ok and not outcome.result.diag.notes
    assert sorted(locl(out)) == ["en-US"]


def test_builds_are_reproducible(repo, tmp_path):
    stage, packs = repo
    pack_file(packs, "fa", f"{english_keys()[0]} = x\n")
    a, b = tmp_path / "a.dat", tmp_path / "b.dat"
    build(stage, a)
    build(stage, b)
    assert a.read_bytes() == b.read_bytes()
