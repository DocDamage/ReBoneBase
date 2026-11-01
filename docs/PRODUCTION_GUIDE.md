# ReBoneBase Production

Run the latest versioned ReBoneBase executable. Open **Production** from the top bar or press **F3**. The first 20 improvements remain in **Studio (F2)**. The included `ProductionDemo.rebone` shows a walk cycle, sword, weighted cape, facial track, second character, events and a collision region. Pass its path as the executable's first argument, or use File → Open.

## Movement and animation

**Move Rig (M)** is the dedicated whole-body translation tool. Ordinary bone and root drags do not translate the whole mannequin. Select a scene actor in Production → Scene → Move Rig target to translate that character instead. Rest mode changes placement; Animate mode creates placement keys. Middle/right dragging pans the camera.

Production → **Motion** contains the new animation tools:

- Select a connected terminal bone, set a chain length, and create long-chain IK. In IK mode, drag its final tail to move the target. Root anchors remain fixed. Strength and iterations are adjustable; joint limits still apply.
- Select a hand, foot or lower limb and choose **Hold selected limb here (IK)**. Target, strength and bend keys belong to the active clip. **Release selected contact here** keys zero influence. Strength interpolates, allowing a gradual handoff to FK. A hold captures the current endpoint rather than jumping to a default target.
- **Bake all clips and switch to FK** samples the displayed motion into ordinary editable bone keys and removes live controllers. It preserves integer-frame poses, including contact releases. Subframe motion is resampled; keep a saved copy if you want the procedural controls later.
- Generate a walk/run clip using stride, cadence, bounce, arm swing and foot lift. The generator expects the named upright humanoid limb structure. It adds editable keys and rejects unreachable settings. **Author root travel** deliberately generates movement as animation data.
- **Clean active loop** adjusts the seam over a chosen range, optionally matching velocity. Inspect the result and undo if it changes your intended motion.
- Add animation layers from another clip. Set weight, offset and additive mode, and mask selected bones. An empty mask affects all bones. Source clips are sampled as authored tracks; live constraints run after blending.
- Choose another clip to preview a weighted transition. **Create transition clip** uses the displayed playback endpoints, including direction and controllers. It creates baked FK motion and holds the source appearance; it does not invent hit windows or gameplay events.
- **Mirror entire clip** creates a new clip from the evaluated motion, swaps linked sides, and copies/remaps clip metadata. Shared artwork remains shared; source texture pixels are not flipped. The original clip remains available.
- Add springs to selected non-root bones for secondary motion. Stiffness, damping, gravity and wind control the result. Evaluation is deterministic for scrubbing and export. Bake to obtain ordinary editable keys.

Trim, split and resize in Studio retain editable sparse keys and their original interpolation, along with live layers, contacts and spring history. These operations do not mark a clip **baked to FK**. Extending a clip holds its prior motion endpoint until a key is added or edited later; this resumes the whole clip's retained motion through that frame. Audio/reference time continues during held motion. Use **Bake all clips and switch to FK** when ordinary FK keys are the intended result. Existing deliberately baked clips keep their baked status. See [editable clip windows](RELIABILITY.md#editable-clip-windows).

## Props, meshes, outfits and faces

**Props:** import an image through the artwork controls, select its bone, then attach it. Set local offset, rotation, scale, pivot and drawing depth. **Key prop state here** records parent, transform, sprite and visibility in the active clip. To transfer an item between hands, choose the destination Parent, then **Switch to parent without a jump**. Choosing the parent alone does not modify the live prop. The switch preserves its displayed world placement at that frame.

**Meshes:** import/select artwork, choose a grid, origin and size, and create a weighted mesh. Nearby bones receive automatic weights. Select a vertex in the preview or by index, edit its rest position/UV/weights, or bind it to selected bones. Binding snapshots the rig's rest transforms, so later rest edits move the artwork with the rig. Enable deformation editing to key vertex offsets at the playhead. Other vertices retain their interpolated offsets. Rendering uses nearest texture samples and supports transparent textures. Grid creation supports up to 12 by 12 cells; this is a planar mesh editor, not a 3D model importer.

**Outfits:** capture current colors and enabled props/meshes, name the outfit, then edit included equipment and part colors. Choose an outfit to apply it without replacing the rig or its animation. Outfit selection is project-wide rather than an animation track.

**Face:** attach a facial sprite track to the head or another bone. Choose an expression/mouth asset and key it at the playhead. For a blink, choose the closed-eye sprite and use **Add blink with chosen sprite**; the preceding sprite returns after two frames. Offset, angle, scale and depth position each face track. Mouth animation is manually keyed; automatic speech recognition or lip sync is not included.

## Scenes, sound and events

**Scene:** import another `.rebone` project as an actor or duplicate the current character. Each actor has its own clip, speed, frame offset, depth, visibility and placement. Move Rig can target the main character or one actor. Actor placement/scale/visibility keys belong to the main timeline's clips. Imported nested actors are excluded; scenes support one level of actors. The main project's canvas crops the final composite.

**Audio & events:** import a **16-bit PCM WAV**, mono or stereo, at 8–192 kHz. Set start offset, volume and enabled state. The waveform supports click-to-scrub; enable audition for short sound previews while scrubbing. Playback advances through frame holds and supports reverse/ping-pong clip directions. Windows playback uses WinMM. Files with compressed WAV codecs, MP3 or other sample encodings must be converted first. Only one timeline soundtrack is active per project. The demo's optional test tone starts disabled.

Add named markers and string payloads at frames. Edit, jump to or remove them in the same tab. Markers are saved and exported as data; the editor does not execute their payloads. Generated engine helpers dispatch marker callbacks during animation playback.

## Pixels and collision regions

**Pixels & boxes:** choose Paint, Erase or Restore and a brush size/color, then select **Pixels (P)** and drag on the main canvas. Corrections affect only the active clip and frame, after the scene has rendered. Erase writes transparency; Restore removes corrections to reveal the generated image. One continuous stroke is one undo operation. Clear frame corrections to remove the entire frame's override.

Choose Hurt, Hit or Interaction and select **Boxes (H)**, then drag a rectangle on the main canvas. Regions may follow a selected bone or remain in canvas space. Set their frame span, offset, size, angle and enabled state; key those fields for animated shapes. The overlay helps inspect placement, but region colors are not painted into the exported sprite. Collision polygons, names and types are included in package metadata.

## Export and engine integration

PNG frames, sprite sheets, GIF and Aseprite exports include props, meshes, faces, scene actors and pixel corrections. Projects without added artwork retain editable group layers in Aseprite; projects with artwork use a composited scene layer with clip tags/timing. Procedural motion is evaluated during export.

Production → **Game export**, or **Game engine package** in the main Export window, writes every clip's transparent PNG sheet (or, with **Pack frames into atlas pages**, shared trimmed and packed `atlas-NN.png` pages; the layout is remembered between sessions), `package.json`, optional PCM WAV, a README and the selected engine helper. Set the pixel pivot and an output directory. Overwrite must be selected explicitly if package files already exist. Duplicate/empty display names receive unique nonempty animation identifiers in the manifest.

Game packages use the same staged publication, rollback and recovery behavior as Studio batch exports. A render failure identifies the affected scene component and stops the export. Filesystem failures retain recovery data when rollback cannot finish. [Export recovery details](RELIABILITY.md#recoverable-exports) describe filesystem requirements and limits. Per-frame `source_time_ms` records the retained source clock for consumers that synchronize audio after trimming or splitting.

- **Godot:** copy the package into a Godot 4 project and attach `ReBonePlayer.gd` to an `AnimatedSprite2D`. Set its manifest path. It creates SpriteFrames, fires marker signals in frame order, keeps one reused collision polygon per region, and plays the reference audio in sync. It offers `play_clip`, `play_direction`, `pause_playback`, `resume_playback` and `restart`. Verified in Godot 4.7.1 ([engine verification](ENGINE_VERIFICATION.md)).
- **Unity:** copy `ReBonePlayer.cs`, `Editor/ReBoneImporter.cs` and the package into a Unity project (one copy of the scripts serves every package). Choose **Assets → ReBoneBase → Create Actor** on `package.json`; running it again updates the same actor. It creates sprites with the exported pivot, fires markers in frame order, reuses one trigger collider per region, plays the reference audio in sync, and offers `Play`, `Pause`, `Resume`, `Restart` and `Step`. Verified in Unity 6000.4.6f1 ([engine verification](ENGINE_VERIFICATION.md)).
- **Unreal:** run the shipped `ReBoneImport.py` in a project with the shipped `ReBoneBase` plugin to create Paper2D textures, sprites, flipbooks and a `ReBoneAnimationSet`; play it with `ReBoneFlipbookComponent`. Import, reimport, markers, regions, looping and directions were verified in Unreal Engine 5.8.3. See the [Unreal guide](UNREAL_GUIDE.md).
- **Generic:** consume `package.json` directly. It uses the shared, versioned sprite format described in [EXPORT_FORMAT.md](EXPORT_FORMAT.md): pages, clip and direction identifiers, frame rectangles with trim offsets, durations, retained source time, events, regions, pivot and audio timing. This is baked sprite output; it does not export an editable engine skeleton or weighted runtime mesh.

## Workspace and shortcuts

The main **Workspace** menu provides Rigging, Animation and Scene layouts, panel visibility/recovery, saved layouts and direct tool destinations. See the [workspace guide](WORKSPACE_GUIDE.md).

**Production → Workspace:** choose Left, Right, Bottom, Floating or Hidden for the skeleton, inspector, timeline and Production panels. Resize dividers/windows, then save the layout under a name. Load saved layouts or reset to defaults. Layouts and shortcut preferences are local application state.

Choose an action, enter a key name such as `M`, `Space`, `ArrowRight` or `F3`, set modifiers and apply. Duplicate chords are rejected. Restore defaults if needed. **Commands / Ctrl+P** opens a searchable command palette; Up/Down selects a result, Enter executes the highlighted result, and Escape closes it. Shortcuts respect active text input.

## Files and limits

Continue using `.rebone` projects; the Production build reads prior files. Save new feature projects with this build: older executables do not understand the new feature data and may drop it when saving. Previous executables are preserved separately.

Inputs and rendering have explicit budgets. There are at most 32 clips and 240 frames per clip; artwork has a 16-megapixel aggregate budget; audio is bounded to 32 MiB PCM; embedded image/audio data across a scene is bounded to 64 MiB. Project files are limited to 96 MiB. Mesh/controller/actor counts and raster work are bounded too. A rejected operation reports its reason and keeps the prior document. Large projects may need smaller canvases or fewer actors for export.

Video reference decoding still uses an existing FFmpeg installation. No Aseprite, browser, Node.js, Lua or network service is needed to run the standalone application. Original license restrictions remain in `BONEBASE_LICENSE.txt`; this is a local personal-use port.

The [Characters guide](CHARACTERS_GUIDE.md) describes the newer automatic canvas growth, permanent attachment rules, animation templates and native GLB/glTF/FBX/OBJ character imports.
