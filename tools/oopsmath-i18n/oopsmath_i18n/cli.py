"""Command line interface: `python omsi.py <command> ...`."""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from . import __version__, meta, ops
from .scopes import Layout, Scope, core, find_root, level

EPILOG = """\
Typical workflow
  omsi level-add <stage dir>                  (levels only) register a stage's English source
  omsi pot --all                              extract .pot templates from the English source
  omsi lang-new fa --name Persian --native فارسی --direction Rtl --font font.vazirmatn
  omsi init fa --all                          create translations/fa/*.po
  (translate the .po files with Poedit, Weblate, or any text editor)
  omsi check fa --all                         validate
  omsi pack fa --all -o fa.zip                build the language pack the game reads
  omsi compile fa --all --install             or write the .ftl straight into assets/i18n/fa/
  omsc build <stage dir> -o stages            the stage compiler bakes the level translations in

Scopes: --core is the game itself; --level ID is one stage (repeatable).
"""


def _scope_args(p: argparse.ArgumentParser, required: bool = True) -> None:
    g = p.add_argument_group("scope")
    g.add_argument("--core", action="store_true", help="the core game strings (assets/i18n/<lang>/core.ftl)")
    g.add_argument("--level", action="append", default=[], metavar="ID",
                   help="a level (stage) id, e.g. 001_first_wall; repeatable")
    g.add_argument("--all", action="store_true", help="core plus every level that has an English source")


def _scopes(args, layout: Layout) -> list[Scope]:
    scopes: list[Scope] = []
    if args.all:
        scopes = [core()] + [level(i) for i in ops.discover_levels(layout)]
    else:
        if args.core:
            scopes.append(core())
        scopes += [level(i) for i in args.level]
    if not scopes:
        raise ops.ToolError("choose a scope: --core, --level ID, or --all")
    return scopes


def _p(msg: str = "") -> None:
    print(msg)


def cmd_pot(args, layout):
    for s in _scopes(args, layout):
        path = ops.write_pot(layout, s)
        _p(f"wrote {path.relative_to(layout.root)}  ({s.name})")


def cmd_lang_new(args, layout):
    if not meta.LANG_CODE.match(args.code):
        raise ops.ToolError(f"'{args.code}' is not a language tag like 'fa', 'de' or 'pt-BR'")
    path = layout.i18n / args.code / "_meta.ron"
    if path.exists() and not args.force:
        raise ops.ToolError(f"{path.relative_to(layout.root)} already exists (use --force)")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(meta.template(args.code, args.name, args.native, args.direction, args.font), encoding="utf-8")
    _p(f"wrote {path.relative_to(layout.root)}")
    _p(f"next: omsi init {args.code} --all")


def cmd_init(args, layout):
    for s in _scopes(args, layout):
        path = ops.init_po(layout, args.lang, s, args.force)
        _p(f"created {path.relative_to(layout.root)}  ({s.name})")


def cmd_update(args, layout):
    for s in _scopes(args, layout):
        path, st = ops.update_po(layout, args.lang, s)
        _p(f"updated {path.relative_to(layout.root)}: {st['new']} new, {st['changed']} changed (fuzzy), "
           f"{st['removed']} removed")


def _print_report(lang: str, r: ops.Report) -> None:
    _p(f"{lang:8} {r.scope.name:24} {r.translated:4}/{r.total:<4} {r.percent:5.1f}%"
       + (f"  fuzzy {r.fuzzy}" if r.fuzzy else ""))
    for e in r.errors:
        _p(f"    ERROR   {e}")
    for w in r.warnings:
        _p(f"    warning {w}")


def cmd_check(args, layout):
    bad = False
    path, data = ops.language_meta(layout, args.lang)
    _, warns = meta.validate(data, args.lang)
    for w in warns + ops.check_fonts(layout, args.lang, data):
        _p(f"    warning {path.name}: {w}")
    for s in _scopes(args, layout):
        r = ops.check_scope(layout, args.lang, s)
        _print_report(args.lang, r)
        bad |= bool(r.errors)
    return 1 if bad else 0


def cmd_compile(args, layout):
    out_root = (layout.i18n / args.lang if args.install
                else Path(args.out) if args.out else layout.build() / args.lang)
    for s in _scopes(args, layout):
        text, r = ops.compile_scope(layout, args.lang, s, args.include_fuzzy)
        _print_report(args.lang, r)
        if r.errors:
            raise ops.ToolError("fix the errors above first")
        dest = out_root / s.ftl_rel()
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text, encoding="utf-8")
        _p(f"wrote {dest}")


def cmd_pack(args, layout):
    scopes = _scopes(args, layout)
    data, reports = ops.build_pack(layout, args.lang, scopes, args.with_fonts, args.include_fuzzy)
    out = Path(args.out) if args.out else layout.work / "packs" / f"{args.lang}.zip"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(data)
    for r in reports:
        _print_report(args.lang, r)
    _p(f"wrote {out}  ({len(data)} bytes)")
    _p("install: drop the zip into assets/i18n/ (next to the language folders) or run `omsi install`.")


def cmd_install(args, layout):
    written = ops.install_pack(layout, Path(args.zip), args.overwrite_meta)
    for w in written:
        _p(f"installed assets/i18n/{w}")


def cmd_stats(args, layout):
    langs = [args.lang] if args.lang else ops.languages(layout)
    scopes = _scopes(args, layout) if (args.all or args.core or args.level) else (
        [core()] + [level(i) for i in ops.discover_levels(layout)])
    for lang, r in ops.stats(layout, langs, scopes):
        _print_report(lang, r)


def cmd_level_add(args, layout):
    stage_id, english = ops.level_add(layout, Path(args.stage_dir), args.id)
    _p(f"registered level '{stage_id}' (English source: {english.relative_to(layout.root).as_posix()})")
    _p(f"next: omsi pot --level {stage_id}   then   omsi init <lang> --level {stage_id}")


def cmd_level_remove(args, layout):
    ops.level_remove(layout, args.id)
    _p(f"unregistered level '{args.id}' (translation files were left in place)")


def cmd_levels(args, layout):
    levels = layout.levels()
    if not levels:
        _p("no levels registered; add one with: omsi level-add <stage dir>")
    for lid, entry in sorted(levels.items()):
        ok = layout.source_ftl(level(lid)).is_file()
        _p(f"{lid:28} {entry['dir']}/{entry.get('ftl', 'localization')}  {'' if ok else '(MISSING)'}")


def cmd_import_ftl(args, layout):
    (scope,) = _scopes(args, layout)
    path, n = ops.import_ftl(layout, args.lang, scope, Path(args.file))
    _p(f"imported {n} translation(s) into {path.relative_to(layout.root)}")


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="omsi", description="OopsMath localization tool",
                                 epilog=EPILOG, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--version", action="version", version=f"omsi {__version__}")
    ap.add_argument("--root", help="repository root (default: found automatically)")
    sub = ap.add_subparsers(dest="command", required=True)

    def add(name, fn, help_, scope=True):
        p = sub.add_parser(name, help=help_)
        p.set_defaults(fn=fn)
        if scope:
            _scope_args(p)
        return p

    add("pot", cmd_pot, "extract .pot template(s) from the English source")
    p = add("lang-new", cmd_lang_new, "create assets/i18n/<code>/_meta.ron for a new language", scope=False)
    p.add_argument("code"); p.add_argument("--name", required=True, help="English name, e.g. Persian")
    p.add_argument("--native", required=True, help="name in the language itself")
    p.add_argument("--direction", choices=["Ltr", "Rtl"], default="Ltr")
    p.add_argument("--font", help="default font id, e.g. font.vazirmatn")
    p.add_argument("--force", action="store_true")
    p = add("init", cmd_init, "create the .po file(s) of a language from the template")
    p.add_argument("lang"); p.add_argument("--force", action="store_true")
    p = add("update", cmd_update, "merge a changed English source into existing .po file(s)")
    p.add_argument("lang")
    p = add("check", cmd_check, "validate .po file(s) and the language metadata")
    p.add_argument("lang")
    p = add("compile", cmd_compile, "convert .po file(s) into game-ready .ftl files")
    p.add_argument("lang"); p.add_argument("-o", "--out", help="output folder (default translations/build/<lang>)")
    p.add_argument("--install", action="store_true",
                   help="write straight into assets/i18n/<lang>/ (what the stage compiler and the game read)")
    p.add_argument("--include-fuzzy", action="store_true")
    p = add("pack", cmd_pack, "build the .zip language pack the game reads")
    p.add_argument("lang"); p.add_argument("-o", "--out", help="zip path (default translations/packs/<lang>.zip)")
    p.add_argument("--with-fonts", action="store_true", help="also include the font files named in _meta.ron "
                   "(only if their licence allows redistribution)")
    p.add_argument("--include-fuzzy", action="store_true")
    p = add("install", cmd_install, "unpack a language pack into assets/i18n", scope=False)
    p.add_argument("zip"); p.add_argument("--overwrite-meta", action="store_true")
    p = add("stats", cmd_stats, "translation coverage")
    p.add_argument("lang", nargs="?")
    p = add("level-add", cmd_level_add, "register a stage source folder as a translatable level", scope=False)
    p.add_argument("stage_dir")
    p.add_argument("--id", help="must equal the stage id; default: read from stage.yaml")
    p = add("level-remove", cmd_level_remove, "unregister a level", scope=False)
    p.add_argument("id")
    add("levels", cmd_levels, "list registered levels", scope=False)
    p = add("import-ftl", cmd_import_ftl, "seed a .po from an existing translated .ftl")
    p.add_argument("lang"); p.add_argument("file")
    return ap


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    layout = Layout(Path(args.root).resolve() if args.root else find_root(Path.cwd()))
    try:
        return args.fn(args, layout) or 0
    except ops.ToolError as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
