"""Reading and writing the Fluent (.ftl) files OopsMath uses.

Two flavours share this parser:

* **core** files (`assets/i18n/<lang>/core.ftl`) use real Fluent: identifiers
  such as ``menu-resume``, ``{ $variables }``, select expressions, attributes
  (``.label``) and terms (``-brand``). The game evaluates them with Fluent.
* **level** files (`assets/i18n/<lang>/levels/<id>.ftl`) use the *simple* stage
  format the stage compiler writes: ``stage.<id>.title = text`` with dotted keys
  and single-line values. The game's stage loader reads them verbatim.

The parser keeps pattern text as written (it does not interpret placeables); it
only splits a file into messages, attributes and comments, which is all that
translation needs.
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field

CORE_ID = re.compile(r"^(-?[A-Za-z][A-Za-z0-9_-]*)[ \t]*=[ \t]?(.*)$")
LEVEL_ID = re.compile(r"^([A-Za-z0-9][A-Za-z0-9_.-]*)[ \t]*=[ \t]?(.*)$")
ATTR = re.compile(r"^\.([A-Za-z][A-Za-z0-9_-]*)[ \t]*=[ \t]?(.*)$")
VARIABLE = re.compile(r"\$([A-Za-z][A-Za-z0-9_-]*)")
MESSAGE_REF = re.compile(r"\{\s*(-?[A-Za-z][A-Za-z0-9_-]*)(?:\.[A-Za-z][A-Za-z0-9_-]*)?\s*\}")


class FtlError(ValueError):
    pass


@dataclass
class Message:
    id: str
    value: str | None = None
    attributes: list[tuple[str, str]] = field(default_factory=list)
    comment: str = ""
    line: int = 0

    def units(self):
        """Yield (context, text) translation units: the value, then each attribute."""
        if self.value is not None:
            yield self.id, self.value
        for name, text in self.attributes:
            yield f"{self.id}.{name}", text


def _dedent(lines: list[str]) -> list[str]:
    indents = [len(l) - len(l.lstrip(" \t")) for l in lines if l.strip()]
    cut = min(indents) if indents else 0
    return [l[cut:] if l.strip() else "" for l in lines]


def _join(first: str, cont: list[str]) -> str:
    cont = _dedent(cont)
    while cont and not cont[-1]:
        cont.pop()
    parts = ([first] if first else []) + cont
    return "\n".join(parts)


def parse(text: str, scope: str = "core") -> list[Message]:
    """Parses `text`. `scope` is "core" (Fluent) or "level" (simple format)."""
    if scope not in ("core", "level"):
        raise ValueError(scope)
    id_re = CORE_ID if scope == "core" else LEVEL_ID
    lines = text.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n").split("\n")
    messages: list[Message] = []
    pending_comment: list[str] = []
    cur: Message | None = None
    target: list | None = None  # [first_line, cont_lines, attr_name|None]
    seen: set[str] = set()

    def finish():
        nonlocal cur, target
        if cur is None:
            return
        _close_target()
        if cur.value is None and not cur.attributes:
            raise FtlError(f"line {cur.line}: message '{cur.id}' has no value")
        messages.append(cur)
        cur = None
        target = None

    def _close_target():
        nonlocal target
        if cur is None or target is None:
            return
        first, cont, attr = target
        pattern = _join(first, cont)
        if attr is None:
            cur.value = pattern if (pattern or cont or first) else None
        else:
            cur.attributes.append((attr, pattern))
        target = None

    def depth(t) -> int:
        first, cont, _ = t
        s = "\n".join([first] + cont)
        return s.count("{") - s.count("}")

    for number, raw in enumerate(lines, 1):
        stripped = raw.strip()
        indented = raw[:1] in (" ", "\t")
        if not stripped:
            # A blank line ends a comment block; it may sit inside a pattern.
            pending_comment = [] if cur is None else pending_comment
            if cur is not None and target is not None:
                target[1].append("")
            continue
        if raw.startswith("#"):
            finish()
            body = raw.lstrip("#")
            if raw.startswith("##"):
                pending_comment = []  # group / resource comments do not attach
            else:
                pending_comment.append(body[1:] if body.startswith(" ") else body)
            continue
        if indented and cur is not None:
            m = ATTR.match(stripped)
            if m and scope == "core" and (target is None or depth(target) <= 0):
                _close_target()
                target = [m.group(2), [], m.group(1)]
                continue
            if scope == "level":
                raise FtlError(
                    f"line {number}: level files only support single-line 'key = value' entries"
                )
            if target is None:
                target = ["", [], None]
            target[1].append(raw)
            continue
        m = id_re.match(raw)
        if not m:
            raise FtlError(f"line {number}: cannot parse '{raw.strip()[:60]}'")
        finish()
        if m.group(1) in seen:
            raise FtlError(f"line {number}: duplicate message id '{m.group(1)}'")
        seen.add(m.group(1))
        cur = Message(m.group(1), comment="\n".join(pending_comment), line=number)
        pending_comment = []
        target = [m.group(2), [], None]
    finish()
    return messages


def render_message(message_id: str, value: str | None, attributes, comment: str = "") -> str:
    out: list[str] = []
    if comment:
        out.extend(f"# {c}".rstrip() for c in comment.split("\n"))
    value = value if value is not None else ""
    if value and "\n" not in value:
        out.append(f"{message_id} = {value}")
    elif value:
        out.append(f"{message_id} =")
        out.extend(("    " + l) if l else "" for l in value.split("\n"))
    else:
        out.append(f"{message_id} =")
    for name, text in attributes:
        if "\n" in text:
            out.append(f"    .{name} =")
            out.extend(("        " + l) if l else "" for l in text.split("\n"))
        else:
            out.append(f"    .{name} = {text}")
    return "\n".join(out)


def variables(text: str) -> set[str]:
    return set(VARIABLE.findall(text))


def references(text: str) -> set[str]:
    return set(MESSAGE_REF.findall(text))


def _strip_literals(text: str) -> str:
    return re.sub(r'\{\s*"(?:[^"\\]|\\.)*"\s*\}', "", text)


def validate_translation(source: str, translation: str, scope: str = "core") -> tuple[list[str], list[str]]:
    """Returns (errors, warnings) for one translated pattern."""
    errors: list[str] = []
    warnings: list[str] = []
    if not translation.strip():
        return errors, warnings
    body = _strip_literals(translation)
    balance = 0
    for ch in body:
        balance += (ch == "{") - (ch == "}")
        if balance < 0:
            break
    if balance != 0:
        errors.append("unbalanced '{' / '}'")
    if scope == "level":
        if "\n" in translation:
            errors.append("level translations must be a single line")
        if "{" in body:
            warnings.append("level text is shown verbatim; placeables are not evaluated")
        return errors, warnings
    src_vars, dst_vars = variables(source), variables(translation)
    unknown = sorted(dst_vars - src_vars)
    if unknown:
        errors.append("unknown variable(s): " + ", ".join("$" + v for v in unknown))
    missing = sorted(src_vars - dst_vars)
    if missing:
        warnings.append("variable(s) not used: " + ", ".join("$" + v for v in missing))
    if "->" in source and "*[" not in translation:
        errors.append("a select expression needs a default variant '*[other]'")
    bad_refs = sorted(references(translation) - references(source))
    if bad_refs:
        errors.append("unknown message/term reference(s): " + ", ".join(bad_refs))
    if source[:1].isspace() != translation[:1].isspace() or source[-1:].isspace() != translation[-1:].isspace():
        warnings.append("leading/trailing whitespace differs from the source")
    return errors, warnings
