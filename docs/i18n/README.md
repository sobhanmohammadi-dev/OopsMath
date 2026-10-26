# Localization

OopsMath ships with English as the source language and can show any number of
other languages. A language is data: a folder (or a `.zip`) with text and a small
metadata file. No code change and no rebuild is needed to add one.

* [How it works](#how-it-works)
* [Folder layout and file formats](#folder-layout-and-file-formats)
* [Translating (the .pot / .po workflow)](#translating-the-pot--po-workflow)
* [Translating a level](#translating-a-level)
* [Fonts and right-to-left languages](#fonts-and-right-to-left-languages)
* [For developers](#for-developers)
* [Limitations](#limitations)

## How it works

```
assets/i18n/en-US/core.ftl   <- the source of every core string (Fluent)
        |  omsi pot                      (tools/oopsmath-i18n)
        v
translations/templates/core.pot
        |  omsi init fa  ->  translate translations/fa/core.po
        v
translations/fa/core.po
        |  omsi pack fa  (compiles the .po, checks it, zips it)
        v
fa.zip  ->  drop into assets/i18n/   (or `omsi install fa.zip`)
        v
the game: language button on the main menu
```

* English (`en-US`) is the **reference**. Whatever a language does not translate
  is shown in English, **message by message**. A half-finished language is
  usable.
* Core strings (menus, HUD, windows, help) and **level** strings (a stage's title,
  description, objectives) are translated separately, so a level can ship or be
  translated on its own. The English of a level stays in its stage source and is
  never copied; translations are baked into the stage by the stage compiler and can
  still be updated later with a pack (see
  [the compiler's localization guide](../../tools/oopsmath-stage-compiler/docs/LOCALIZATION.md)).
* The player picks the language with the button in the corner of the main menu
  (shown when two or more languages are installed). The choice is remembered per
  user (`%APPDATA%\OopsMath\language.txt`, `~/.config/OopsMath/language.txt`,
  `~/Library/Application Support/OopsMath/language.txt`).
  `OOPSMATH_LOCALE=fa` overrides it for one run.

## Folder layout and file formats

```
assets/i18n/
  en-US/
    _meta.ron                  language data
    core.ftl                   all core strings (the English level text is in the stage sources)
  fa/
    _meta.ron
    core.ftl
    levels/001_first_wall.ftl
    fonts/                     (optional) fonts that belong to this language
  de.zip                       (optional) a pack with the same layout inside
assets/fonts/                  fonts shared by all languages
```

Folder names are BCP 47 tags: `en-US`, `fa`, `de`, `pt-BR`, `zh-Hans`.
`OOPSMATH_I18N_DIR` points the game at another folder.

Packs: every folder and every `*.zip` in `assets/i18n/` is read. For one language
the first valid `_meta.ron` wins; `core.ftl` and `levels/*.ftl` of later packs
override earlier ones message by message (so a small zip can update one level or
fix a few strings); the first font file for an id wins. A language without a
valid `_meta.ron` is ignored with a warning in the log.

### `_meta.ron`

```ron
#![enable(implicit_some)]
(
    format: 1,                  // always 1
    code: "fa",                 // must equal the folder name
    name: "Persian",            // English name
    native_name: "فارسی",       // shown on the language button
    direction: Rtl,             // Ltr | Rtl
    fallback: "en-US",          // for untranslated messages (default en-US)
    version: "1.0.0",
    authors: ["Your name"],
    fonts: (
        default: "font.vazirmatnregular",   // everything not listed below
        title: "font.vazirmatnbold",        // large headings
        heading: "font.vazirmatnbold",
        // body: ..., button: ...
        contexts: { "hud-numbers": "font.vazirmatnregular" },   // named contexts
    ),
)
```

Create one with `omsi lang-new fa --name Persian --native فارسی --direction Rtl
--font font.vazirmatnregular`.

### `core.ftl` (Fluent)

[Fluent](https://projectfluent.org) is a localization format that handles plurals,
variables and grammar:

```ftl
menu-play = PLAY
menu-language = Language: { $name }
blocks-bought =
    { $count ->
        [one] You bought one block.
       *[other] You bought { $count } blocks.
    }
-brand = OopsMath
about = About { -brand }
    .title = About the game
```

* Ids: lower-case words joined by `-` (no dots). Messages may have attributes
  (`.title`) and may use terms (`-brand`).
* `{ $name }` is a variable the game fills in. Keep the variable names; you may
  reorder them or leave one out.
* A `->` select must keep a default variant `*[other]`. Each language has its own
  plural categories (English `one`/`other`, Persian `one`/`other`, Russian
  `one`/`few`/`many`/`other`, ...): write the variants your language needs.
* A `# comment` line above a message is the note to translators; it is copied into
  the `.pot` file.

### `levels/<id>.ftl` (simple format)

Level files use the stage format the stage compiler writes: one line per string,
`stage.<id>.title = text`, dotted keys, no continuation lines, no placeables.

## Translating (the .pot / .po workflow)

Install the tool once: `pip install -r tools/oopsmath-i18n/requirements.txt`. All
commands run from the repository root as `python tools/oopsmath-i18n/omsi.py ...`
(shortened to `omsi` below). Scope flags: `--core`, `--level ID` (repeatable) or
`--all`.

1. **Extract the template** from the English source:
   `omsi pot --all` -> `translations/templates/core.pot` and
   `translations/templates/levels/<id>.pot`.
2. **New language?** `omsi lang-new de --name German --native Deutsch` (creates
   `assets/i18n/de/_meta.ron`).
3. **Create the .po:** `omsi init de --all` -> `translations/de/core.po`,
   `translations/de/levels/<id>.po`.
4. **Translate** the `.po` files with Poedit, Weblate, Lokalise, a text editor...
   Each entry has `msgctxt` (the message id), `msgid` (the English text),
   `msgstr` (yours), and the translator note.
5. **Validate:** `omsi check de --all`. Errors (unknown variable, unbalanced
   braces, missing `*[other]`, ...) block packing; warnings (unused variable,
   fuzzy, missing font) do not.
6. **Pack:** `omsi pack de --all -o de.zip` compiles the `.po` into `.ftl`, checks it
   and writes the zip (`_meta.ron`, `core.ftl`, `levels/*.ftl`). Untranslated
   messages are left out so the game uses English for them.
   Use `--with-fonts` to include the font files named in `_meta.ron` (only when
   their licence allows redistribution).
7. **Install:** drop `de.zip` into `assets/i18n/`, or `omsi install de.zip`, or run
   `omsi compile de --all` and commit the generated files into `assets/i18n/de/`
   (`omsi pack ... && omsi install` does the same in one go).

When the English text changes: `omsi update de --all` merges the new template into
your `.po`; translations whose English changed are kept and marked **fuzzy**
(review them, then remove the fuzzy flag; fuzzy entries are not packed unless you
pass `--include-fuzzy`). `omsi stats` shows the progress of every language.
`omsi import-ftl de --core file.ftl` seeds a `.po` from an existing `.ftl`.

## Translating a level

Who owns what (details: [compiler guide](../../tools/oopsmath-stage-compiler/docs/LOCALIZATION.md)):

* the **English** text of a level is the stage author's `localization/en-US.ftl`;
  `omsi` reads it from there, it is never copied;
* the **translations** are `.po` files managed by `omsi`, compiled to
  `assets/i18n/<lang>/levels/<stage id>.ftl`;
* `omsc build` bakes those files into the stage's `.dat`, so a stage is self-contained;
  a pack installed later still overrides it.

```
omsi level-add tools/oopsmath-stage-compiler/examples/001_first_wall   # once per level
omsi pot --level 001_first_wall
omsi init fa --level 001_first_wall                                    # translate the .po
omsi check fa --level 001_first_wall
omsi compile fa --level 001_first_wall --install                       # or: omsi pack ... && omsi install
python tools/oopsmath-stage-compiler/omsc.py build tools/oopsmath-stage-compiler/examples/001_first_wall -o stages
```

Level files use the simple stage format (`stage.<id>.title = text`, one line each); any
text a language lacks is shown in English. `omsi levels` lists the registered levels;
`omsi import-ftl` brings in a translation that still lives in a stage source.

## Fonts and right-to-left languages

* **Font ids.** `font.` + the font file name without extension, lower case, letters
  and digits only. `Vazirmatn-Bold.ttf` is `font.vazirmatnbold`. The game finds
  the files in `assets/fonts/`, in `assets/i18n/<lang>/fonts/` and inside language
  zips (`<lang>/fonts/`). A missing file is never fatal: one warning, built-in
  font. See `assets/fonts/README.md` (no font is bundled).
* **Roles.** `default`, `body`, `heading`, `title`, `button`. A text without an
  explicit role is classed by its size (>= 24 px title, >= 19 px heading, else
  body).
* **Contexts.** Any text can opt into a named context with the `FontContext`
  component; a language maps context names to fonts in `fonts.contexts`. A context
  beats the role.
* **Right-to-left.** Text shaping and bidirectional ordering are done by Bevy's
  text engine (Parley); with an Arabic-script font, Persian letters join and
  mixed Persian/English/number text is ordered correctly. For `direction: Rtl` the
  game right-aligns text that would otherwise be left-aligned. **Panel layouts are
  not mirrored** (a panel at the right stays at the right).

## For developers

* Add the English string to `assets/i18n/en-US/core.ftl` (with a `#` note), run
  `omsi pot --core`.
* Text with a key: `(Text::new(""), LocalizedText::new("menu-play"))`; with
  variables: `LocalizedText::new("menu-language").with("name", native_name)`. The
  `Text` is filled on spawn and refreshed when the language changes. In code:
  `i18n.tr("key")`, `i18n.tr_args("key", &[("n", 3.into())])`.
* Font role/context: `FontRole::Button`, `FontContext("hud-numbers".into())`.
* **Text built in code** (`format!("Placed: {n} / {max}")`) is translated without
  touching the code that builds it: the English messages of `core.ftl` are templates
  (`hud-placed-max = Placed: { $placed } / { $max }`). Every `Text` without a
  `LocalizedText` is checked when it appears, when any system rewrites it, and when
  the language changes. Rules: one line per template; plain `{ $name }` placeables
  only; a variable captures up to the next literal text (two variables in a row need
  the first to be a number); the most specific template wins; values are translated
  too (`Wood Log` -> `block-wood_log`; a raw id such as `block.wood_log` becomes the block's name in every language, English included; nested templates);
  `Label: value` lines translate a known label. **The English text of such a message
  is a contract with the code that builds the string**: `tests/i18n_pack.rs` builds the
  game's real strings and fails if one stops being recognised. New code should use keys
  (`LocalizedText`) instead.
* **Names:** one message per block / tool: `block-<id>` (`block.wood_log` ->
  `block-wood_log`) and `tool-<id>`.
* Level text: the stage loader's message tables get the external level files
  through `stage::localization::install_level_overlay`.
* Code: `crates/oopsmath-engine/src/i18n/` (`meta`, `pack`, `catalog`, `translator`,
  `service`, `fonts`, `plugin`). Tests: unit tests in those files and
  `tests/i18n_pack.rs` (reads a real pack built by the tool and checks the shipped
  data).

## Limitations

* Text built in code is translated through templates (see above). Strings that have no
  template yet (author-facing configuration errors, new messages) stay English until a
  message is added to `core.ftl`.
* The language is changed on the main menu. Texts follow it live (including everything
  open at that moment); stage content that the game resolves when a stage loads (stage
  title, objective titles, blueprint titles, challenge prompts) follows on the next
  stage start.
* Numbers are formatted with Western digits in every language.
* No font is bundled: Persian needs `Vazirmatn-*.ttf` in `assets/fonts/`, otherwise
  Persian letters are missing. Other scripts need their own font.
* Panel layout is not mirrored for right-to-left languages.
* The Persian translation is a first draft and should be reviewed by a native
  speaker.
* Pluralization is available in Fluent messages; only messages written with
  selects use it.
