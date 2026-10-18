# Seametric

**Garment patternmaking, built in the open.**

Seametric is an independent, open-source garment pattern CAD project written in
Rust. We are building toward a future where more people can access, understand,
and shape the digital tools used to make clothes.

The prototype brings together point editing in real measurements, a navigable
canvas, undo/redo, and readable local documents. It is a small beginning toward
more accessible garment design and clothing production.

> [!IMPORTANT]
> **Work in progress — milestone 0.1.** Seametric is an early desktop prototype
> under active development. It currently edits an existing sample panel and is
> not ready for production garment workflows. Expect incomplete tools and rough
> edges; testing and feedback help guide its development.

[Why Seametric](#why-seametric) · [Features](#features) ·
[Get started](#get-started) · [Project status](#project-status) ·
[Under the hood](#under-the-hood) · [Contributing](#contributing)

## Why Seametric

Creating clothes should be within reach of more people. Seametric aims to help
democratize access to digital patternmaking tools for students, educators,
independent makers, and small clothing businesses.

Our goal is to lower the cost and knowledge barriers around digital garment tools.
We want people to be able to learn from the software, adapt it to their work, and
contribute what they discover. Open code, readable document data, and shared
knowledge are central to that goal.

Seametric starts with reliable geometry and understandable editing behavior.
Its code, sample geometry, and interface are independently authored. The desktop
works locally, with no account or online service required to edit and save.

## Features

These features are available in the current prototype, which opens a
**400 × 600 mm rectangular skirt panel** with four connected corners, piece
metadata, and a vertical grainline.

### Edit in real measurements

Drag a corner, enter exact X/Y coordinates, or apply a relative offset in
millimeters. Connected segments follow the edited point. Measurements stay in
physical units at every zoom level.

### Inspect the whole piece or a single corner

Pan across the canvas, zoom around the cursor, and use **Fit piece** to return to
the full panel. Moving the view leaves the pattern's measurements unchanged.

### Revisit your point edits

Undo and redo the last 100 point edits with buttons or keyboard shortcuts. One
complete drag counts as one edit. The unsaved-change indicator follows your
document as you edit, undo, and redo.

### Keep documents on your machine

Save and load readable JSON files with coordinates in millimeters. Documents are
validated before loading, saving, and applying edits. Failed loads and rejected
commands preserve the active document.

## Get started

Install Rust with edition 2024 support, then run from the repository root:

```sh
cargo run -p pattern-desktop
```

With dependencies already cached, you can run without network access:

```sh
cargo run -p pattern-desktop --offline
```

The lockfile pins dependencies. The desktop uses egui/eframe 0.36.2 with the Glow
renderer. Linux builds require the usual X11/Wayland/OpenGL development libraries.
Platform coverage is still limited, and running the prototype in your environment
is part of evaluating this early milestone.

<details>
<summary>Canvas controls and file handling</summary>

- **Select and move:** click a corner to select it, then drag it. Selection uses a
  constant display radius at every zoom level.
- **Set coordinates:** enter X/Y in the inspector and click **Apply coordinates**.
- **Move by an offset:** select a corner, enter **dX/dY (mm)** under **Relative
  move**, then click **Offset point**. Each click adds the offset again. For example,
  `(400, 0)` plus `(10, -5)` becomes `(410, -5)` mm. Positive Y moves upward.
  Changing selection clears the offset fields; unapplied X/Y drafts do not affect
  the offset operation.
- **Undo and redo:** use the toolbar buttons or **Cmd/Ctrl+Z** to undo and
  **Cmd/Ctrl+Shift+Z** or **Cmd/Ctrl+Y** to redo. One complete drag counts as one
  edit. Shortcuts leave text-field editing to the field. A new edit discards redo
  history; a successful load clears all edit history.
- **Pan:** drag empty space, or drag with the right or middle mouse button.
- **Zoom:** scroll or pinch over the canvas. **Fit piece** recenters the view.
  Native pinch gestures still need manual verification.
- **Save and load:** enter a relative or absolute JSON path and click **Save** or
  **Load**. Relative paths resolve against the process's working directory. Save
  replaces an existing file; load replaces the active document, including unsaved
  changes. There is currently no unsaved-change confirmation.

</details>

## Project status

**Milestone 0.1 is a geometry and interaction prototype.** The sample panel is a
demonstration, not a finished sewing pattern. The current work establishes the
editing, validation, and persistence foundations for a larger garment tool.

Curves, seam allowances, point or segment creation, metadata or grainline editing,
snapping, grading, printing, and external CAD file formats are outside this
milestone. Native file dialogs, autosave, and unsaved-change confirmation are also
unavailable.

Platform coverage is limited. Native pinch gestures and the updated undo/redo
buttons and shortcuts still need a fresh manual desktop check. Automated tests
cover interaction behavior, but do not replace that check.

The long-term direction is broader access to garment design and production tools.
Progress starts with making this small prototype dependable and learning from
people who make clothes.

## Under the hood

```text
crates/pattern-core     geometry, document validation, typed commands, JSON
crates/pattern-desktop  egui/eframe application, canvas interaction, filesystem I/O
```

- **Physical geometry:** `f64` millimeters, independent of the interface.
- **Explicit edits:** typed commands through one validated document boundary.
- **Stable connections:** segments refer to point IDs instead of copying coordinates.
- **Local persistence:** versioned JSON with UI state kept out of documents.
- **Small core:** no GUI, filesystem, clock, random, or network dependencies.

<details>
<summary>Document model, command examples, and persistence details</summary>

Physical geometry uses `f64` millimeters, with X increasing rightward and Y upward.
The core has no GUI, filesystem, clock, random, or network dependencies.
`MmPoint` and `CanvasPoint` are distinct types, and `ViewTransform` converts physical
coordinates to canvas coordinates. Pan and zoom never change document geometry;
only the renderer converts coordinates to `f32`.

Documents contain ordered pieces with stable piece and point IDs. Segments refer
to point IDs, so connected segments follow an edited point without copying its
coordinates. Grainlines have independent physical endpoints and remain unchanged
when a corner moves.

All document edits go through `PatternCommand` and `Document::apply`. Commands
validate before mutation; rejected edits leave the document unchanged. The current
commands are `MovePoint`, for absolute coordinates, and `TranslatePoint`, for
relative offsets:

```rust
use pattern_core::{Document, MmPoint, PatternCommand, PieceId, PointId};

let mut document = Document::sample_skirt();
document
    .apply(PatternCommand::MovePoint {
        piece_id: PieceId(1),
        point_id: PointId(2),
        to: MmPoint::new(425.0, -10.0),
    })
    .expect("Valid point move");
```

Both commands can also be serialized as JSON:

```json
{"type":"move_point","piece_id":1,"point_id":2,"to":{"x":425.0,"y":-10.0}}
```

```json
{"type":"translate_point","piece_id":1,"point_id":2,"delta":{"x":10.0,"y":-5.0}}
```

Repeating an absolute move leaves the point at the same target; repeating a
translation accumulates. Nonfinite coordinates, invalid IDs, and arithmetic
overflow are rejected. Core data structures are public for construction and
inspection; edits should use the validated command boundary.

Selection, input drafts, viewport, file paths, unsaved-change tracking, and
undo/redo history belong to desktop state and are excluded from saved documents.
History restores point positions through the same command boundary. Saving keeps
history and records a document snapshot used to track unsaved changes, including
after undo and redo.

### Persistence and validation

Document JSON uses `schema_version: 1`, with all coordinates in millimeters.
Unsupported versions, unknown fields, duplicate IDs, missing segment endpoints,
invalid metadata, nonfinite coordinates, and zero-length grainlines are rejected.
The JSON serializer preserves `f64` values through round trips.

Save validates and serializes the document, writes and syncs a temporary sibling
file, then renames it over the destination. Write failures preserve the existing
destination. Load parses and validates before replacing the active document;
invalid input leaves the active document intact. File I/O is synchronous for the
small documents in this milestone. A temporary file left by an interrupted save
may need removal before saving again.

Validation checks document structure, not garment manufacturing correctness.
Collapsed segments, self-intersections, disconnected contours, and grainlines
outside a piece are currently permitted. Ordered data and explicit commands keep
edit ordering and JSON output deterministic. Geometry tests use floating-point
tolerances; this does not guarantee identical floating-point results across all
platforms for future algorithms.

</details>

## Development and verification

Run these checks from the repository root:

```sh
cargo fmt --all --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo run -p pattern-desktop --offline
```

Omit `--offline` when dependencies need to be fetched.

Tests cover geometry and view transforms, point editing, segment references,
atomic command rejection, relative movement and overflow, document validation,
JSON round trips, canvas interaction, save/load, and undo/redo behavior.
Manually verify selection, dragging, panning, zooming, and save/load in the desktop
when changing interaction behavior.

## Contributing

You can help shape Seametric whether you write code, teach patternmaking, make
clothes, or are learning your first pattern. Useful contributions include:

- Trying the prototype and describing where an interaction becomes confusing.
- Reporting bugs with reproduction steps, build environment, and sample JSON
  when relevant.
- Improving setup instructions and explaining patternmaking concepts.
- Sharing practical needs from classrooms, home sewing, and small clothing
  businesses.
- Proposing focused code changes with meaningful verification.

Start with [AGENTS.md](AGENTS.md) for the project rules. Keep proposed changes small
and aligned with the current milestone. Geometry, command, and serialization
changes should include meaningful tests. Preserve the separation between physical
geometry and UI state, validate edits before mutation, and update this README when
behavior or the document format changes. New document
versions need explicit schema migrations.

## License

Seametric is licensed under the [MIT License](LICENSE). Dependencies retain their
own licenses.
