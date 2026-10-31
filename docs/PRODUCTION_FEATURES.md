# Production features: the second 20 improvements

Open **Production** (F3) in `ReBoneBase-production.exe`. These additions build on the first 20 Studio improvements; all are implemented locally in Rust.

| # | Improvement | Where to use it |
|---|---|---|
| 1 | Constrained IK for chains of multiple connected bones | Motion → Long-chain IK |
| 2 | Animated contact targets and pin strength, including release | Motion → Contacts and seamless IK / FK |
| 3 | Switch between live IK and editable FK without an integer-frame pose jump | Hold/release contacts; Bake all clips and switch to FK |
| 4 | Editable walk and run cycle generation | Motion → gait settings |
| 5 | Loop seam position and optional velocity cleanup | Motion → Clean active loop |
| 6 | Weighted animation layers, additive blending and bone masks | Motion → Animation layers and bone masks |
| 7 | Transition preview and generated transition clips | Motion → transition controls |
| 8 | Complete mirrored clips with evaluated controller motion | Motion → Mirror entire clip |
| 9 | Bone-attached sprite props, keyed transforms and parent switching | Props |
| 10 | Textured meshes, automatic weights and keyed vertex deformation | Meshes |
| 11 | Saved outfit colors and equipment combinations | Outfits |
| 12 | Sprite expressions, blinks and mouth shape tracks | Face |
| 13 | Deterministic spring secondary motion | Motion → Secondary motion |
| 14 | Named frame events with payloads | Audio & events |
| 15 | WAV playback, waveform navigation and scrub audition | Audio & events |
| 16 | Multiple characters with separate animation clocks and keyed placement | Scene |
| 17 | Per-frame pixel paint, erase and restoration | Pixels & boxes; P |
| 18 | Animated hit, hurt and interaction regions | Pixels & boxes; H |
| 19 | Godot, Unity and generic sprite packages with metadata | Game export |
| 20 | Docked/floating panels, saved layouts, remappable shortcuts and command palette | Workspace; Ctrl+P |

See [the Production guide](PRODUCTION_GUIDE.md) for workflows, supported formats and limits. Whole-body manual movement remains exclusive to **Move Rig (M)**; ordinary bone drags pose the skeleton.
