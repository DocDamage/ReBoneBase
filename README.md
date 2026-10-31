# ReBoneBase

A standalone Windows skeletal pixel-animation editor written in Rust with egui/eframe. Aseprite, Electron, a browser, Node.js, and Lua are **not required to run it**.

This is a local port of the supplied Bonebase 2.2.2 extension. The original archive and extracted source remain in this workspace. REA was used to inventory and identify the shipped artifact; source comparisons guide the Rust implementation.

## Run

Open `dist/ReBoneBase-motion.exe` for the latest build, or build from source:

```powershell
cargo run --release
```

You can pass a `.rebone` project or `.bbrig` rig as the first command-line argument. The executable contains its templates and fonts and can run from any folder.

## Workflow

1. Start with the Humanoid rig or choose one of the 20 original starter rigs.
2. In **Rig**, drag bone ends, draw connected bones, adjust joint radii and skin bulges, and assign colored groups. Linked twins support mirrored rest-pose gestures.
3. In **Animate**, drag bones for FK, use the IK tool on a lower limb, and scrub the dope sheet. Pose edits create keys automatically. Multiple clips, eight easing modes including Cubic tangents, and a pose library are included.

Whole-body translation uses **Move Rig (M)**: select it and drag in the canvas. It moves all skeleton roots together; in Animate mode it keys their movement. Ordinary root grabs stay locked, loose-limb body drags rotate at their joint, and IK only acts on connected limb ends without driving the root. Middle/right dragging moves the camera.
4. Import a reference image from File. Adjust its position, scale, and opacity in the inspector.
5. Import FBX, BVH, glTF, GLB or text Unity `.anim` skeletal motion from File for a 2D rig, or Production → 3D characters for an existing 3D model. Preview joint mapping and import selected clips; the 3D dialog can drive both targets together. Unreal `.uasset` and binary/Humanoid-only Unity clips require an engine FBX export.
6. Save `.rebone` projects. Export transparent PNG frames, PNG sprite sheets, GIF animation, or layered Aseprite animation.

Shortcuts: **B** draw bones, **V** select, **M** move rig, **R** radius, **I** IK, **K** key pose, **Space** play/pause, **Left/Right** step, **F/0** fit, **Ctrl+Z** undo, **Ctrl+Shift+Z** redo, **Ctrl+S** save, **Ctrl+O** open. Middle/right drag pans; scroll zooms.

## Unity and Unreal animation import

External animation can drive existing rigged 3D characters and 2D skeletons with attached sprite artwork. Model animation choices, speed, looping and offsets belong to individual timeline clips and survive save/load, undo, trim, split and reorder. Native animation-only FBX files do not need a mesh. Each import batch commits only after all clips validate.

See the [Animation import guide](docs/ANIMATION_IMPORT_GUIDE.md) for formats, mapping, projections and engine exports. `integrations/unreal/export_animations.py` helps export selected Unreal Animation Sequences as FBX. A standalone `.uasset`, binary `.anim`, Animator Controller or Animation Blueprint needs the owning engine project to bake/export its evaluated skeleton motion. Unrigged PNGs and static unskinned models need rigging before articulated motion can drive them.

## Canvas, attachments, 3D and animation templates

The canvas now grows with the character in all directions and preserves joint/pixel positions. Ordinary limb and head drags rotate around fixed attachments in Rig and Animate modes. **Move Rig (M)** moves the whole character.

Use **Animation templates…** beside the dope sheet for ten editable motions. Use **File → Import 3D character…** or **Production → 3D characters** to add actual **GLB/glTF, FBX or OBJ** models with textures, skinning and included animations. Camera controls and joint pose overrides render the model into the pixel canvas and existing export formats. FBX/OBJ loading is native; no Blender installation is required.

See the [Characters guide](docs/CHARACTERS_GUIDE.md). Included examples: `dist/CharactersDemo.rebone`, `dist/AnimationTemplates.rebone`, and `dist/Mannequin.obj` with its `.mtl` material file. All older executables are preserved separately.

## Compatibility and scope

- Native `.rebone` JSON projects preserve rig geometry, groups, clips, keyframes, reference images, and saved poses. Saves validate documents and replace files through a same-directory temporary file.
- `.bbrig` imports use a bounded Rust parser for Lua **data literals**, without executing file contents. Unsupported source features are reported during import. Original live IK controllers, embedded/per-frame reference images, saved source poses, ground constraints and global-root tracks are not transferred. Native ping-pong and per-frame durations are imported. Bake source animations before importing when those features are required.
- Native IK supports persistent constrained hand/foot pins and ground contact. Native 3D character import and planar motion retargeting are described above; the original controller schema and live Aseprite connection are outside this standalone port. New bone creation currently creates one bone at a time; mirror gestures apply to existing linked twins.
- BVH/glTF retargeting is a native planar FK importer with automatic and manual joint mapping. It preserves target bone lengths and root movement. glTF supports selecting and importing multiple named animations. Morph targets, embedded models/materials, and arbitrary camera orbit are outside this importer. External glTF buffers must be inside the selected file's directory.
- Rendering uses the original tapered capsule, cosine bulge, and smooth-union equations. Tests compare all 20 original templates: 81,917 of 81,920 pixel colors match exactly; three boundary classifications differ by less than 0.0001 in the signed-distance field because the editable Rust geometry uses 32-bit floats.
- File, canvas, keyframe, image, and export-work budgets prevent unexpectedly large imports or exports from allocating unbounded memory. Long motion is resampled to at most 240 frames. Batch export supports the active clip or all clips.

## Verify

```powershell
cargo test --all-targets
# With FFmpeg installed, also run the two optional decoder tests:
cargo test --all-targets -- --include-ignored
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

The Lua dependency is development-only: tests execute the original trusted source as an oracle for transforms, animation, IK, rasterization, and serialized legacy rigs. It is absent from the application's runtime dependency graph. `cargo run --example extract_templates` regenerates native template data from the supplied source.

Animation import acceptance: **270 Rust tests passed**, including two optional real-FFmpeg checks, with **87.79% source line coverage** (examples and test-only source files excluded). Two local Unreal-helper API mock tests also pass; actual Unity/Unreal Editor execution has not been performed. Formatting and Clippy with warnings denied pass. Coverage and release details are in `docs/evidence/coverage-motion.json` and `docs/evidence/release-motion.json`. The packaged Windows app passed native file import, mapping, shared 2D/3D preview, scrubbing and undo checks. The sample in `examples/output/Wave.rebone` has matching PNG, GIF, sprite-sheet and layered Aseprite exports.

All 20 requested improvements are available through **Studio** and the motion-import dialog: [feature list](docs/FEATURE_IDEAS.md), [Studio guide](docs/STUDIO_GUIDE.md). Studio adds recovery/backups, selection/constraints/pins, key/curve/clip editing, shared libraries, onion controls, outlines/palettes, reference sequences/video and batch presets with atlas metadata and Aseprite tags. Video decoding uses an existing FFmpeg installation; choose its executable in Studio.

The second 20 improvements are available through **Production (F3)**: [feature list](docs/PRODUCTION_FEATURES.md), [Production guide](docs/PRODUCTION_GUIDE.md). They add long-chain IK, animated contacts, FK baking, gait/loop/layer/transition tools, mirrored clips, sprite props, textured weighted meshes, outfits, facial tracks, springs, audio/events, multiple actors, pixel corrections, collision regions, game packages and customizable workspaces. **Commands / Ctrl+P** searches available actions. Open `dist/ProductionDemo.rebone` for a working example.

Game packages contain baked sprite sheets and metadata with Godot/Unity helper scripts. Godot runtime loading, events and collisions were exercised in Godot 4.7.1; the Unity Editor integration has not been run here. Sound import supports PCM16 WAV. Use this latest executable to save projects with the new feature data; older releases may omit data they do not understand.

Build a portable local copy with `scripts/package.ps1`. It writes the executable, sample project, README, and original license into `dist`.

See [porting evidence](docs/PORTING.md) for source identity, REA evidence, and verification details.

## Original notice

The supplied Bonebase license allows source modification for personal use and prohibits redistribution of the original or modified product. The original notice is preserved in `extension/LICENSE.txt` and `BONEBASE_LICENSE.txt`. Publication permission was confirmed by the repository owner. The preserved license notices describe the upstream terms; this repository does not replace them.
