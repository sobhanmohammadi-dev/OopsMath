"""`_meta.ron`: language metadata, validation and the new-language template."""
from __future__ import annotations

import re
from pathlib import Path

from . import ron

FORMAT = 1
ROLES = ("default", "body", "heading", "title", "button")
LANG_CODE = re.compile(r"^[a-z]{2,3}(-[A-Z][a-z]{3})?(-([A-Z]{2}|[0-9]{3}))?$")
FONT_ID = re.compile(r"^font\.[a-z0-9]+$")

TEMPLATE = '''#![enable(implicit_some)]
// Language metadata for OopsMath. See docs/i18n/README.md.
(
    format: 1,
    code: "{code}",
    name: "{name}",
    native_name: "{native}",
    direction: {direction},   // Ltr | Rtl
    fallback: "en-US",         // used for messages this language does not translate
    version: "1.0.0",
    authors: [],
    fonts: (
        // Font ids are "font." + the file name without extension, lower case,
        // letters and digits only: Vazirmatn-Bold.ttf -> font.vazirmatnbold.
        // The files are looked up in assets/fonts/ and assets/i18n/{code}/fonts/.
        // Leave a role out to use the built-in font.
{font_lines}
        contexts: {{
            // "hud-numbers": "font.vazirmatn",
        }},
    ),
)
'''


def template(code: str, name: str, native: str, direction: str, font: str | None) -> str:
    if font:
        lines = [f'        default: "{font}",', f'        title: "{font}",', f'        heading: "{font}",']
    else:
        lines = ['        // default: "font.example",']
    return TEMPLATE.format(code=code, name=name, native=native, direction=direction, font_lines="\n".join(lines))


def load(path: Path) -> dict:
    try:
        data = ron.loads(path.read_text(encoding="utf-8"))
    except (OSError, ron.RonError) as err:
        raise ValueError(f"{path}: {err}") from err
    if not isinstance(data, dict):
        raise ValueError(f"{path}: expected a struct")
    return data


def validate(data: dict, folder_code: str | None = None) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    warnings: list[str] = []
    if data.get("format") != FORMAT:
        errors.append(f"format must be {FORMAT}")
    code = data.get("code")
    if not isinstance(code, str) or not LANG_CODE.match(code):
        errors.append(f"code '{code}' is not a language tag like 'en-US' or 'fa'")
    elif folder_code is not None and code != folder_code:
        errors.append(f"code '{code}' does not match the folder name '{folder_code}'")
    for key in ("name", "native_name"):
        if not isinstance(data.get(key), str) or not data.get(key):
            errors.append(f"{key} is required")
    if data.get("direction", "Ltr") not in ("Ltr", "Rtl"):
        errors.append("direction must be Ltr or Rtl")
    fonts = data.get("fonts") or {}
    if not isinstance(fonts, dict):
        errors.append("fonts must be a struct")
        fonts = {}
    for key, value in fonts.items():
        if key == "contexts":
            if not isinstance(value, dict):
                errors.append("fonts.contexts must be a map")
                continue
            for ctx, font in value.items():
                if not isinstance(font, str) or not FONT_ID.match(font):
                    errors.append(f"fonts.contexts['{ctx}']: '{font}' is not a font id (font.<name>)")
        elif key in ROLES:
            if value is not None and (not isinstance(value, str) or not FONT_ID.match(value)):
                errors.append(f"fonts.{key}: '{value}' is not a font id (font.<name>)")
        else:
            warnings.append(f"fonts.{key}: unknown role (known: {', '.join(ROLES)})")
    return errors, warnings


def font_ids(data: dict) -> list[str]:
    fonts = data.get("fonts") or {}
    ids = [v for k, v in fonts.items() if k in ROLES and isinstance(v, str)]
    ids += [v for v in (fonts.get("contexts") or {}).values() if isinstance(v, str)]
    return sorted(set(ids))


def normalize_font_stem(stem: str) -> str:
    return "font." + re.sub(r"[^a-z0-9]", "", stem.lower())
