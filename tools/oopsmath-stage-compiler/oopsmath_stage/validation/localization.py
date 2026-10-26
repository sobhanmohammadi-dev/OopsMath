"""Validation layer 4b: stage-local FTL files and localization key references."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from ..constants import LOCALE_RE, MAX_TEXT_FILE_BYTES, SOURCE_LOCALE
from ..errors import Diagnostics, SourceDataError, UnsafePathError
from ..ftl import parse_ftl
from ..model import LocaleFile
from ..paths import resolve_safe_path
from ..registry import CompileOptions
from ..yamlio import read_limited


def discover_locale_files(stage_dir: Path, data: dict[str, Any],
                          diag: Diagnostics) -> list[tuple[str, Path, str]]:
    """Return (locale, filesystem path, relative path) for every stage FTL file."""
    configured = data.get("localization", {}).get("ftl")
    rel_dir_or_file = configured if configured is not None else "localization"
    try:
        path, normalized = resolve_safe_path(stage_dir, rel_dir_or_file)
    except UnsafePathError as exc:
        diag.error(f"localization.ftl: unsafe path '{rel_dir_or_file}': {exc}")
        return []
    candidates: list[tuple[Path, str]] = []
    if path.is_file():
        if path.suffix.lower() != ".ftl":
            diag.error(f"localization.ftl: '{normalized}' must be an .ftl file or a directory")
            return []
        candidates = [(path, normalized)]
    elif path.is_dir():
        for child in sorted(path.glob("*.ftl"), key=lambda p: p.name):
            candidates.append((child, f"{normalized}/{child.name}"))
        if configured is not None and not candidates:
            diag.error(f"localization.ftl: directory '{normalized}' contains no .ftl files")
    elif configured is not None:
        diag.error(f"localization.ftl references missing file or directory '{normalized}'")
        return []
    found: list[tuple[str, Path, str]] = []
    seen: dict[str, str] = {}
    for child, rel in candidates:
        try:
            resolve_safe_path(stage_dir, rel)
        except UnsafePathError as exc:
            diag.error(f"{rel}: {exc}")
            continue
        locale = child.stem
        if not LOCALE_RE.match(locale):
            diag.error(f"{rel}: '{locale}' is not a valid locale name (expected e.g. 'fa' or 'en-US')")
            continue
        if locale in seen:
            diag.error(f"{rel}: locale '{locale}' is already provided by '{seen[locale]}'")
            continue
        seen[locale] = rel
        found.append((locale, child, rel))
    return sorted(found, key=lambda item: item[0])


def find_i18n_dir(stage_dir: Path, opts: CompileOptions, diag: Diagnostics) -> Path | None:
    """The language-pack folder to bake from, or None.

    An explicit ``--i18n`` folder must exist. Otherwise ``assets/i18n`` is looked
    for in the stage folder's parents (a stage inside the game repository finds
    the repository's packs automatically)."""
    if not opts.bake_i18n:
        return None
    if opts.i18n_dir is not None:
        if opts.i18n_dir.is_dir():
            return opts.i18n_dir
        diag.error(f"--i18n: '{opts.i18n_dir}' is not a directory")
        return None
    here = stage_dir.resolve()
    for parent in [here, *here.parents][:8]:
        candidate = parent / "assets" / "i18n"
        if candidate.is_dir():
            return candidate
    return None


def bake_language_packs(stage_dir: Path, data: dict[str, Any], opts: CompileOptions,
                        source_locales: list[LocaleFile], diag: Diagnostics) -> list[LocaleFile]:
    """Merges ``<i18n>/<lang>/levels/<stage id>.ftl`` into the stage's locales.

    The stage's own files hold the source language (en-US); translations live
    in the language packs (``tools/oopsmath-i18n``). Baking makes the compiled
    stage self-contained. A pack wins over a translation of the same key left in
    the stage source. Translations are *partial by design*: a key a language
    lacks is shown in English by the game, so it is not reported as missing."""
    root = find_i18n_dir(stage_dir, opts, diag)
    if root is None:
        return source_locales
    stage_id = data["stage"]["id"]
    english = next((loc for loc in source_locales if loc.locale == SOURCE_LOCALE), None)
    by_locale = {loc.locale: loc for loc in source_locales}
    baked: list[str] = []
    for lang_dir in sorted((p for p in root.iterdir() if p.is_dir()), key=lambda p: p.name):
        lang = lang_dir.name
        path = lang_dir / "levels" / f"{stage_id}.ftl"
        if lang == SOURCE_LOCALE or not LOCALE_RE.match(lang) or not path.is_file():
            continue
        rel = f"i18n/{lang}/levels/{stage_id}.ftl"
        try:
            raw = read_limited(path, MAX_TEXT_FILE_BYTES)
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            diag.error(f"{rel}: not valid UTF-8")
            continue
        except (OSError, SourceDataError) as exc:
            diag.error(f"{rel}: cannot be read: {exc}")
            continue
        keys = parse_ftl(text, rel, diag)
        if english is not None:
            unknown = sorted(keys - english.keys)
            if unknown:
                diag.warn(f"{rel}: {len(unknown)} key(s) are not in the English source and are "
                          f"ignored by the game's stage (first: '{unknown[0]}')")
        namespace = data.get("localization", {}).get("namespace")
        if namespace:
            outside = [k for k in sorted(keys) if not (k == namespace or k.startswith(namespace + "."))]
            if outside:
                diag.warn(f"{rel}: key '{outside[0]}' is outside the declared namespace '{namespace}'")
        existing = by_locale.get(lang)
        if existing is None:
            by_locale[lang] = LocaleFile(lang, rel, raw, keys, origin="pack")
        else:
            merged = existing.raw.rstrip(b"\n") + b"\n" + raw
            by_locale[lang] = LocaleFile(lang, f"{existing.rel_path} + {rel}", merged,
                                         existing.keys | keys, origin="stage+pack")
            diag.note(f"{lang}: the stage source also holds {lang} strings; the language pack "
                      f"wins per key. Move them with `omsi import-ftl` and delete the stage file.")
        total = len(english.keys) if english else len(keys)
        done = len(keys & english.keys) if english else len(keys)
        baked.append(f"{lang} ({done}/{total})")
    if baked:
        diag.note("baked language-pack translations: " + ", ".join(baked)
                  + f" from {root}")
    return sorted(by_locale.values(), key=lambda loc: loc.locale)


def collect_localization_refs(data: dict[str, Any]) -> list[tuple[str, str]]:
    stage, learning = data["stage"], data["learning"]
    refs = [("stage.title", stage["title"])]
    if "description" in stage:
        refs.append(("stage.description", stage["description"]))
    for i, line in enumerate(data.get("story", {}).get("dialogue", [])):
        refs.append((f"story.dialogue[{i}].text", line["text"]))
    refs.append(("learning.question", learning["question"]))
    for i, blueprint in enumerate(data.get("construction", {}).get("blueprints", [])):
        if "title" in blueprint:
            refs.append((f"construction.blueprints[{i}].title", blueprint["title"]))
    for i, task in enumerate(data.get("objectives", {}).get("tasks", [])):
        math, info = task.get("math"), task.get("data")
        if isinstance(math, dict) and isinstance(math.get("prompt"), str):
            refs.append((f"objectives.tasks[{i}].math.prompt", math["prompt"]))
        if task.get("type") == "construction" and isinstance(info, dict) \
                and isinstance(info.get("title"), str):
            refs.append((f"objectives.tasks[{i}].data.title", info["title"]))
    if "explanation" in learning.get("solution", {}):
        refs.append(("learning.solution.explanation", learning["solution"]["explanation"]))
    for i, hint in enumerate(learning.get("hints", [])):
        refs.append((f"learning.hints[{i}].text", hint["text"]))
    return refs


def load_and_validate_localization(stage_dir: Path, data: dict[str, Any], opts: CompileOptions,
                                   diag: Diagnostics) -> list[LocaleFile]:
    locales: list[LocaleFile] = []
    for locale, path, rel in discover_locale_files(stage_dir, data, diag):
        try:
            raw = read_limited(path, MAX_TEXT_FILE_BYTES)
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            diag.error(f"{rel}: not valid UTF-8")
            continue
        except (OSError, SourceDataError) as exc:
            diag.error(f"{rel}: cannot be read: {exc}")
            continue
        locales.append(LocaleFile(locale, rel, raw, parse_ftl(text, rel, diag)))

    base = opts.base_keys
    namespace = data.get("localization", {}).get("namespace")
    unverifiable = 0
    for yaml_path, key in collect_localization_refs(data):
        if base is not None and key in base:
            continue
        if not locales and base is None:
            unverifiable += 1
            continue
        missing_in = [loc.locale for loc in locales if key not in loc.keys]
        if locales and not missing_in:
            continue
        if not locales or len(missing_in) == len(locales):
            hint = "" if base is not None else \
                " (no --base-locales were given; pass them if this key is global)"
            diag.error(f"localization key '{key}' is missing from stage FTL and base localization "
                       f"(referenced at {yaml_path}){hint}")
        else:
            diag.warn(f"localization key '{key}' (referenced at {yaml_path}) is missing "
                      f"from locale(s): {', '.join(missing_in)}")
    if unverifiable:
        diag.warn(f"{unverifiable} localization key reference(s) could not be verified: "
                  "the stage has no FTL files and no --base-locales were given")
    for loc in locales:
        for key in sorted(loc.keys):
            if base is not None and key in base:
                diag.warn(f"{loc.rel_path}: stage-local key '{key}' shadows a base game key")
            if namespace and not (key == namespace or key.startswith(namespace + ".")):
                diag.warn(f"{loc.rel_path}: key '{key}' is outside the declared namespace '{namespace}'")
    return bake_language_packs(stage_dir, data, opts, locales, diag)
