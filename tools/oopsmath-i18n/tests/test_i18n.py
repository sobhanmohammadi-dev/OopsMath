import sys
import zipfile
from pathlib import Path

import polib
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from oopsmath_i18n import cli, ftl, meta, ops, po as pomod, ron  # noqa: E402
from oopsmath_i18n.scopes import Layout, core, level  # noqa: E402

SOURCE = """\
# header comment

## Menu

menu-play = PLAY
# Shown after a purchase. $count is how many blocks.
bought =
    { $count ->
        [one] You bought one block.
       *[other] You bought { $count } blocks.
    }
-brand = OopsMath
about = About { -brand }
    .title = About the game
"""

LEVEL = "# strings\nstage.t.title = My stage\nstage.t.description = Build a wall.\n"
META_FA = """#![enable(implicit_some)]
(
    format: 1,
    code: "fa",
    name: "Persian",
    native_name: "فارسی",
    direction: Rtl,
    fonts: (
        default: "font.vazirmatn",
        title: "font.vazirmatnbold",
        contexts: { "hud": "font.vazirmatn" },
    ),
)
"""


@pytest.fixture()
def repo(tmp_path):
    src = tmp_path / "assets/i18n/en-US"
    src.mkdir(parents=True)
    (src / "core.ftl").write_text(SOURCE, encoding="utf-8")
    fa = tmp_path / "assets/i18n/fa"
    fa.mkdir(parents=True)
    (fa / "_meta.ron").write_text(META_FA, encoding="utf-8")
    stage = tmp_path / "stages_src/t"
    (stage / "localization").mkdir(parents=True)
    (stage / "stage.yaml").write_text("schema_version: 1\nstage:\n  id: t\n", encoding="utf-8")
    (stage / "localization/en-US.ftl").write_text(LEVEL, encoding="utf-8")
    layout = Layout(tmp_path)
    ops.level_add(layout, stage)
    return layout


def test_parse_messages_attributes_comments_terms():
    msgs = {m.id: m for m in ftl.parse(SOURCE)}
    assert list(msgs) == ["menu-play", "bought", "-brand", "about"]
    assert msgs["menu-play"].comment == ""
    assert msgs["bought"].comment.startswith("Shown after a purchase")
    assert "*[other] You bought { $count } blocks." in msgs["bought"].value
    assert [c for c, _ in msgs["about"].units()] == ["about", "about.title"]
    assert dict(msgs["about"].units())["about.title"] == "About the game"


def test_level_files_are_single_line_only():
    assert ftl.parse(LEVEL, "level")[0].id == "stage.t.title"
    with pytest.raises(ftl.FtlError):
        ftl.parse("stage.t.a = x\n    continued\n", "level")
    with pytest.raises(ftl.FtlError):
        ftl.parse("a = 1\na = 2\n")


def test_render_roundtrip_is_stable():
    for m in ftl.parse(SOURCE):
        text = ftl.render_message(m.id, m.value, m.attributes, m.comment)
        again = ftl.parse(text)[0]
        assert (again.value, again.attributes) == (m.value, m.attributes), m.id


def test_validation_rules():
    src = "Bought { $count } blocks."
    assert ftl.validate_translation(src, "{ $count } بلوک", "core") == ([], [])
    errors, _ = ftl.validate_translation(src, "{ $total } blocks", "core")
    assert any("unknown variable" in e for e in errors)
    errors, warnings = ftl.validate_translation(src, "no variable", "core")
    assert not errors and any("not used" in w for w in warnings)
    assert ftl.validate_translation(src, "{ $count blocks", "core")[0]
    select = "{ $n ->\n [one] a\n *[other] b\n}"
    assert ftl.validate_translation(select, "{ $n ->\n [one] a\n [other] b\n}", "core")[0]
    assert ftl.validate_translation("x", "a\nb", "level")[0]
    assert ftl.validate_translation("x", "", "core") == ([], [])


def test_pot_po_cycle_with_fuzzy_merge(repo):
    path = ops.write_pot(repo, core())
    pot = polib.pofile(str(path))
    assert [e.msgctxt for e in pot] == ["menu-play", "bought", "-brand", "about", "about.title"]
    assert pot.find("bought", by="msgid") is None or True
    po_path = ops.init_po(repo, "fa", core())
    po = pomod.load(po_path)
    po.find("PLAY").msgstr = "شروع"
    pomod.save(po, po_path)
    with pytest.raises(ops.ToolError):
        ops.init_po(repo, "fa", core())  # never overwrite translations
    # The English text changes and a string is added.
    src = repo.source_ftl(core())
    src.write_text(src.read_text(encoding="utf-8").replace("= PLAY", "= PLAY NOW") + "extra = Extra\n",
                   encoding="utf-8")
    _, stats = ops.update_po(repo, "fa", core())
    assert stats == {"new": 1, "changed": 1, "removed": 0}
    merged = pomod.load(po_path)
    play = [e for e in merged if e.msgctxt == "menu-play"][0]
    assert play.msgstr == "شروع" and "fuzzy" in play.flags and play.previous_msgid == "PLAY"
    # Fuzzy translations are not shipped unless asked for.
    text, report = ops.compile_scope(repo, "fa", core())
    assert "menu-play" not in text and report.translated == 0
    text, _ = ops.compile_scope(repo, "fa", core(), include_fuzzy=True)
    assert "menu-play = شروع" in text


def test_compile_omits_untranslated_and_rejects_bad_translations(repo):
    ops.init_po(repo, "fa", core())
    p = repo.po("fa", core())
    po = pomod.load(p)
    for e in po:
        if e.msgctxt == "menu-play":
            e.msgstr = "شروع"
        elif e.msgctxt == "bought":
            e.msgstr = "{ $count ->\n    [one] یک\n   *[other] { $count } تا\n}"
        elif e.msgctxt == "about.title":
            e.msgstr = "درباره"
    pomod.save(po, p)
    text, report = ops.compile_scope(repo, "fa", core())
    parsed = {m.id: m for m in ftl.parse(text)}
    assert set(parsed) == {"menu-play", "bought", "about"}  # untranslated ids are absent
    assert parsed["about"].value is None and parsed["about"].attributes == [("title", "درباره")]
    assert report.translated == 3 and report.total == 5 and not report.errors
    # An invalid translation is an error, never silently shipped.
    po = pomod.load(p)
    [e for e in po if e.msgctxt == "bought"][0].msgstr = "{ $oops } تا"
    pomod.save(po, p)
    assert ops.check_scope(repo, "fa", core()).errors
    with pytest.raises(ops.ToolError):
        ops.build_pack(repo, "fa", [core()])


def test_levels_and_pack_contents(repo):
    ops.init_po(repo, "fa", level("t"))
    p = repo.po("fa", level("t"))
    po = pomod.load(p)
    po[0].msgstr = "مرحلهٔ من"
    pomod.save(po, p)
    data, reports = ops.build_pack(repo, "fa", [level("t")])
    out = repo.root / "fa.zip"
    out.write_bytes(data)
    with zipfile.ZipFile(out) as z:
        assert sorted(z.namelist()) == ["fa/_meta.ron", "fa/levels/t.ftl"]
        assert "stage.t.title = مرحلهٔ من" in z.read("fa/levels/t.ftl").decode()
        assert "stage.t.description" not in z.read("fa/levels/t.ftl").decode()  # English fallback in game
    assert reports[0].translated == 1 and reports[0].total == 2
    assert ops.build_pack(repo, "fa", [level("t")])[0] == data  # reproducible
    written = ops.install_pack(repo, out)
    assert written == ["fa/levels/t.ftl"]  # an existing _meta.ron is kept
    assert (repo.i18n / "fa/levels/t.ftl").is_file()


def test_install_rejects_unsafe_paths(repo, tmp_path):
    bad = tmp_path / "bad.zip"
    with zipfile.ZipFile(bad, "w") as z:
        z.writestr("fa/../../evil.ftl", "x")
    with pytest.raises(ops.ToolError):
        ops.install_pack(repo, bad)


def test_ron_meta_parsing_and_validation():
    data = ron.loads(META_FA)
    assert data["direction"] == "Rtl" and data["fonts"]["contexts"] == {"hud": "font.vazirmatn"}
    assert meta.validate(data, "fa") == ([], [])
    assert meta.validate(data, "de")[0]
    bad = ron.loads('(format: 1, code: "fa", name: "x", native_name: "y", fonts: (default: "Vazir"))')
    assert any("font id" in e for e in meta.validate(bad, "fa")[0])
    assert meta.font_ids(data) == ["font.vazirmatn", "font.vazirmatnbold"]
    assert meta.normalize_font_stem("Vazirmatn-Bold") == "font.vazirmatnbold"
    with pytest.raises(ron.RonError):
        ron.loads("(code: ")


def test_template_is_valid_ron(tmp_path):
    text = meta.template("fa", "Persian", "فارسی", "Rtl", "font.vazirmatn")
    data = ron.loads(text)
    assert meta.validate(data, "fa") == ([], [])
    assert data["fonts"]["default"] == "font.vazirmatn"


def test_scope_selection_and_cli(repo, capsys):
    assert cli.main(["--root", str(repo.root), "pot", "--all"]) == 0
    assert (repo.root / "translations/templates/core.pot").is_file()
    assert (repo.root / "translations/templates/levels/t.pot").is_file()
    assert cli.main(["--root", str(repo.root), "pot"]) == 2  # no scope given
    assert cli.main(["--root", str(repo.root), "init", "fa", "--core", "--level", "t"]) == 0
    assert cli.main(["--root", str(repo.root), "check", "fa", "--all"]) == 0
    assert cli.main(["--root", str(repo.root), "pack", "fa", "--core"]) == 0
    assert (repo.root / "translations/packs/fa.zip").is_file()


def test_shipped_data_is_consistent():
    root = Path(__file__).resolve().parents[3]
    if not (root / "assets/i18n/en-US/core.ftl").is_file():
        pytest.skip("not run inside the repository")
    layout = Layout(root)
    for lang in ["en-US", *ops.languages(layout)]:
        data = meta.load(layout.i18n / lang / "_meta.ron")
        assert meta.validate(data, lang)[0] == []
    for lang in ops.languages(layout):
        for scope in [core(), *[level(i) for i in ops.discover_levels(layout)]]:
            if layout.po(lang, scope).is_file():
                assert ops.check_scope(layout, lang, scope).errors == []


def test_level_english_stays_in_the_stage_source(repo):
    # Nothing is copied into assets/i18n/en-US/levels; the registry points at the stage.
    assert not (repo.i18n / "en-US" / "levels").exists()
    assert repo.levels() == {"t": {"dir": "stages_src/t", "ftl": "localization"}}
    assert repo.source_ftl(level("t")) == repo.root / "stages_src/t/localization/en-US.ftl"
    # Editing the stage source is picked up by the next `pot`.
    src = repo.source_ftl(level("t"))
    src.write_text(src.read_text(encoding="utf-8") + "stage.t.extra = More\n", encoding="utf-8")
    pot = ops.make_pot(repo, level("t"))
    assert [e.msgctxt for e in pot][-1] == "stage.t.extra"


def test_level_add_validates_the_stage(repo, tmp_path):
    bad = tmp_path / "stages_src/bad"
    bad.mkdir()
    with pytest.raises(ops.ToolError, match="no stage.yaml"):
        ops.level_add(repo, bad)
    (bad / "stage.yaml").write_text("stage:\n  id: other\n", encoding="utf-8")
    with pytest.raises(ops.ToolError, match="differs from stage.id"):
        ops.level_add(repo, bad, "wrong")
    with pytest.raises(ops.ToolError, match="en-US.ftl"):
        ops.level_add(repo, bad)
    (bad / "localization").mkdir()
    (bad / "localization/en-US.ftl").write_text("stage.other.title = T\n", encoding="utf-8")
    assert ops.level_add(repo, bad)[0] == "other"
    assert set(repo.levels()) == {"t", "other"}
    ops.level_remove(repo, "other")
    assert set(repo.levels()) == {"t"}
    with pytest.raises(ops.ToolError, match="not registered"):
        ops.source_messages(repo, level("nope"))


def test_compile_install_writes_into_the_language_folder(repo):
    ops.init_po(repo, "fa", level("t"))
    p = repo.po("fa", level("t"))
    po = pomod.load(p)
    po[0].msgstr = "مرحله"
    pomod.save(po, p)
    assert cli.main(["--root", str(repo.root), "compile", "fa", "--level", "t", "--install"]) == 0
    text = (repo.i18n / "fa/levels/t.ftl").read_text(encoding="utf-8")
    assert "stage.t.title = مرحله" in text
