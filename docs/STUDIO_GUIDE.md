# ReBoneBase Studio

Run the latest versioned ReBoneBase executable. Use the main [animation workspace](WORKSPACE_GUIDE.md) for common editing, or open **Studio** in the top bar for the full tool set. The original 20 templates, FK/IK editor and existing `.rebone` files remain usable.

## Rig & pins

Hover a bone to highlight it. Endpoints take priority over bone bodies; a parent's tail takes priority over its child's coincident head. Alt-click or Tab cycles overlapping bones. Shift-click adds/removes a bone; drag empty canvas to box-select. Locked bones can still be selected in the skeleton tree and unlocked in the Selection inspector or Studio.

Set individual editing locks, length locks, pose rotation limits and a preferred IK bend. Rotate selected joints together. Rest joint translation and detachment are disabled for existing attached limbs; their local attachment offsets stay fixed. Root anchors are excluded from selection transforms. **Move Rig (M)** is the dedicated whole-body tool: drag anywhere to move all roots. In Animate mode it keys root movement; planted pins can keep hands/feet fixed while the body moves.

Pin a selected lower limb end, or use **Plant both feet**. Grounded pins account for the visible sole rather than placing the ankle on the floor. In IK mode, drag a pinned limb end to change its target. Pins respect joint limits and editing locks. Unreachable targets stay as close as the constrained chain allows. Pins persist in projects and bake into animation exports.

Existing attached bones retain their parent and attachment point. A newly authored unattached root can be connected to start a chain; other hierarchy operations cannot detach existing extremities. See the [Characters guide](CHARACTERS_GUIDE.md) for the updated movement rules.

## Animation

Click timeline diamonds, Shift-click to extend selection, then drag to move them. Ctrl+C copies selected keys; Ctrl+V pastes at the playhead. Delete removes selected keys before deleting a bone. Studio provides select-all, snapping, offset and time-scale controls. Retiming rejects keys outside the clip instead of silently losing them.

The main Timeline **Curves** tab and Studio share the same editor. It shows the selected bone's channel, or its group's depth/visibility channel. Add a key, drag points, adjust exact frame/value fields, and choose easing. Yellow handles edit Cubic tangents in value per frame. Editing an incoming tangent automatically enables Cubic easing on the preceding segment.

Duplicate, delete, reorder, trim and split clips. Trim/split preserve sparse keys, interpolation, live controllers and spring history, including fractional-frame poses. Keys outside the visible range are retained for evaluation but hidden from the dope sheet and curve handles. Extending a clip holds its prior endpoint. Adding or editing a key later resumes the entire clip's retained motion through that frame; audio and reference time keep advancing during holds. Explicit FK baking is still available in Production. See [clip timing details](RELIABILITY.md#editable-clip-windows).

Choose Forward, Reverse or PingPong and set individual frame holds in milliseconds. The main Play button and exported timing use these settings. Enable the root-motion path or convert the active clip to an in-place cycle.

## Motion import

Use **File → Import 3D motion** for BVH, glTF or GLB. Choose a named animation to preview, scrub the source skeleton and actual target mannequin, and adjust joint mapping, projection and root scale. Save/load retarget profiles by joint names so node-order changes do not invalidate a mapping. Select several animations to import separate clips in one undoable operation. Failed batches leave the current rig intact.

The importer is planar skeletal retargeting. It preserves target lengths and root movement; it does not import meshes, materials or morph targets. Motion is sampled to at most 240 frames per clip, and projects support up to 32 clips. External glTF buffers must stay inside the selected file's directory.

## Appearance & library

Set onion skins before/after the playhead, spacing, tints and opacity. Configure pixel outline width/color per group. Capture group colors in a shared palette, edit swatches, and apply them to selected bones' groups. Indexed PNG export maps to that palette and preserves transparent pixels.

The shared library stores rigs and poses with thumbnails, tags and favorites. Search by name/tag, filter favorites, load a rig or apply a pose to matching bone names in another project. Rig/Pose categories distinguish entry types. Library files stay local.

## References

Import naturally sorted image sequences, or decode a video/animated GIF in the background. Video requires FFmpeg; choose its executable in Studio if unavailable on PATH. An existing Krita installation's FFmpeg is detected on Windows when present. FFmpeg is not bundled or installed by this app.

Set reference FPS, start offset, looping, position, scale and opacity. References follow the active clip's timeline, including frame holds. Imports keep at most 240 frames and 16 megapixels in total; video extraction is limited to 256 pixels per side and 90 seconds of decoding time.

## Recovery & files

Dirty projects get local recovery snapshots every 30 seconds by default; configure 10–600 seconds or snapshot immediately. Startup opens the recovery panel when snapshots exist. Recover a snapshot as a new project, then save it explicitly. Saving an existing file retains ten previous versions; restore one as a new project without overwriting its source.

Application data lives in `%APPDATA%\ReBoneBase`, or `REBONEBASE_HOME` when set. This includes preferences, recovery snapshots, backups, retarget profiles and the shared library. Project saves use validated JSON and same-directory temporary files.

Recovery discovery validates the snapshot itself, including snapshots whose sidecar metadata is missing, damaged or interrupted. An unmatched snapshot is offered without an assumed original save location. Warnings identify files that could not be loaded; discovery preserves them on disk. Backups are written before replacing an existing project, and old-backup pruning occurs only after a successful save. Cleanup warnings do not turn a completed save into a failure.

Schema 1 projects upgrade in memory when opened. The original file stays unchanged until you save. Schema 2 files require a compatible build; unsupported fields, features and newer versions stop loading with a reason.

## Batch export

Save named presets for RGBA sheets, indexed sheets, GIFs, layered Aseprite files or individual PNG frames. Choose all clips or the active clip, naming prefix/suffix and sheet columns. Each batch writes `atlas.json` with clip names, source frames, rectangles and durations. Aseprite output includes named animation tags; the main Export menu can also write all clips in one tagged file.

Existing files require **Replace existing files**. Export renders and validates the complete batch before publishing, with bounded memory/render work. Pins and outlines are included. Output is staged and original files are retained until publication succeeds. A failed publication attempts rollback; if that cannot complete, the error identifies the retained recovery directory and the next export attempts recovery first. Unrelated files are preserved. This provides recoverable publication, not simultaneous visibility of every package file. Export requires a filesystem with hard-link support, checked before existing files are moved. See [export recovery](RELIABILITY.md#recoverable-exports).

Ctrl+Z / Ctrl+Shift+Z undo/redo project edits. Library entries, saved preferences/profiles, exported files and recovery snapshots are separate local files.
