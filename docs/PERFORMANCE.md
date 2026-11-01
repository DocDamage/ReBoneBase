# Performance and background exports

## Background jobs

Every export runs on a shared background job (`src/jobs.rs`). The job owns a snapshot of the project, so editing continues while it renders. Progress (stage, frames done/total) appears in the status bar and the Export window. **Cancel** is honored until rendering ends; exports render completely in memory and only then write files, so a cancelled export writes nothing. Once writing starts, Cancel is disabled and the export finishes (folder exports still use the recoverable transaction described in [RELIABILITY.md](RELIABILITY.md#recoverable-exports)). Studio reference imports use the same job type and can also be cancelled. Animation and 3D-character imports still use their own worker threads.

## Unified export

**Export ↗ → Export…** (Ctrl+E) lists every target in one window: current frame (PNG / indexed PNG), sprite sheet (PNG / indexed), GIF, Aseprite (active clip / all clips with tags), batch preset folder and game-engine package. The menu shortcuts, Studio → Batch export and Production → Game export start the same job (`rebonebase::export::run`).

## Rendering reuse

- An export grows the canvas envelope once. Previously batch and package exports grew it again for every clip, rescanning all clips each time.
- Repeated frames in a sequence (ping-pong playback) are rendered once and held only until their last use (`frame_cache::SequenceRenderer`).
- The canvas keeps recent skin and onion-skin renders for an unchanged project (96 MiB of textures, 64 MiB of onion frames), so playback loops and scrubbing re-use frames. Any edit empties the cache; during drags it is bypassed.

## Undo memory

Undo used to keep a fixed 60 full snapshots. Images, audio and 3D models are reference-counted and shared between snapshots, so history now charges each snapshot only its editable data (`Project::snapshot_bytes`) and keeps up to 200 steps within a 192 MiB budget, never fewer than 10.

## Performance checks

```powershell
cargo run --release --locked --example perf_check -- --check perf/budgets.json
cargo run --release --locked --example perf_check -- --scale small --check perf/budgets.json
```

`perf_check` builds a synthetic project (`rebonebase::perf::large_project`: humanoid, 12 clips × 48 frames, half ping-pong, every bone keyed every frame), times each export path, history recording, undo/redo and cancellation, and fails if a median exceeds `perf/budgets.json`. `--json report.json` saves the measurements. Deterministic properties (render reuse, bounded history, cancellation, identical output) are covered by `tests/performance.rs`.

Measured on the development Windows machine, release build, same large project and the APIs both versions share (median of 3):

| Workload | Before | After |
| --- | ---: | ---: |
| Batch sheets, all clips (incl. writing) | 1304 ms | 527 ms |
| Batch GIFs, all clips | 1480 ms | 662 ms |
| Batch individual frames (853 files) | 6303 ms | 4288 ms |
| Godot package | 1358 ms | 573 ms |
| Active-clip GIF | 118 ms | 89 ms |
| Undo steps kept after 200 edits | 60 | 200 |
