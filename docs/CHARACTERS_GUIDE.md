# Canvas, attached joints, 3D characters and animation templates

Run the latest versioned ReBoneBase executable. This release includes the earlier Studio and Production tools.

## A canvas that follows the character

The canvas grows automatically when the character reaches its edges. It grows left/up as well as right/down, and includes skin radii, bulges, outlines, attached artwork, meshes, scene actors and 3D characters. Growth changes the image bounds, leaving the skeleton's world positions and existing pixel corrections in place. The camera stays steady during a drag.

After an edit, the canvas also considers the other animation frames so a later pose is not cropped in an export. Sprite sheets, GIF, Aseprite and game packages use one common envelope across clips. Collision metadata is converted into the exported image's coordinates.

Use **Fit (F)** to see the enlarged canvas. The canvas does not automatically shrink. Growth preserves the export pivot's world anchor; an explicit crop that excludes it clamps the pivot to the crop edge. In Inspector → **Canvas & symmetry**, turn off **Grow canvas with character** to use a fixed crop and edit its dimensions. The maximum canvas is 4096 by 4096; rendering/export budgets may impose a smaller practical limit for complex scenes. Rejected growth reports the reason.

## Permanently attached extremities

Dragging an arm, head or other limb now rotates it around its existing attachment point. It cannot be moved onto a different part of the body. This also applies to Rig mode, mirrored edits and multi-selection. Drag a bone's terminal endpoint to change its length; its head remains anchored.

Connected joints remain at their parent's tail. Loose joints such as shoulders retain their original local offsets on the parent; they are not collapsed into the center of the torso. Existing non-root translation keys are preserved in the file but no longer detach joints during playback or export. Studio's detach/rest-translation controls and the inspector's head-position fields cannot relocate attached limbs. An unattached root created with the Bone tool may still be connected to start a new chain.

**Move Rig (M)** remains the tool for moving the whole character. Middle/right dragging pans the view.

## Animation templates

Click **Animation templates…** beside the dope sheet, or open Production → **Motion** / Studio → **Animation**.

Choose **Idle, Walk, Run, Jump, Wave, Punch, Crouch, Sit, Celebrate** or **Hit reaction**, set Strength, then click **Add animation template**. Each adds a named clip with ordinary editable keys. It keeps the character and existing clips intact and supports undo. Repeated additions receive distinct names. These humanoid templates expect named chest/arm/leg bones; Humanoid is a suitable starting rig. Walk/run settings remain available for more detailed gait generation.

The supplied extension contains 20 rig templates but no bundled reusable motion library. These ten animation templates are newly authored for this standalone app. `AnimationTemplates.rebone` contains examples.

## Adding a 3D character

Use **File → Import 3D character…** or Production → **3D characters**. This imports the model itself; **Import 3D motion** remains the separate tool for retargeting motion onto the 2D mannequin.

Supported input families are **GLB/glTF, FBX and OBJ**. Imports run in the background, then add an embedded character to the project. Keep sidecar buffers, textures and materials inside the selected model's directory. Remote resources and paths escaping that directory are rejected. Save the resulting `.rebone` project to keep the imported model and textures with it.

The first imported 3D character hides the 2D mannequin. Use **Show 2D mannequin in the scene** to show both. Choose a character, adjust its pixel scale, depth, camera yaw and camera pitch, and use **Move Rig** to place it. The 3D panel selects the active Move Rig target. The model renders into the same pixel canvas and participates in PNG, GIF, Aseprite and game-package exports.

Choose an included animation or **Rest pose** for the active timeline clip. Play/scrub the main timeline to preview it; loop and speed control the imported motion. **Create timeline for 3D animation** creates a matching timeline, within the application's 240-frame limit. To pose a rigged model, select one of its nodes under **Pose a joint**.

### Correcting a pose at chosen frames

**Keyed correction** rotates the selected joint at the playhead's frame, on top of the imported motion. With **Auto-key** on, editing X/Y/Z creates or updates a key at that frame; with it off, only frames that already have a key (◆) can be edited, and **Set key** keys the current value. **Delete key**, **◀ Key** / **Key ▶** and **Clear joint keys** act on the selected joint. Between keys the correction blends along the shorter rotation, so 170° → -170° turns through 180° rather than back through 0°; before the first and after the last key it holds. A correction of more than 180° between two adjacent keys therefore needs an intermediate key.

Keys belong to the selected character and the active timeline clip. The Timeline shows them as blue rows named **3D · joint** for the selected character: click or Shift-click keys to select them, drag to move them, and use **Keys → Copy / Paste / Delete keys** and **Apply easing** as for 2D keys. Trim, split, duplicate, reorder and delete keep each correction on the same visible frame. Each edit is one undo step, and keys are saved in the project.

**Whole-clip offset** is the earlier static override: it rotates the joint on every frame of every clip, and keyed corrections apply after it. Only rotation is corrected, so joint positions stay attached to their parents. The canvas, every ordinary export and four-/eight-direction exports render the same corrected pose. Projects saved before keyed corrections open with no keys and look unchanged.

### Cleaning up imported motion

**Clean up motion** (Production → 3D characters, below the joint controls) fixes part of an imported animation without replacing it. Choose frames of the active clip (**Whole clip** selects all), choose a tool, then **Preview cleanup**. The preview shows a before and after image at the playhead and a measurement; **Apply cleanup** writes the result as ordinary correction keys in one undo step. The imported motion and clip timing never change, and frames and joints outside the chosen range keep exactly their previous pose, including earlier corrections. To revert, undo or delete the keys.

- **Reduce jitter** smooths the selected joint (or all joints) with a Gaussian window. **Radius** is in frames; **Strength** blends from the original (0) to fully smoothed (1). It reports mean rotational jitter before and after.
- **Plant foot** treats the selected joint as a foot and keeps it where it lands at the first frame, bending its parent and grandparent (thigh and shin) to hold it. **Blend in/out** ease the correction over extra frames before and after the contact. **Align to floor** plants it at a chosen height; **Detect floor** uses the joint's lowest height in the clip, which is usually full leg extension, so set the floor slightly higher if the leg cannot reach it. **Detect contact** finds the next frames where the foot is low and nearly still. The foot keeps its own orientation. When the body moves the foot out of reach, the leg gets as close as it can and the preview states the remaining distance. It reports total foot slide during the contact.
- **Reduce keys** removes correction keys in the range while every frame stays within **Pose tolerance °** of the current pose; frames outside the range do not move at all. Use it after smoothing or planting, which key every frame.

Corrections are rotation-only, so cleanup keeps bone lengths and attachments, and the canvas, every export and direction exports show the same result.

Rendering uses an orthographic camera, nearest texture sampling, material colors and depth testing. Materials use base color and alpha; metallic/roughness and normal-map lighting are omitted. Skinning uses four vertex influences, with warnings when additional influences are omitted. Transparent overlapping surfaces have the usual limitations of triangle sorting. OBJ generally supplies static geometry/materials; a file must contain a skeleton/animation to offer those editing options. Unsupported material/deformation features are reported by the importer. This is a character-to-pixel workflow, not a general 3D modeling package.

Use the supplied `CharactersDemo.rebone` or import `Mannequin.obj` together with its `.mtl` for a local example. Import budgets and the canvas-dependent rendering budget prevent excessively heavy scenes; reduce the canvas resolution or simplify a model when an import reports a work limit.

## Four- and eight-direction export

Choose **Export → Directional set · 4 or 8 views**. Pick 4 or 8 directions and a starting angle (0° puts the first direction at screen right; 45° gives a diagonal four-direction set).

- **3D camera views** render the chosen clips from every direction by rotating each enabled 3D character's camera yaw. Set **Characters currently face** to the direction the character faces at its present yaw (normally s, toward the viewer). Camera pitch is kept. 2D artwork is not rotated.
- **2D clips per direction** assign an authored clip to each direction, or **Mirror of** another direction. A flat drawing has no unseen side or back, so nothing is invented. Every assigned clip must have the same frame count, holds and looping. Mirroring centers the canvas on the pivot (snapped to the nearest half pixel) so the flipped frames keep their anchor.

**Preview orientations** renders the current frame from each direction. Every direction uses one canvas envelope and pivot, so switching direction in a game never moves the character. The export uses the selected engine and atlas options, runs in the background with progress and Cancel, and never changes the project. The Godot helper adds `play_direction(name, direction)` and Unity's player adds `Play(name, direction)`. Switching only the direction keeps the current frame and timing.

## Import separate animations

The latest build also imports external FBX, BVH, GLB/glTF and text Unity .anim skeletal clips onto existing 3D characters or 2D rigs. The [Animation import guide](ANIMATION_IMPORT_GUIDE.md) covers joint mapping, simultaneous 2D/3D clips and Unity/Unreal export steps. Unreal .uasset and binary/Humanoid-only Unity clips require baking/export in the owning engine project.
