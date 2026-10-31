# Porting evidence

## Target identity

- Artifact: `Bonebase-2.2.2.aseprite-extension`.
- SHA-256: `6758415e3bd05206b6331841cdeff0022607902c6e225040c3a427467af9afd7`.
- REA repository requested: https://github.com/DocDamage/rea, inspected at commit `23de10bb2849cdce8ddbe70504aa7cc57de75a98`.
- Workflow: `.agents/skills/reverse-engineer-anything/SKILL.md` from that checkout.
- Actual CLI: existing local REA 6.4.0. The requested 6.8.0 package-runner launch did not resolve an executable, so inspection used the installed entrypoint directly. No Codex settings or registrations were modified.
- Operation: `inspect-artifact <absolute archive path> --format json --full-output`.
- Complete output: `evidence/rea-artifact.json`.
- Inspection Evidence: `ev_1b3048402a960fbb47112df2f2e7ac134f19a4c89ca1d90ece4b5cf8a5507ffc`.
- Inventory Evidence: `ev_8a7610135c652673aa7726e337558f8f9867b96e84448bd9deb708fb797c5fcf`.

REA observed a ZIP artifact graph with 48 nodes/occurrences (root plus 47 files). This establishes artifact identity and archive contents. It does not establish runtime behavior or Lua semantics. Semantic findings below come from source inspection and executable source-oracle comparisons, not REA decompilation.

## Source-to-port map

| Shipped source | Native implementation | Verification |
|---|---|---|
| `src/engine/rig.lua` | `src/model.rs` | Hierarchy, loose/connected bones, local transforms, all template FK, posed transforms, validation |
| `src/engine/anim.lua` | `src/animation.rs` | Seven easing functions, raw keyed rotation interpolation, shortest loop seam, scale bounds |
| `src/engine/ik.lua` | `src/ik.rs` | Two-bone reachable/unreachable/coincident targets and both bend directions |
| `src/engine/skin.lua` | `src/skin.rs` | All 20 template images, part stacking/visibility, smooth-union field, boundary tolerance |
| `src/engine/templates.lua` | `assets/templates.json` | Trusted development extractor plus all-template oracle comparisons |
| `src/kit/serial.lua`, rig serialization | `src/legacy.rs` | Safe data-only parser, actual original rig files, malformed/code/depth/size rejection |
| Original host dialogs and canvas | `src/app.rs`, `src/app/` | Native Windows observation and headless pointer-gesture/undo regressions |
| Original host sprite export | `src/aseprite.rs`, `src/project.rs` | Independent Aseprite file reader, PNG/GIF decoding, dimensions, layering and durations |
| BVH/glTF workflow | `src/motion.rs` | Native parser/retargeter fixtures, valid keys, malformed inputs and buffer-path boundaries |

The native host, project format, file dialogs, motion import interface, pose storage, and exports are port design choices. The README identifies behavioral differences and unsupported source features. No Aseprite runtime comparison was performed.

## Verification approach

Engine and parser changes began with missing-implementation or behavioral tests. The GUI pointer test reproduces the real desktop drag path through egui input events and verifies that IK writes keys and undo restores the previous clip. Runtime Lua is not used: `mlua` exists only as a dev dependency for the trusted source oracle.

Final acceptance results are recorded in `CODEX_CHECKPOINT.md`.

Final local acceptance: 62 tests passed, 86.29% source line coverage, clean formatting and Clippy with warnings denied, successful optimized Windows build. The packaged executable was launched and exercised through the native UI; IK dragging, automatic key creation, stable canvas layout, and fast Ctrl+Z input were verified. Portable release SHA-256: `73de444f36b8bed1837d1cb83c6471ce89e92d480d16d1c9b4985b8b904560e8`.

## Movement follow-up

The user explicitly requested whole-body translation only through a dedicated Move Rig tool. Ordinary root drags are now locked. Generic IK requires a connected tail and cannot solve through a root bone; loose-limb pose drags rotate at their fixed joint rather than translating their head. Move Rig (M) translates every independent root together in Rest, including the mirror axis, or keys root X/Y in Pose while preserving existing offsets. A pivot guard prevents zero-lever FK jumps; history navigation ends an active drag.

This is an intentional change from the original editor's root-body translation behavior. Eight additional regressions cover root protection, torso IK, shoulder attachment, multiple roots, prior pose offsets, pivot presses, keyboard/pointer Move Rig, and active-drag undo. Final acceptance is 70 passing tests, 86.64% source line coverage, clean formatting/Clippy, and native Windows checks. The existing user session was preserved; the revised executable is `dist/ReBoneBase-updated.exe`, SHA-256 `753d88ec562ad1a88597cad128f1a078b4101c6a09603d03f3d48049d186ff0c`.

## Studio feature release

All 20 requested feature improvements are implemented; the [Studio guide](STUDIO_GUIDE.md) describes the native workflows and limits. New Rust services handle persistent constrained pins, editing operations, local recovery/library storage, reference decoding and batch export. The native motion dialog previews the actual target mannequin, stores named retarget profiles and atomically imports selected animations in the background.

Acceptance: 130 passing tests including two explicit real-FFmpeg checks, 87.62% source line coverage, clean formatting/Clippy and a locked optimized Windows build. Source-oracle tests remain green. Pointer regressions verify fast timeline/curve drags, scrubbing, tangent behavior, hierarchy/pin interactions and undo; independent file readers verify indexed PNG and tagged Aseprite output. Native smoke exercised Studio pins and curves, undo, startup recovery and source/target previews. Release details: `evidence/release-studio.json`; executable `dist/ReBoneBase-studio.exe`, SHA-256 `5a44180508625e639f9190522b936a972d838c9246e44806021ed0115fb9cb01`.

## Production feature release

The second 20 requested improvements are implemented in Rust, alongside the original port and Studio features. The [Production feature list](PRODUCTION_FEATURES.md) and [guide](PRODUCTION_GUIDE.md) describe motion controllers, clip-aware editing/baking, textured artwork, scene/audio/event workflows, pixel corrections, collision metadata, engine packages and workspace customization. Whole-body manual translation remains exclusive to Move Rig.

Acceptance: 211 passing tests including two real-FFmpeg checks, 87.30% source line coverage, clean formatting/Clippy and a locked optimized Windows build. One independent review was completed and its findings addressed. Tests cover displayed-pose preservation through temporal edits, released IK channels, bind snapshots/texture seams, malformed input budgets, whole-scene Aseprite decoding, engine package metadata, GUI authoring and undo. Godot 4.7.1 loaded both exported clips and exercised footstep events and collision creation. Native Windows checks exercised the packaged app with the demonstration project, gait generation/undo, WAV audition, palette search/activation, independent Move Rig placement and pixel stroke/undo. Unity Editor integration and physical speaker output were not measured.

Release details: `evidence/release-production.json`; executable `dist/ReBoneBase-production.exe`, SHA-256 `ec5e219fd809623058003d5561b4879f0267f2977b80be9999167a9cda5d2f50`. All earlier executables remain preserved. No publication or redistribution was performed.

## Canvas, attachment and 3D character release

The [Characters guide](CHARACTERS_GUIDE.md) documents automatic growing bounds, permanent authored joint anchors, ten new editable motion templates, and actual model import for GLB/glTF, FBX and OBJ. The bundled original source contains 20 rig templates but no reusable animation preset library; the ten motion templates are new native authored equivalents. Model data is embedded in projects and rendered with a native orthographic depth buffer into the pixel canvas. FBX/OBJ normalization uses [ufbx-rust](https://github.com/ufbx/ufbx-rust); its [upstream license](https://github.com/ufbx/ufbx-rust/blob/master/LICENSE) is preserved in UFBX_LICENSE.txt.

Acceptance: 245 passing tests including 2 real-FFmpeg checks, 87.53% source line coverage, clean formatting/Clippy and locked optimized build. Source-oracle tests remain green. New regressions cover negative growth, corrected pixels/scene actor rasters, export pivots, near-normalized weighted meshes, fixed limb heads, animation-layer/legacy offset protection and stable history camera behavior. Native model fixtures exercise GLB/glTF texture/skin/animation, OBJ+MTL, FBX skin/animation, z-buffer ordering, alpha seams, malformed paths and numeric overflow. Independent review findings were fixed with runtime regressions.

Native Windows checks exercised the packaged templates and 3D demonstration, arm rotation with fixed head offsets/length and upward canvas growth, template addition/undo, 3D camera changes, and final-build Move Rig expanding 96x96 to 167x96 with steady undo back to 96x96. Evidence: evidence/release-characters.json and native screenshots. Executable dist/ReBoneBase-characters.exe SHA256 `4a24a03022765bbf9d283fd80018efc69ad28adbca19c15ac502bd67623498ec`. Earlier executables/sessions were preserved; no publication or redistribution occurred.


## External animation import release

The [Animation import guide](ANIMATION_IMPORT_GUIDE.md) covers Unity/Unreal exchange motion for existing 3D models and 2D rigs. Native meshless FBX, BVH, GLB/glTF and text Unity transform .anim clips support mapping and previews; a batch can drive both target types together. Unreal .uasset and binary/compressed/Humanoid-only Unity clips require the owning engine project to bake/export skeleton motion. A local Unreal batch-export helper is included, with two API mock tests; actual engine Editor integration was not executed.

3D playback belongs to each main timeline clip. Project-aware editing retains assignments and source-time offsets; custom frame holds, retiming and open-clip endpoints have runtime regressions. Source transformations remain anchored to target joints. Canvas-envelope preflight limits large multi-clip geometry scans.

Acceptance: 270 Rust tests passed including two optional real-FFmpeg checks, 87.79% source line coverage, formatting/Clippy clean, locked optimized build. One independent integration review completed; accepted endpoint, timing, visibility and work-budget findings were fixed and verified. Final packaged native checks exercised actual Windows file picking, joint mapping, simultaneous preview/import, scrubbing and undo. App left open at the clean shared-motion example. Executable `dist/ReBoneBase-motion.exe`, SHA256 `5c70029dcf942ae707d13d5628efaa1afe0e623897c6d08e19b35924dbc8da26`, 11,238,400 bytes. Details: evidence/release-motion.json and coverage-motion.json. Prior builds and original extension remain preserved.
