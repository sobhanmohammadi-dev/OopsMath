"""Built-in self tests (also exercised by tests/test_selftest.py)."""

from __future__ import annotations

import hashlib
import struct
import tempfile
from pathlib import Path
from typing import Any, Callable, Mapping, Optional, Sequence

import jsonschema
import yaml

from .compiler import compile_stage
from .constants import (
    COMPRESSION_NONE, COMPRESSION_ZSTD, DAT_MAGIC, EXIT_OK, GLB_JSON_CHUNK, GLB_MAGIC,
    HEADER_SIZE, SECTION_ENTRY_SIZE, HeaderFlag,
)
from .dat import Section, build_dat, parse_dat
from .errors import Diagnostics
from .ftl import parse_ftl
from .registry import CompileOptions, Registry
from .scaffold import create_stage
from .schema import STAGE_SCHEMA
from .util import pack_msgpack, unpack_msgpack, zstd_compress, zstd_decompress
from .validation import validate_stage
from .world import decode_world

_SELF_TEST_YAML = """\
schema_version: 1
stage:
  id: test_stage
  title: stage.test.title
  description: stage.test.description
  difficulty: tutorial
  level: 1
learning:
  question: stage.test.question
  curriculum:
    grade: {min: 7, max: 8}
    topics: [area]
world:
  bounds: {size: [16, 8, 16]}
construction:
  allowed_blocks:
    - {id: brick, min_count: 0, max_count: 10}
  targets:
    - id: wall_a
      type: wall
      region: {min: [0, 0, 0], max: [3, 2, 0]}
objectives:
  tasks:
    - id: t_math
      type: math
      math: {type: area, answer: {mode: exact, value: 12}}
    - {id: t_build, type: construction, depends_on: [t_math], target: wall_a}
  completion:
    all: [{task_completed: t_math}, {target_completed: wall_a}]
rewards:
  money: 100
  xp: 10
"""

_SELF_TEST_FTL = ("# test strings\nstage.test.title = Test stage\n"
                  "stage.test.description = A test.\nstage.test.question = How many bricks?\n")


def _expect(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def _make_glb(payload: bytes = b'{"asset":{"version":"2.0"}}') -> bytes:
    padded = payload + b" " * (-len(payload) % 4)
    body = struct.pack("<II", len(padded), GLB_JSON_CHUNK) + padded
    return GLB_MAGIC + struct.pack("<II", 2, 12 + len(body)) + body


def _make_vox(voxels: Sequence[tuple[int, int, int, int]],
              size: tuple[int, int, int] = (4, 4, 4)) -> bytes:
    def chunk(cid: bytes, content: bytes) -> bytes:
        return cid + struct.pack("<II", len(content), 0) + content
    xyzi = struct.pack("<I", len(voxels)) + b"".join(struct.pack("<BBBB", *v) for v in voxels)
    children = chunk(b"SIZE", struct.pack("<iii", *size)) + chunk(b"XYZI", xyzi)
    return b"VOX " + struct.pack("<I", 150) + b"MAIN" + struct.pack("<II", 0, len(children)) + children


def _write_stage(root: Path, doc: dict[str, Any], files: Optional[Mapping[str, Any]] = None) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    (root / "stage.yaml").write_text(yaml.safe_dump(doc, sort_keys=False), encoding="utf-8")
    for rel, content in (files or {}).items():
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content if isinstance(content, bytes) else content.encode("utf-8"))
    return root


def _compile_dir(stage_dir: Path, opts: Optional[CompileOptions] = None) -> bytes:
    result = validate_stage(stage_dir, opts or CompileOptions())
    if not result.diag.ok or result.source is None:
        raise AssertionError("unexpected validation errors: " + "; ".join(result.diag.errors))
    return compile_stage(result.source)


def _st_schema() -> None:
    jsonschema.Draft202012Validator.check_schema(STAGE_SCHEMA)


def _st_header_roundtrip() -> None:
    blob = build_dat([Section("META", pack_msgpack({"a": 1})), Section("STAG", pack_msgpack({"b": 2}))])
    parsed = parse_dat(blob)
    h = parsed.header
    _expect(blob[:8] == DAT_MAGIC and h.header_size == 64 and h.section_entry_size == 48, "header fields")
    _expect(h.file_size == len(blob) and len(blob) % 16 == 0, "file size / alignment")
    _expect(parsed.header_crc_ok and parsed.is_valid, "header CRC")


def _st_binary_layout() -> None:
    """Pin the exact binary layout so refactors cannot change the format."""
    blob = build_dat([Section("META", b"m"), Section("STAG", b"s")])
    _expect(len(blob) == 64 + 2 * 48 + 32 and HEADER_SIZE == 64 and SECTION_ENTRY_SIZE == 48,
            f"unexpected layout size {len(blob)}")
    magic, fmt, schema, hsize, _r, flags, fsize, toff, count, esize, minrt = struct.unpack_from(
        "<8sHHHHIQQIII", blob, 0)
    _expect((magic, fmt, schema, hsize, flags, fsize, toff, count, esize, minrt) ==
            (b"OOPSMDAT", 1, 1, 64, 0, len(blob), 64, 2, 48, 0x10000), "header values")
    _expect(blob[64:68] == b"META" and blob[112:116] == b"STAG", "section table order")
    _expect(struct.unpack_from("<I", blob, 68)[0] == (1 << 9), "META flags")
    _expect(struct.unpack_from("<I", blob, 116)[0] == (1 << 8), "STAG flags")
    _expect(struct.unpack_from("<Q", blob, 72)[0] == 160, "META offset")


def _st_section_directory() -> None:
    sections = [Section("META", b"m" * 40), Section("STAG", b"s" * 50), Section("ASIX", b"x"),
                Section("ASDT", bytes(range(200)))]
    parsed = parse_dat(build_dat(sections))
    _expect([s.type for s in parsed.sections] == ["META", "STAG", "ASIX", "ASDT"], "section order")
    for original, got in zip(sections, parsed.sections):
        _expect(got.raw == original.payload and got.offset % 16 == 0, f"{original.type} round trip")


def _st_crc() -> None:
    blob = bytearray(build_dat([Section("META", b"meta"), Section("STAG", b"stag-data-1234567890")]))
    parsed = parse_dat(bytes(blob))
    target = parsed.sections[1]
    blob[target.offset] ^= 0xFF
    _expect(parse_dat(bytes(blob)).sections[1].status == "CRC32 MISMATCH", "section corruption undetected")
    blob2 = bytearray(build_dat([Section("META", b"meta"), Section("STAG", b"stag")]))
    blob2[16] ^= 0x01
    _expect(not parse_dat(bytes(blob2)).header_crc_ok, "header corruption undetected")


def _st_zstd() -> None:
    data = b"OopsMath " * 500
    packed = zstd_compress(data)
    _expect(len(packed) < len(data) and zstd_decompress(packed, len(data)) == data, "zstd round trip")
    blob = build_dat([Section("META", data), Section("STAG", b"tiny")])
    parsed = parse_dat(blob)
    _expect(parsed.sections[0].compression == COMPRESSION_ZSTD, "compressible section not compressed")
    _expect(parsed.sections[1].compression == COMPRESSION_NONE, "incompressible section was compressed")
    _expect(bool(parsed.header.flags & HeaderFlag.HAS_COMPRESSED_SECTIONS), "compressed flag")


def _st_msgpack() -> None:
    a = pack_msgpack({"b": [1, 2.5, None], "a": {"y": b"\x00\x01", "x": True}})
    b = pack_msgpack({"a": {"x": True, "y": b"\x00\x01"}, "b": [1, 2.5, None]})
    _expect(a == b, "msgpack is not canonical")
    _expect(unpack_msgpack(a)["a"]["y"] == b"\x00\x01", "msgpack round trip")


def _st_minimal_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        stage = _write_stage(Path(tmp) / "s", yaml.safe_load(_SELF_TEST_YAML))
        result = validate_stage(stage, CompileOptions())
        _expect(result.diag.ok, f"minimal stage rejected: {result.diag.errors}")
        parsed = parse_dat(_compile_dir(stage))
        _expect([s.type for s in parsed.sections] == ["META", "STAG"], "optional sections must be omitted")
        _expect(unpack_msgpack(parsed.get("META").raw)["stage_id"] == "test_stage", "META content")
        _expect(not validate_stage(stage, CompileOptions(strict=True)).diag.ok, "strict mode must fail on warnings")


def _st_deterministic() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        files = {"localization/en.ftl": _SELF_TEST_FTL, "localization/fa.ftl": _SELF_TEST_FTL}
        first = _compile_dir(_write_stage(Path(tmp) / "one", doc, files))
        second = _compile_dir(_write_stage(Path(tmp) / "deep" / "two", doc, files))
        _expect(first == second, "output differs between identical sources")
        _expect(first == _compile_dir(Path(tmp) / "one"), "output differs between runs")


def _st_ftl_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        files = {"localization/en.ftl": _SELF_TEST_FTL, "localization/fa.ftl": _SELF_TEST_FTL}
        stage = _write_stage(Path(tmp) / "s", yaml.safe_load(_SELF_TEST_YAML), files)
        result = validate_stage(stage, CompileOptions(strict=True))
        _expect(result.diag.ok, f"FTL stage rejected: {result.diag.errors}")
        parsed = parse_dat(_compile_dir(stage))
        locl = unpack_msgpack(parsed.get("LOCL").raw)
        _expect(sorted(locl) == ["en", "fa"] and locl["en"] == _SELF_TEST_FTL.encode(), "LOCL content")
        bad = _write_stage(Path(tmp) / "bad", yaml.safe_load(_SELF_TEST_YAML),
                           {"localization/en.ftl": "stage.test.title = Only title\n"})
        _expect(any("stage.test.question" in e for e in validate_stage(bad, CompileOptions()).diag.errors),
                "missing localization key not reported")
        diag = Diagnostics()
        parse_ftl("a = 1\na = 2\ngarbage line\nb = { oops\n", "x.ftl", diag)
        _expect(len(diag.errors) == 3, f"expected 3 FTL errors, got {diag.errors}")


def _st_markdown_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["learning"]["lesson"] = {"markdown": "lesson.md"}
        doc["learning"]["hints"] = [{"id": "h1", "text": "stage.test.title", "markdown": "./lesson.md"}]
        stage = _write_stage(Path(tmp) / "s", doc, {"lesson.md": "# Area\n\u0645\u0633\u0627\u062d\u062a\n"})
        parsed = parse_dat(_compile_dir(stage))
        docs = unpack_msgpack(parsed.get("DOCS").raw)["documents"]
        _expect(len(docs) == 1 and docs[0]["id"] == "lesson.md", "markdown must be deduplicated by path")
        _expect(docs[0]["text"] == "# Area\n\u0645\u0633\u0627\u062d\u062a\n", "markdown must be preserved exactly")
        stag = unpack_msgpack(parsed.get("STAG").raw)
        _expect(stag["learning"]["hints"][0]["markdown"] == "lesson.md", "STAG must reference the document ID")


def _st_glb_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["assets"] = {"custom": [{"id": "beam", "file": "assets/special_beam.glb", "type": "prop",
                                     "behavior": {"mass_kg": 12.5}}]}
        glb = _make_glb()
        stage = _write_stage(Path(tmp) / "s", doc, {"assets/special_beam.glb": glb})
        parsed = parse_dat(_compile_dir(stage))
        entry = unpack_msgpack(parsed.get("ASIX").raw)["assets"][0]
        asdt = parsed.get("ASDT").raw
        _expect(asdt[entry["offset"]:entry["offset"] + entry["size"]] == glb, "ASDT must hold exact GLB bytes")
        _expect(entry["sha256"] == hashlib.sha256(glb).hexdigest(), "ASIX sha256")
        _expect(parsed.get("ASDT").compression == COMPRESSION_NONE, "ASDT must stay raw")
        bad = _write_stage(Path(tmp) / "bad", doc, {"assets/special_beam.glb": b"not a glb at all......"})
        _expect(not validate_stage(bad, CompileOptions()).diag.ok, "invalid GLB accepted")


def _st_vox_stage() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["world"]["source"] = {"voxel_file": "world.vox", "palette": {"1": "brick", "2": "stone"}}
        vox = _make_vox([(0, 0, 0, 1), (1, 0, 0, 2), (1, 1, 1, 2)])
        stage = _write_stage(Path(tmp) / "s", doc, {"world.vox": vox})
        parsed = parse_dat(_compile_dir(stage))
        size, palette, voxels = decode_world(parsed.get("WRLD").raw)
        _expect(size == (16, 8, 16) and palette == ["air", "brick", "stone"], "WRLD header/palette")
        _expect(voxels == {(0, 0, 0): "brick", (1, 0, 0): "stone", (1, 1, 1): "stone"}, f"voxels {voxels}")
        unmapped = dict(doc, world=dict(doc["world"], source={"voxel_file": "world.vox", "palette": {"1": "brick"}}))
        bad = _write_stage(Path(tmp) / "bad", unmapped, {"world.vox": vox})
        _expect(any("no entry in world.source.palette" in e for e in validate_stage(bad, CompileOptions()).diag.errors),
                "unmapped palette index must be an error")


def _st_invalid_reference() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["objectives"]["tasks"][1]["depends_on"] = ["nope"]
        doc["objectives"]["completion"] = {"task_completed": "ghost"}
        errors = validate_stage(_write_stage(Path(tmp) / "s", doc), CompileOptions()).diag.errors
        _expect(any("unknown task 'nope'" in e for e in errors), f"missing dependency error: {errors}")
        _expect(any("unknown task 'ghost'" in e for e in errors), f"missing condition error: {errors}")
        doc2 = yaml.safe_load(_SELF_TEST_YAML)
        doc2["construction"]["allowed_blocks"][0]["id"] = "foo"
        opts = CompileOptions(manifest=Registry(blocks=frozenset({"brick"})))
        errors2 = validate_stage(_write_stage(Path(tmp) / "t", doc2), opts).diag.errors
        _expect(any("unknown block 'foo'" in e for e in errors2), f"manifest check failed: {errors2}")
        result = validate_stage(_write_stage(Path(tmp) / "u", doc2), opts)
        _expect(result.source is None, "invalid stage must not produce a source")


def _st_unsafe_paths() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        for bad_path in ("../outside.md", "/etc/passwd", "C:/x.md", "a\\b.md"):
            doc = yaml.safe_load(_SELF_TEST_YAML)
            doc["learning"]["lesson"] = {"markdown": bad_path}
            errors = validate_stage(_write_stage(Path(tmp) / "s", doc), CompileOptions()).diag.errors
            _expect(any("unsafe path" in e for e in errors), f"'{bad_path}' accepted: {errors}")
        outside = Path(tmp) / "outside.md"
        outside.write_text("secret", encoding="utf-8")
        stage = Path(tmp) / "link"
        doc = yaml.safe_load(_SELF_TEST_YAML)
        doc["learning"]["lesson"] = {"markdown": "link.md"}
        _write_stage(stage, doc)
        try:
            (stage / "link.md").symlink_to(outside)
        except (OSError, NotImplementedError):
            return  # symlinks unavailable on this platform
        errors = validate_stage(stage, CompileOptions()).diag.errors
        _expect(any("outside the stage directory" in e for e in errors), "symlink escape accepted")


def _st_scaffold() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        stage = Path(tmp) / "new_stage"
        create_stage(stage, "002_demo")
        result = validate_stage(stage, CompileOptions(strict=True))
        _expect(result.diag.ok, f"scaffolded stage invalid: {result.diag.errors + result.diag.warnings}")
        _expect(parse_dat(_compile_dir(stage)).is_valid, "scaffolded stage must compile")


TESTS: list[tuple[str, Callable[[], None]]] = [
    ("embedded schema is a valid Draft 2020-12 schema", _st_schema),
    ("header round trip", _st_header_roundtrip),
    ("binary layout is pinned", _st_binary_layout),
    ("section directory round trip", _st_section_directory),
    ("CRC validation", _st_crc),
    ("Zstandard round trip and policy", _st_zstd),
    ("MessagePack round trip and canonical order", _st_msgpack),
    ("minimal YAML-only stage", _st_minimal_stage),
    ("deterministic DAT generation", _st_deterministic),
    ("stage with FTL", _st_ftl_stage),
    ("stage with Markdown", _st_markdown_stage),
    ("stage with custom GLB", _st_glb_stage),
    ("stage with VOX world", _st_vox_stage),
    ("invalid reference rejection", _st_invalid_reference),
    ("unsafe path rejection", _st_unsafe_paths),
    ("scaffolded stage is valid and compiles", _st_scaffold),
]


def run_self_test(style=None, out=None) -> int:
    import sys
    out = out or sys.stdout
    failures = 0
    for name, test in TESTS:
        try:
            test()
        except Exception as exc:  # report every failure, never hide it
            failures += 1
            mark = style.paint("FAIL", "red", "bold") if style else "FAIL"
            print(f"{mark}  {name}: {type(exc).__name__}: {exc}", file=out)
        else:
            mark = style.paint("PASS", "green") if style else "PASS"
            print(f"{mark}  {name}", file=out)
    print(f"{len(TESTS) - failures}/{len(TESTS)} tests passed", file=out)
    return 1 if failures else EXIT_OK
