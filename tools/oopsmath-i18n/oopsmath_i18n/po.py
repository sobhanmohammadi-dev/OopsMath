"""gettext .pot/.po handling for Fluent messages.

One PO entry per translatable unit: `msgctxt` is the message id (or
`id.attribute`), `msgid` the English pattern, `msgstr` the translation. The
extracted comment (`#.`) is the Fluent comment above the message; the reference
(`#:`) points at the source line.
"""
from __future__ import annotations

from pathlib import Path

import polib

from . import ftl

HEADER_SCOPE = "X-OopsMath-Scope"


def _new(language: str | None, scope_name: str, source: str) -> polib.POFile:
    po = polib.POFile(wrapwidth=0)
    po.metadata = {
        "Project-Id-Version": "OopsMath",
        "Report-Msgid-Bugs-To": "",
        "Language": language or "",
        "MIME-Version": "1.0",
        "Content-Type": "text/plain; charset=UTF-8",
        "Content-Transfer-Encoding": "8bit",
        HEADER_SCOPE: scope_name,
        "X-OopsMath-Source": source,
    }
    return po


def units(messages: list[ftl.Message], source: str):
    for m in messages:
        for ctxt, text in m.units():
            yield polib.POEntry(
                msgctxt=ctxt,
                msgid=text,
                msgstr="",
                comment=m.comment if ctxt == m.id else "",
                occurrences=[(source, str(m.line))],
            )


def make_pot(messages: list[ftl.Message], scope_name: str, source: str) -> polib.POFile:
    pot = _new(None, scope_name, source)
    pot.metadata["Language"] = ""
    for entry in units(messages, source):
        pot.append(entry)
    return pot


def make_po(pot: polib.POFile, language: str) -> polib.POFile:
    po = _new(language, pot.metadata.get(HEADER_SCOPE, ""), pot.metadata.get("X-OopsMath-Source", ""))
    for e in pot:
        po.append(polib.POEntry(msgctxt=e.msgctxt, msgid=e.msgid, msgstr="", comment=e.comment,
                                occurrences=e.occurrences))
    return po


def merge(po: polib.POFile, pot: polib.POFile) -> tuple[polib.POFile, dict]:
    """Updates `po` to the units of `pot`, keeping translations.

    A translation whose English text changed is kept but marked fuzzy.
    Returns (new po, {"new": n, "changed": n, "removed": n})."""
    old = {e.msgctxt: e for e in po}
    out = _new(po.metadata.get("Language", ""), pot.metadata.get(HEADER_SCOPE, ""),
               pot.metadata.get("X-OopsMath-Source", ""))
    stats = {"new": 0, "changed": 0, "removed": 0}
    seen = set()
    for e in pot:
        seen.add(e.msgctxt)
        prev = old.get(e.msgctxt)
        entry = polib.POEntry(msgctxt=e.msgctxt, msgid=e.msgid, msgstr="", comment=e.comment,
                              occurrences=e.occurrences)
        if prev is None:
            stats["new"] += 1
        else:
            entry.msgstr = prev.msgstr
            entry.flags = [f for f in prev.flags if f != "fuzzy"]
            if prev.msgid != e.msgid and prev.msgstr:
                entry.flags.append("fuzzy")
                entry.previous_msgid = prev.msgid
                stats["changed"] += 1
            elif "fuzzy" in prev.flags:
                entry.flags.append("fuzzy")
        out.append(entry)
    stats["removed"] = len([c for c in old if c not in seen])
    return out, stats


def save(po: polib.POFile, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    po.wrapwidth = 0
    po.save(str(path))


def load(path: Path) -> polib.POFile:
    return polib.pofile(str(path), wrapwidth=0)
