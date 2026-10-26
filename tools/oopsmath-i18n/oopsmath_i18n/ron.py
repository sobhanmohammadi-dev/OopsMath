"""A small RON reader, enough for `_meta.ron` (structs, lists, maps, strings,
numbers, booleans, unit variants, `Some(..)`, comments, trailing commas)."""
from __future__ import annotations

import re


class RonError(ValueError):
    pass


_NUM = re.compile(r"-?\d+(\.\d+)?([eE][+-]?\d+)?")
_IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


class _Parser:
    def __init__(self, text: str):
        self.s = text
        self.i = 0
        self.implicit_some = False

    def error(self, msg: str):
        line = self.s.count("\n", 0, self.i) + 1
        raise RonError(f"line {line}: {msg}")

    def ws(self):
        while self.i < len(self.s):
            c = self.s[self.i]
            if c.isspace():
                self.i += 1
            elif self.s.startswith("//", self.i):
                j = self.s.find("\n", self.i)
                self.i = len(self.s) if j < 0 else j + 1
            elif self.s.startswith("/*", self.i):
                j = self.s.find("*/", self.i)
                if j < 0:
                    self.error("unterminated comment")
                self.i = j + 2
            else:
                break

    def peek(self):
        self.ws()
        return self.s[self.i] if self.i < len(self.s) else ""

    def expect(self, ch: str):
        if self.peek() != ch:
            self.error(f"expected '{ch}'")
        self.i += 1

    def string(self) -> str:
        self.expect('"')
        out = []
        while True:
            if self.i >= len(self.s):
                self.error("unterminated string")
            c = self.s[self.i]
            self.i += 1
            if c == '"':
                return "".join(out)
            if c == "\\":
                e = self.s[self.i]
                self.i += 1
                if e == "u":
                    m = re.match(r"\{([0-9a-fA-F]+)\}", self.s[self.i:])
                    if not m:
                        self.error("bad \\u escape")
                    out.append(chr(int(m.group(1), 16)))
                    self.i += m.end()
                else:
                    out.append({"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\", "'": "'"}.get(e, e))
            else:
                out.append(c)

    def value(self):
        c = self.peek()
        if c == '"':
            return self.string()
        if c == "[":
            self.i += 1
            items = []
            while self.peek() != "]":
                items.append(self.value())
                if self.peek() == ",":
                    self.i += 1
            self.i += 1
            return items
        if c == "{":
            self.i += 1
            items = {}
            while self.peek() != "}":
                k = self.value()
                self.expect(":")
                items[k] = self.value()
                if self.peek() == ",":
                    self.i += 1
            self.i += 1
            return items
        if c == "(":
            return self.struct()
        m = _NUM.match(self.s, self.i)
        if m and (c.isdigit() or c == "-"):
            self.i = m.end()
            t = m.group(0)
            return float(t) if ("." in t or "e" in t.lower()) else int(t)
        m = _IDENT.match(self.s, self.i)
        if not m:
            self.error(f"unexpected '{c}'")
        self.i = m.end()
        name = m.group(0)
        if name == "true":
            return True
        if name == "false":
            return False
        if name == "None":
            return None
        if name == "Some" and self.peek() == "(":
            self.i += 1
            v = self.value()
            self.expect(")")
            return v
        if self.peek() == "(":  # tuple-variant / named struct: keep the payload
            return self.struct()
        return name  # unit variant, e.g. Rtl

    def struct(self):
        self.expect("(")
        fields = {}
        while self.peek() != ")":
            m = _IDENT.match(self.s, self.i)
            if not m:
                self.error("expected a field name")
            name = m.group(0)
            self.i = m.end()
            self.expect(":")
            fields[name] = self.value()
            if self.peek() == ",":
                self.i += 1
        self.i += 1
        return fields


def loads(text: str):
    text = text.lstrip("\ufeff")
    p = _Parser(text)
    while True:  # leading `#![enable(..)]` attributes
        p.ws()
        if p.s.startswith("#!", p.i):
            j = p.s.find("]", p.i)
            if j < 0:
                p.error("unterminated attribute")
            if "implicit_some" in p.s[p.i:j]:
                p.implicit_some = True
            p.i = j + 1
        else:
            break
    v = p.value()
    p.ws()
    if p.i != len(p.s):
        p.error("trailing characters")
    return v
