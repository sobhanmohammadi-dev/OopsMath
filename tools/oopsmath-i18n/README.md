# oopsmath-i18n (`omsi`)

Translation tool for OopsMath: extracts `.pot` templates from the English Fluent
source, manages `.po` files, validates them and packs them into the `.zip` language
packs the game reads. Full guide: [docs/i18n/README.md](../../docs/i18n/README.md).

```
pip install -r requirements.txt          # polib, PyYAML (+ pytest for the tests)
python omsi.py --help
```

| Command | What it does |
|---|---|
| `pot (--core / --level ID / --all)` | extract `translations/templates/*.pot` from `assets/i18n/en-US` |
| `lang-new CODE --name N --native N [--direction Rtl] [--font ID]` | create `assets/i18n/CODE/_meta.ron` |
| `init LANG scope` | create the `.po` file(s) from the template |
| `update LANG scope` | merge a changed English source into the `.po` (changed texts become fuzzy) |
| `check LANG scope` | validate placeholders, plural defaults, syntax; show coverage; check fonts |
| `compile LANG scope [-o DIR \| --install]` | write game-ready `.ftl` from the `.po` (`--install`: into `assets/i18n/<lang>/`) |
| `pack LANG scope [-o FILE] [--with-fonts]` | compile + validate + write the `.zip` pack |
| `install FILE.zip` | unpack a pack into `assets/i18n` |
| `stats [LANG]` | translation progress |
| `level-add STAGE_DIR` | register a stage source folder as a translatable level (the English stays in the stage; nothing is copied) |
| `level-remove ID` / `levels` | unregister / list levels |
| `import-ftl LANG scope FILE` | seed a `.po` from a translated `.ftl` |

Scope: `--core`, `--level ID` (repeatable) or `--all`. Exit codes: 0 ok, 1 `check`
found errors, 2 usage or file problem. Packs are reproducible (fixed timestamps,
sorted entries).

Tests: `python -m pytest -q tests`.

PO mapping: `msgctxt` = message id (`id.attribute` for attributes), `msgid` = the
English pattern, `msgstr` = the translation, `#.` = the Fluent comment,
`#:` = source line, `#, fuzzy` = English changed after the translation.
