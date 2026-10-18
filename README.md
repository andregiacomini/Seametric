# Seametric — milestone 0.1

An independent, open-source desktop prototype for industrial garment patternmaking,
written in Rust. Its code, sample geometry, and UI are independently authored.

## Run

```sh
cargo run -p pattern-desktop
```

Rust with edition 2024 support is required; verified with Rust 1.99.0 on macOS.
The lockfile pins dependencies, including [egui/eframe 0.36.2](https://docs.rs/eframe/0.36.2/eframe/). The desktop uses
eframe's Glow renderer; Linux builds need the usual X11/Wayland/OpenGL development
libraries. Windows and Linux have not yet been verified.

The application opens a **400 × 600 mm rectangular skirt panel**, with four points,
four connected segments, piece metadata, and a vertical grainline.

- Click a corner to select it; drag it to move. Selection uses a constant display radius.
- Enter X/Y in the inspector and click **Apply coordinates** for an exact move.
- Select a corner, enter **dX/dY (mm)** under **Relative move**, then click
  **Offset point** to add that displacement to its current position. For example,
  `(400, 0)` plus `(10, -5)` becomes `(410, -5)` mm. Positive Y moves upward.
  Each click adds the offset again. Changing selection clears the offset fields;
  unapplied X/Y drafts do not affect the operation.
- Click **Undo** / **Redo** for point edits, or use **Cmd/Ctrl+Z** and
  **Cmd/Ctrl+Shift+Z** (also **Cmd/Ctrl+Y** for redo). One complete drag is one
  action. Shortcuts leave text-field editing to the field. The desktop keeps
  the last 100 point edits; a new edit discards redo, and a successful load clears
  history. Undo/redo does not move the view or change selection.
- Drag empty space, or drag with the right/middle button, to pan.
- Scroll or pinch over the canvas to zoom around the cursor. **Fit piece** recenters it.
- Enter a relative or absolute JSON path and click **Save** or **Load**.
  Relative paths resolve against the process's working directory. Save replaces an
  existing file; load replaces the current document, including unsaved changes.
- The status bar reports successful operations and errors; the toolbar marks unsaved edits.

## Small architecture

```text
crates/pattern-core     geometry, document validation, typed commands, JSON
crates/pattern-desktop  egui/eframe application, canvas interaction, filesystem I/O
```

The core has no GUI, filesystem, clock, random, or network dependencies. Geometry
uses `f64` millimeters: X increases rightward, Y upward. `MmPoint` and `CanvasPoint`
are distinct types. `ViewTransform` converts to canvas-local logical display units
(egui points, not hardware pixels), inverting Y. Pan/zoom never edit geometry.
Only the renderer converts coordinates to `f32`.

Undo/redo stores stable IDs and the point positions before and after each edit
in desktop state. Restoration uses `MovePoint` through `Document::apply`, so a
failed restoration leaves both the document and history stacks unchanged. Drag
updates are applied as usual, with one history entry recorded on release. History
is disabled during a canvas gesture and is not included in document JSON. Save
retains history and updates a saved-document snapshot; the unsaved marker follows
document equality with that snapshot, including after undo/redo.

`Document` contains an ordered vector of pieces. Each piece owns point IDs unique
within that piece, segments referencing those IDs, metadata, and a grainline with
independent physical endpoints. Moving a corner therefore updates both adjacent
segments without copying endpoint coordinates. Grainlines remain unchanged by
corner edits. Selection, drafts, viewport, file path, and dirty state are UI state
and are excluded from JSON.

All desktop edits call `Document::apply(PatternCommand)`. The initial command is
`MovePoint`, with an absolute target in millimeters and stable piece/point IDs.
Commands are also serializable, giving future providers the same entry point:

```rust
use pattern_core::{Document, MmPoint, PatternCommand, PieceId, PointId};

let mut document = Document::sample_skirt();
document.apply(PatternCommand::MovePoint {
    piece_id: PieceId(1),
    point_id: PointId(2),
    to: MmPoint::new(425.0, -10.0),
})?;
# Ok::<(), pattern_core::PatternError>(())
```

Equivalent command JSON:

```json
{"type":"move_point","piece_id":1,"point_id":2,"to":{"x":425.0,"y":-10.0}}
```

The small `TranslatePoint` command adds a physical delta to one existing point:

```json
{"type":"translate_point","piece_id":1,"point_id":2,"delta":{"x":10.0,"y":-5.0}}
```

Unlike an absolute move, repeating a translation accumulates. It preserves IDs,
segment references, other points, metadata, and grainlines. Both the delta and
computed target must be finite; even finite deltas can overflow, in which case
the command leaves the entire document unchanged. The document schema stays at
version 1. Offset input fields are UI state and are not saved in the document.

Commands validate before mutation and reject unknown IDs and nonfinite targets;
failed commands leave the document unchanged. Repeating an absolute move is
idempotent. Core data structures are public for straightforward construction and
inspection; consumers must use the validated load/save/command methods rather
than bypassing them to mutate documents.

## Persistence and validation

Document JSON uses `schema_version: 1`; coordinate fields are always millimeters.
There is deliberately no configurable unit mode. Unsupported schema versions,
unknown fields, duplicate IDs, missing segment endpoints, invalid metadata,
nonfinite coordinates, and zero-length grainlines are rejected. `serde_json`'s
`float_roundtrip` feature preserves `f64` values through JSON round trips.

Desktop save serializes and validates first, writes and syncs a temporary sibling
file, then renames it over the destination. Write failures preserve the existing
destination. Load parses and validates before replacing the active document, so
invalid JSON leaves the active document intact. File I/O is synchronous, suitable
for the small milestone documents. A leftover temporary file from a crashed save
may need removal before saving again.

Validation checks document structure, not garment manufacturing correctness.
Collapsed segments, self-intersections, disconnected contours, and grainlines
outside a piece are currently permitted; no polygon or seam-allowance model exists.
Ordered vectors and explicit commands give deterministic edit ordering and JSON
output. Floating-point comparisons in geometric tests use tolerances; this is not
a promise of bitwise cross-platform results for all future geometry algorithms.

## Verification

```sh
cargo fmt --all
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Tests cover coordinate inversion, Y direction, pan, cursor-anchored zoom and limits,
invalid transforms, point editing and segment references, atomic command rejection,
document validation, provider command JSON, exact document JSON round trips,
input-driven canvas interaction (including motion and release in the same frame),
on-disk save/load and replacement, and preservation of unsaved edits after a failed load.
Relative movement tests cover accumulation, signed fractional offsets, unchanged
connections and other geometry, nonfinite input and arithmetic overflow, command
and document JSON round trips, and inspector/view behavior.
History tests cover absolute and relative edit order, saved-state tracking,
load reset, failed edit preservation, redo branching, the 100-edit limit,
keyboard focus, and drags spanning multiple frames.
Use `--offline` with a populated Cargo cache.

Verified on October 11, 2026, using Rust 1.99.0 on macOS 26.5:

- `cargo fmt --all` and `cargo fmt --all --check`: passed.
- `cargo test --workspace --offline`: 15 tests passed (10 core, 5 desktop).
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- `cargo build -p pattern-desktop --offline`: passed.
- Native GUI smoke test: rendering, selection, point dragging, numeric coordinate
  application, empty-space panning, wheel zoom, JSON save, and load restoration passed.

The smoke test caught dropped movement when motion and release arrived together;
the gesture handler was corrected and regression tests now cover that behavior.
No checks remain failing. Linux/Windows and native pinch gestures are unverified.

Relative point movement verified on October 12, 2026:

- `cargo fmt --all --check`: passed.
- `cargo test --workspace --offline`: 20 tests passed (13 core, 7 desktop).
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- `cargo run -p pattern-desktop --offline`: compiled and launched; the user
  verified the new operation in the desktop. Agent-driven native interaction
  was unavailable because Computer Use access to the temporary test app was
  not approved. The October 11 canvas and save/load smoke-test results above
  are the existing baseline; they were not manually repeated by the agent.

Undo/redo verified on October 12, 2026:

- `cargo fmt --all --check`: passed.
- `cargo test --workspace --offline`: 27 tests passed (13 core, 14 desktop).
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- `cargo run -p pattern-desktop --offline`: compiled and launched.
- Native verification of the updated buttons and shortcuts remains pending:
  Computer Use access to the temporary test app was not approved. Automated
  coverage exercises selection, dragging, pan/zoom, save/load, and history behavior;
  it does not replace a fresh manual desktop check.

## Future extension points and deliberate omissions

Add new geometry types and commands in `pattern-core`, with validation and tests.
Add schema migrations explicitly before accepting a new document version. An AI
provider can later propose typed commands through the existing boundary, with
authorization and review supplied by its host. Additional edit types can extend
the small desktop history when introduced; no event bus or plugin framework is
needed for this milestone.

Milestone 0.1 does not include LLM integration, proprietary formats, DXF, grading,
curves, seam allowances, point/segment creation, metadata/grainline editing,
snapping, printing, native file dialogs, autosave, or unsaved-change
confirmation. The sample is a geometry demonstration, not a production sewing pattern.

Licensed under MIT; dependencies retain their own licenses.
