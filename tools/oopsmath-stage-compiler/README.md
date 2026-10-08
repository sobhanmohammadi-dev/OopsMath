# OopsMath Stage Compiler (OMSC)

**DAT v1 · Stage Schema v1**

OopsMath Stage Compiler (**OMSC**) is the offline content compiler for **OopsMath**.

It converts a stage source directory containing `stage.yaml` and optional supporting files into a single deterministic `.dat` runtime package.

```text
Stage Source
    │
    ├── stage.yaml
    ├── localization/*.ftl
    ├── *.md
    ├── *.vox
    └── assets/*.glb
            │
            ▼
      OMSC Validation
            │
            ▼
      Normalization
            │
            ▼
       DAT Packaging
            │
            ▼
     Deterministic .dat
```

> **Important:** OMSC is a compiler and packaging tool, not the OopsMath game runtime.
>
> OMSC validates source structure, schemas, references, files, and package integrity. It does **not** simulate gameplay, construction, physics, objectives, scoring, or runtime conditions.

---

## Features

OMSC currently provides:

* Stage Schema v1 validation
* YAML parsing with structural validation
* Reference validation
* Built-in asset/block/tool/audio manifest validation
* Fluent (`.ftl`) localization validation
* Markdown documentation packaging
* MagicaVoxel (`.vox`) world import
* VOX scene-graph flattening
* Coordinate conversion from Z-up to OopsMath Y-up
* Custom voxel world encoding
* GLB custom-asset validation and packaging
* MessagePack serialization for structured sections
* Zstandard compression for compressible sections
* CRC32 integrity checks
* Deterministic `.dat` generation
* DAT inspection and section dumping
* Multi-stage builds
* Built-in self-tests

---

# Requirements

OMSC requires Python and the following third-party packages:

* [PyYAML](https://pypi.org/project/PyYAML/)
* [jsonschema](https://pypi.org/project/jsonschema/)
* [msgpack](https://pypi.org/project/msgpack/)
* [zstandard](https://pypi.org/project/zstandard/)

Install dependencies with:

```bash
pip install PyYAML jsonschema msgpack zstandard
```

---

# Repository Structure

Current repository structure:

```text
OOPSMATH-STAGE-COMPILER/
│
├── omsc.py
├── README.md
├── stage.schema.yaml
│
└── examples/
    └── 001_first_wall/
        ├── stage.yaml
        ├── lesson.md
        ├── solution.md
        │
        └── localization/
            ├── fa.ftl
            └── en-US.ftl
```

`stage.schema.yaml` contains the Stage Schema used by the compiler.

`examples/001_first_wall/` is the first complete example stage and is also useful for testing the compiler.

---

# CLI Usage

## Validate a Stage

Validation checks the stage source without producing a runtime package.

```bash
python omsc.py validate examples/001_first_wall
```

Typical output:

```text
✔ Stage parsed
✔ Schema valid
✔ References valid
✔ Assets valid
✔ Localization valid
✔ Documentation valid
```

---

## Build a Stage

Compile a stage into a `.dat` package:

```bash
python omsc.py build examples/001_first_wall
```

Specify the output path explicitly:

```bash
python omsc.py build examples/001_first_wall -o build/001.dat
```

A successful build produces a deterministic binary package.

Example:

```text
✔ Stage parsed
✔ Schema valid
✔ References valid
✔ Assets valid
✔ Localization valid
✔ Documentation valid
✔ Stage compiled
Output: build\001.dat
```

---

## Build All Stages

Compile all stage directories under a source directory:

```bash
python omsc.py build-all examples --output-dir build/stages
```

Each valid stage is compiled independently into its own `.dat` package.

---

## Inspect a DAT Package

Inspect the package header and section directory:

```bash
python omsc.py inspect build/001.dat
```

Example:

```text
File:             build\001.dat
Magic:            OOPSMDAT
Format version:   1
Schema version:   1
Header size:      64
...
```

Dump a specific section:

```bash
python omsc.py inspect build/001.dat --dump STAG
```

Other useful sections include:

```text
META
STAG
WRLD
LOCL
DOCS
ASIX
```

`ASDT` is the binary payload area for custom embedded assets and may not be human-readable.

---

## Run Self-Test

Run the built-in compiler test suite:

```bash
python omsc.py self-test
```

The self-test covers core functionality including:

* schema validation
* deterministic serialization
* DAT header generation
* section directory generation
* CRC32
* Zstandard compression
* MessagePack encoding
* localization parsing
* Markdown packaging
* GLB validation
* VOX import
* invalid references
* unsafe paths

---

# Common Options

## `--manifest FILE`

Use a built-in OopsMath registry in JSON or YAML format.

```bash
python omsc.py validate examples/001_first_wall \
    --manifest config/manifest.yaml
```

The manifest may define registries such as:

```yaml
blocks:
tools:
assets:
audio:
```

The manifest allows the compiler to verify that stage references resolve to known built-in identifiers.

Built-in resources are referenced by stable IDs and are **not embedded into the stage package**.

---

## `--base-locales DIR`

Specify the global game localization directory.

```bash
python omsc.py validate examples/001_first_wall \
    --base-locales locales
```

The base locale registry belongs to the game and is conceptually separate from stage-local localization.

---

## `--strict`

Treat compiler warnings as errors:

```bash
python omsc.py validate examples/001_first_wall --strict
```

This is useful for CI and release builds.

---

# Stage Source Format

A minimal stage can consist of only a YAML file:

```text
001_example/
└── stage.yaml
```

A larger stage can contain additional resources:

```text
001_example/
├── stage.yaml
├── lesson.md
├── solution.md
│
├── localization/
│   ├── fa.ftl
│   └── en-US.ftl
│
├── world.vox
│
└── assets/
    └── special_beam.glb
```

All additional files are optional.

A stage that uses only built-in resources can compile successfully without any FTL, Markdown, VOX, or GLB files.

---

# `stage.yaml`

Every stage starts with:

```yaml
schema_version: 1
```

The complete Stage Schema is defined in:

```text
stage.schema.yaml
```

The YAML contains stage metadata, learning content, story data, world configuration, construction constraints, economy, physics declarations, objectives, events, rewards, localization references, and other stage-level configuration.

OMSC validates the **structure and references** of these declarations.

It does not execute them.

For example:

```yaml
physics:
  enabled: true

  tests:
    - id: "wall_static_load_test"
      type: static_load
```

The compiler verifies that this declaration is structurally valid.

The actual physics simulation is the responsibility of the OopsMath runtime.

---

# Localization

Stage localization is stored separately from global game localization.

The recommended directory structure is:

```text
localization/
├── fa.ftl
└── en-US.ftl
```

In `stage.yaml`:

```yaml
localization:
  ftl: "localization"
  namespace: "stage.001"
```

The `ftl` field is a **directory path**, not a list of individual files.

OMSC discovers locale files from this directory.

For example:

```text
fa.ftl     → locale: fa
en-US.ftl  → locale: en-US
```

The stage package therefore stores the available locales separately from the global game localization.

---

## Localization Key References

Stage data can reference keys such as:

```text
stage.001.title
stage.001.description
stage.001.dialogue.intro
stage.001.question
```

A referenced key must exist in either:

1. the base locale registry supplied through `--base-locales`, or
2. the stage-local FTL files.

When neither a stage FTL nor `--base-locales` is available, OMSC cannot verify those references.

In that situation:

* validation does not fail solely because the keys cannot be verified;
* OMSC emits a warning.

With `--strict`, warnings are treated as errors.

---

# Documentation

Markdown files such as:

```text
lesson.md
solution.md
```

are stage documentation resources.

OMSC packages their contents into the `DOCS` section of the resulting `.dat`.

The runtime therefore does not need to access the original Markdown files after the stage has been compiled.

---

# DAT v1

OopsMath stages are stored in a custom binary container.

DAT v1 combines:

* a custom OopsMath header
* a custom section directory
* MessagePack for structured data
* Zstandard compression
* custom binary voxel-world data
* raw binary payloads for embedded custom assets

Conceptually:

```text
┌─────────────────────────────────────────┐
│ DAT Header                              │
│ 64 bytes                                │
├─────────────────────────────────────────┤
│ Section Directory                       │
│ 48 bytes per entry                      │
├─────────────────────────────────────────┤
│ META                                    │
├─────────────────────────────────────────┤
│ STAG                                    │
├─────────────────────────────────────────┤
│ WRLD                                    │
├─────────────────────────────────────────┤
│ LOCL                                    │
├─────────────────────────────────────────┤
│ DOCS                                    │
├─────────────────────────────────────────┤
│ ASIX                                    │
├─────────────────────────────────────────┤
│ ASDT                                    │
└─────────────────────────────────────────┘
```

Sections are optional where appropriate.

For example, a stage without custom assets does not need `ASIX` or `ASDT`.

---

# DAT Header

DAT v1 uses a fixed 64-byte little-endian header.

Important fields include:

```text
magic
format_version
schema_version
header_size
flags
file_size
section_table_offset
section_count
section_entry_size
minimum_runtime_version
header_crc32
```

The magic is:

```text
OOPSMDAT
```

The header size is:

```text
64 bytes
```

---

## Header CRC32

`header_crc32` is calculated over **all 64 header bytes**.

Before calculating the CRC32, the four-byte `header_crc32` field itself is set to zero.

Conceptually:

```text
header_with_crc_field_zeroed
            │
            ▼
          CRC32
            │
            ▼
      header_crc32
```

The resulting value is written into the header.

---

# Section Directory

Each section entry is exactly 48 bytes and contains fields for:

```text
type
flags
offset
stored_size
raw_size
crc32
reserved
```

All DAT values are little-endian.

Each section starts on a 16-byte alignment boundary.

---

# Section Types

## `META`

Small package metadata intended for quick identification.

Structured using MessagePack.

---

## `STAG`

Compiled stage definition.

This section contains the structured stage configuration derived from `stage.yaml`.

It includes data such as:

* stage metadata
* story references
* learning configuration
* player configuration
* camera configuration
* construction rules
* economy
* physics declarations
* objectives
* events
* rewards
* localization metadata
* world metadata

`STAG` does not contain the original YAML source.

---

## `WRLD`

Compiled voxel-world data.

The source world may originate from:

* explicit YAML blocks
* a MagicaVoxel `.vox` file

The runtime receives the normalized OopsMath world representation rather than the original VOX file.

---

## `LOCL`

Stage-local localization data.

Each locale stores its FTL content as UTF-8 data.

Example:

```text
en-US
fa
```

---

## `DOCS`

Embedded Markdown documentation.

Typical entries include:

```text
lesson.md
solution.md
```

---

## `ASIX`

Asset index for custom embedded assets.

It contains metadata required by the runtime to locate binary assets inside `ASDT`.

Typical metadata includes:

```text
asset ID
asset type
MIME type
offset
size
SHA-256
```

---

## `ASDT`

Binary asset data.

Custom assets such as stage-specific GLB files are stored here as binary payloads.

Built-in OopsMath assets are not copied into this section.

---

# Compression

Structured sections are eligible for Zstandard compression.

OMSC stores the compressed representation only when it is smaller than the original data.

This avoids increasing package size when compression provides no benefit.

The package records both:

```text
stored_size
raw_size
```

so the runtime can validate and decompress sections safely.

Binary asset data such as `ASDT` is kept uncompressed so the asset index can reference the raw binary payload directly and because formats such as GLB may already be efficiently packed.

---

# Integrity

Every section contains a CRC32 checksum.

The runtime can therefore detect:

* corrupted sections
* incomplete writes
* accidental file modification

CRC32 is an integrity mechanism, not a cryptographic authenticity mechanism.

Custom assets may additionally contain SHA-256 metadata in the asset index.

---

# World Coordinate Conventions

OopsMath uses:

```text
Y-up
```

MagicaVoxel uses:

```text
Z-up
```

When importing VOX data, OMSC converts coordinates using:

```text
(x, y, z) → (x, z, y)
```

---

# VOX Import

When a `.vox` source is used, OMSC transforms the source before packaging it.

The import pipeline includes:

```text
VOX
 │
 ├── parse voxel data
 ├── parse supported scene graph nodes
 ├── flatten scene graph
 ├── apply transforms
 ├── coordinate conversion
 ├── rebase
 ├── apply world offset
 └── encode OopsMath WRLD
```

---

## Scene Graph Flattening

VOX scene graph data is flattened into a single voxel set before it is stored in the stage package.

The runtime therefore does not need to reproduce the authoring-time VOX scene graph.

---

## Rebase

After flattening, the geometry is rebased so that its minimum corner becomes:

```text
(0, 0, 0)
```

---

## World Offset

After coordinate conversion, `world.source.offset` is applied.

When `world.source.offset` is omitted, OMSC uses:

```text
world.bounds.origin
```

as the default offset.

---

# VOX Palette Mapping

VOX color indices use the range:

```text
1..255
```

OMSC maps these indices to OopsMath built-in blocks using:

```yaml
world:
  source:
    palette:
```

An optional:

```yaml
default_block:
```

may be used as a fallback.

The exact block IDs are resolved against the available built-in registry when a manifest is supplied.

---

# World Coordinates

Coordinates written explicitly in `stage.yaml` are treated as **absolute world coordinates**.

The `WRLD` section stores **bounds-local coordinates**.

The conversion is:

```text
local = absolute - bounds.origin
```

This allows the world representation to remain compact while preserving the stage's world-space placement.

---

# YAML Block Priority

When both YAML-defined blocks and VOX voxels describe the same location:

```text
YAML blocks override VOX voxels
```

Explicit stage declarations therefore have priority over imported authoring data.

---

# Built-in vs Custom Assets

OopsMath distinguishes between two asset categories.

## Built-in Resources

Examples include:

```text
brick
wood
stone
concrete
hammer
tape_measure
```

These are referenced by stable IDs.

They are supplied by the game runtime and are **not duplicated inside every `.dat` file**.

---

## Custom Stage Assets

A stage may provide its own asset, for example:

```text
assets/
└── special_beam.glb
```

Custom asset data is packaged into the `.dat` file.

The asset index stores the metadata required to retrieve the binary payload.

---

# Validation Scope

OMSC validates the **stage package**, not the gameplay state.

## Compiler Responsibilities

OMSC may validate:

* YAML syntax
* schema compliance
* valid identifiers
* referenced stage IDs
* block/tool/asset/audio references
* file existence
* safe relative paths
* localization files
* localization references when verifiable
* Markdown files
* GLB structure
* VOX structure
* package layout
* section sizes
* CRC32
* deterministic output

Unsafe source paths such as absolute paths or parent traversal are rejected.

---

## Runtime Responsibilities

The OopsMath engine is responsible for runtime behavior such as:

* player interaction
* mathematical task execution
* inventory changes
* purchasing
* block placement
* construction validation
* structural graphs
* physics simulation
* collapse behavior
* objective evaluation
* completion conditions
* failure conditions
* scoring
* events and triggers

For example, the compiler can verify that a stage declares:

```yaml
structure_stable: true
```

as part of a completion condition.

It does **not** determine whether a player's actual construction is stable.

The runtime does that.

---

# Deterministic Builds

OMSC is designed to produce deterministic packages.

For identical:

```text
source files
compiler version
configuration
manifest
```

the generated `.dat` should be byte-for-byte identical.

For example:

```powershell
python omsc.py build examples/001_first_wall -o build/a.dat
python omsc.py build examples/001_first_wall -o build/b.dat

Get-FileHash build/a.dat -Algorithm SHA256
Get-FileHash build/b.dat -Algorithm SHA256
```

The resulting hashes should match.

Determinism is important for:

* reproducible builds
* CI
* testing
* release verification
* cacheability
* debugging

OMSC therefore avoids embedding non-deterministic information such as machine-specific absolute paths, timestamps, or random identifiers into the package.

---

# Exit Codes

OMSC uses the following exit codes:

| Code | Meaning                            |
| ---: | ---------------------------------- |
|  `0` | Success                            |
|  `2` | Validation error or corrupt DAT    |
|  `3` | Invalid arguments or configuration |
|  `4` | I/O error                          |
|  `5` | Binary write error                 |

This makes the compiler suitable for scripts and CI pipelines.

---

# Example Stage

The repository contains:

```text
examples/001_first_wall/
```

This stage demonstrates the complete basic pipeline:

```text
Read lesson
    ↓
Solve multiplication / division
    ↓
Determine required bricks
    ↓
Purchase materials
    ↓
Construct the wall
    ↓
Run structural test
    ↓
Complete the stage
```

Its source files include:

```text
stage.yaml
lesson.md
solution.md
localization/fa.ftl
localization/en-US.ftl
```

Build it with:

```bash
python omsc.py build examples/001_first_wall -o build/001.dat
```

Inspect the generated package:

```bash
python omsc.py inspect build/001.dat
```

Inspect the compiled stage:

```bash
python omsc.py inspect build/001.dat --dump STAG
```

Inspect localization:

```bash
python omsc.py inspect build/001.dat --dump LOCL
```

Inspect documentation:

```bash
python omsc.py inspect build/001.dat --dump DOCS
```

---

# Compiler / Runtime Boundary

The intended OopsMath content pipeline is:

```text
                 OFFLINE
┌──────────────────────────────────────┐
│ Stage Source                         │
│ stage.yaml / FTL / MD / VOX / GLB   │
└──────────────────┬───────────────────┘
                   │
                   ▼
          ┌─────────────────┐
          │      OMSC       │
          │     Compiler    │
          └────────┬────────┘
                   │
                   ▼
             001_first_wall.dat
                   │
                   │
=================== Runtime Boundary ===================
                   │
                   ▼
          ┌─────────────────┐
          │ OopsMath Engine │
          │ Rust + Bevy     │
          └────────┬────────┘
                   │
                   ▼
              Gameplay
```

The game runtime should consume compiled `.dat` packages rather than parsing raw stage source files.

This keeps game runtime loading predictable and separates content authoring from gameplay execution.

---

# Design Goals

OMSC is designed around the following principles:

### 1. Data-driven stages

Adding a stage should primarily mean adding a new source directory instead of hard-coding stage-specific data into Rust.

### 2. Deterministic builds

The same source should produce the same package.

### 3. Runtime efficiency

The game should load a compact binary representation rather than parse YAML, Markdown, FTL, VOX, and GLB source data during gameplay.

### 4. Clear compiler/runtime separation

The compiler validates and packages data.

The runtime executes the game.

### 5. Extensibility

DAT sections, Stage Schema fields, asset registries, and runtime capabilities should be extensible without requiring every stage to use every available feature.

---

# Specification Conventions

The following conventions are normative for DAT v1 and the Stage Compiler implementation.

* Integers in the DAT format use little-endian encoding.
* The DAT header is exactly 64 bytes.
* Section directory entries are exactly 48 bytes.
* Section offsets use 16-byte alignment.
* Structured sections use MessagePack.
* Compressible sections may use Zstandard.
* CRC32 values are used for integrity checking.
* Unknown non-critical sections may be skipped by a compatible runtime.
* Unknown critical sections must cause the runtime to reject the package.
* Source paths must be relative and must not escape the stage source directory.
* Runtime behavior is never evaluated by the compiler.

---

# Current Package Model

A stage package is conceptually:

```text
001_first_wall.dat
│
├── META
│   └── stage/package metadata
│
├── STAG
│   └── compiled stage definition
│
├── WRLD
│   └── compiled voxel world
│
├── LOCL
│   └── stage-local FTL data
│
├── DOCS
│   └── embedded Markdown
│
├── ASIX
│   └── custom asset index
│
└── ASDT
    └── custom binary asset data
```

Only sections required by a particular stage are included.

---

# License

This project is part of the OopsMath project.