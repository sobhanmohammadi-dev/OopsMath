# Fonts

Drop `.ttf` / `.otf` files here. The game finds them by id: `font.` + the file
name without extension, lower case, letters and digits only.

| File | Font id |
|---|---|
| `Vazirmatn-Regular.ttf` | `font.vazirmatnregular` |
| `Vazirmatn-Bold.ttf` | `font.vazirmatnbold` |
| `NotoSans-Regular.otf` | `font.notosansregular` |

A language names the ids it wants in `assets/i18n/<lang>/_meta.ron`
(see `docs/i18n/README.md`). A missing file is not an error: the game logs one
warning and uses its built-in font.

No font is bundled with the repository. Only add fonts whose licence allows
redistribution (for example the SIL Open Font License) and keep their licence
file next to them. Persian needs a font with Arabic-script glyphs; Vazirmatn is
a suitable one (https://github.com/rastikerdar/vazirmatn).
