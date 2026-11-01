# Engine playback verification

One diagnostic fixture is played through the Godot, Unity and Unreal integrations by real editors and runtimes. The results are recorded in [evidence/engine-check.json](evidence/engine-check.json).

```
python scripts/engine_check.py [--only godot,unity,unreal]
```

The driver regenerates the fixture, creates fresh engine projects under `target/engine-check`, runs every check, and writes the evidence file. It exits nonzero if any check fails or an engine cannot run.

## The fixture

`cargo run --example create_engine_fixture` writes `EngineFixture.rebone` and, per engine, a `main` package (every clip) and a `directions` package (a four-direction set). Both use packed atlas pages limited to 64 × 64, so the fixture always spans several pages.

| Case | Where |
| --- | --- |
| Off-centre, fractional pivot (14.4, 34) | every frame |
| Variable holds: 40, 60, 100, 150, 170, 250, 400 ms | Walk, Attack, Run |
| Empty frame (one transparent pixel) | Walk frame 4 |
| Duplicate frames sharing pixels, each with its own record | Walk 6–8, Sway (ping-pong) |
| Two markers on one frame, in authored order | Walk frame 3 |
| Regions that move, resize, tilt, and appear only for frames 2–5 | Walk, Attack |
| Looping, non-looping and ping-pong clips | Walk, Attack, Sway |
| Audio that starts 300 ms into the source timeline | all clips |
| Four directions with a mirrored west, and a direction switch mid-clip | `directions` |
| Pause, resume and restart | `main` |

## What each engine checks

All three check, against `package.json`:
- the frame shown over time against the exported holds;
- marker names and order, including frames passed within one long tick;
- region placement (every corner) and that region objects are reused, never recreated;
- that non-looping clips stop on their last frame;
- pause (frame, time and audio held, no markers), resume and restart (frame 0, its markers once);
- that a direction switch keeps frame and time and fires nothing;
- every frame's page, rectangle, trim restoration and pivot.

**Audio sync rule:** the audio position is the frame's retained source time plus the time spent in the frame, minus the track's start. Audio is silent before the track starts. Each engine checks the position its helper asks for, to the millisecond.

| Engine | Version | How it runs |
| --- | --- | --- |
| Godot | 4.7.1 | `ReBonePlayer.gd` on a real `AnimatedSprite2D`, headless, driven by Godot's own frame loop. The check runs on a fixed 240 fps clock, a coarse 10 fps clock (frames skipped within one tick), and in real time to measure audio drift against Godot's mixer. |
| Unreal | 5.8.3 | The importer runs headless in a new project. `verify_import.py` compares every imported sprite, texture and flipbook with the package. The plugin's automation tests step `ReBoneFlipbookComponent` at 1/240 s and in one long tick. |
| Unity | 6000.4.6f1 | Batch-mode editor: `ReBoneImporter` imports both packages twice (the second time must update the same actor). `ReBoneCheck` then steps `ReBonePlayer` at 1/240 s and 1/10 s. |

## Results (2026-11-01, one run of all three engines)

| Engine | Result |
| --- | --- |
| Godot 4.7.1 | All six runs passed: 13,790 checks. Real-time audio drift against the mixer, headless dummy driver: main package median 43 ms, 90th percentile 71 ms, maximum 90 ms; direction set median 58 ms, 90th percentile 78 ms, maximum 94 ms. |
| Unreal 5.8.3 | The plugin built. Both packages imported (main: 6 clips, 21 sprites, 4 pages; directions: 4 clips, 14 sprites, 3 pages). All 47 frames matched the package, including fractional pivots. Both automation suites passed. |
| Unity 6000.4.6f1 | Both packages imported, and importing again updated the same actors. All 10 clips passed at 1/240 s and 1/10 s, along with placement, pause/resume/restart and direction switching: 13,217 checks. Atlas pages kept non-power-of-two sizes such as 64 × 63. |

## Problems this fixture found and fixed

- Unreal's Paper2D snapped fractional custom pivots to whole pixels. This moved the anchor by up to half a pixel against the other engines, so the importer now turns snapping off.
- Unreal's component re-fired a frame's markers when a tick ended exactly on a boundary, because Paper2D assigns a boundary instant to the earlier frame. It also replayed a whole loop's markers when a tick ended within tolerance of the loop end.
- Audio whose start fell inside a frame stayed silent until the next frame in all three helpers. Each helper now starts audio when it becomes due.
- Unity's importer let Unity rescale non-power-of-two atlas pages to powers of two, which breaks every sprite rectangle; it now keeps pages at their size. Unity's player also recreated region colliders every frame; it now reuses them.
- Godot's helper freed and recreated region polygons every frame; it now reuses them.
- The Unreal importer put a rig's clips and its direction set in the same folder, so importing one deleted the other. Imports now go to a folder named after the export folder.
