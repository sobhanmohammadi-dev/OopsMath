# Localization in the stage compiler

Two tools touch a stage's text. Each thing has **one owner**, so nothing is copied
and nothing can drift:

| What | Owner | Where it lives |
|---|---|---|
| The **English** (source) strings of a stage | the stage author, validated by the compiler | `<stage>/localization/en-US.ftl` |
| **Translations** of a stage | translators, with `tools/oopsmath-i18n` (`omsi`) | `translations/<lang>/levels/<stage id>.po` -> `assets/i18n/<lang>/levels/<stage id>.ftl` |
| Strings of the **game** (menus, HUD, help, block names) | `omsi` | `assets/i18n/<lang>/core.ftl` |
| Putting them together in a `.dat` | the compiler (`omsc build`) | `stages/<id>.dat` |

```
stage source ── en-US.ftl ──────────────┐                          (author edits)
                                        ├─► omsc build ─► stages/<id>.dat   (self-contained)
omsi: .pot ─► .po ─► levels/<id>.ftl ───┘   (bakes assets/i18n/*/levels/<id>.ftl)
```

## What `omsc build` does with translations

1. Reads the stage's own `localization/*.ftl` as before (syntax, namespace, every key
   a stage references must exist; `en-US.ftl` is the source language).
2. **Bakes language packs:** for every `assets/i18n/<lang>/levels/<stage id>.ftl` it finds,
   it adds `<lang>` to the stage's locales. The folder is looked up in the stage's
   parent folders (a stage inside the game repository finds `assets/i18n` by itself);
   `--i18n DIR` chooses another folder, `--no-i18n` builds from the stage's own files only.
3. Checks the baked file: valid FTL, keys that are not in the English source (warning),
   keys outside the stage namespace (warning). A language does **not** have to translate
   every key: the game shows English for the rest, so partial translations are normal and
   are not reported as missing (`--strict` stays green).
4. Prints a note such as `baked language-pack translations: fa (5/5) from assets/i18n`.

The `.dat` is therefore self-contained, and a translation update that is installed later
(a `.zip` dropped into `assets/i18n/`) still overrides what is baked: at runtime the
language packs win over the stage's compiled text, key by key.

## Precedence, from weakest to strongest

stage source `<locale>.ftl` &lt; language pack baked at build time &lt; language pack read by the game.
English always comes from the stage source (a pack cannot replace `en-US`).

## Rules for stage authors

* Write and maintain **only `en-US.ftl`**. Keep every key under the stage's namespace
  (`stage.<id>.*`).
* To make a stage translatable register it once: `omsi level-add <stage dir>` (this
  records the folder in `translations/levels.json`; the English text is never copied).
  The level id is the stage id.
* After changing English text run `omsi update <lang> --level <id>` (translators see which
  strings changed) and rebuild the stage.

## Migrating translations that live in a stage source

Older stages may contain `localization/fa.ftl`. They still compile (the compiler keeps
supporting extra locale files), but the build prints a note when a language pack also has
that language, because the pack wins. To migrate:

```
omsi level-add <stage dir>
omsi init fa --level <id>
omsi import-ftl fa --level <id> <stage dir>/localization/fa.ftl
omsi compile fa --level <id> --install
omsc build <stage dir> -o stages        # bakes it back; the .dat is equivalent
# then delete <stage dir>/localization/fa.ftl
```

## Why the compiler does not translate

Translating needs a workflow (templates, review, fuzzy matching, packs, fonts) that is the
same for the core game and for stages, and translators should not need the stage sources
or a build. Keeping that in one tool, and keeping the compiler a pure *consumer* of its
output, removes the duplicate copies and the "which file is right?" problem.
