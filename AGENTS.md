# Project rules

Scope is milestone 0.1: a small, independent garment pattern CAD prototype.

- Keep physical geometry in millimeters (`f64`), independent of egui and filesystems.
- Keep UI coordinates and selection out of serialized documents.
- Route document edits through `PatternCommand` and `Document::apply`.
- Validate documents at load, save, and command boundaries. Failed commands must not mutate them.
- Use stable piece/point IDs; segments reference points rather than copy coordinates.
- Preserve deterministic ordering; no time, randomness, network, or GUI dependencies in the core.
- Add meaningful tests for geometry, command behavior, and serialization changes.
- Keep dependencies and modules small. Do not add LLM integration, proprietary parsing,
  grading, DXF, or speculative service/plugin abstractions.
- Use independently authored code/assets/UI. Public product capabilities are inspiration only.
- Update README.md when behavior or the document format changes.

Run from the repository root:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p pattern-desktop
```

Use `--offline` for Cargo commands when the dependency cache is available.
Manually verify selection/dragging, panning, zooming, and save/load in the desktop.
