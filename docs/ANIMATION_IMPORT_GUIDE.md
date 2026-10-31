# Import Unity and Unreal animations

Run the latest versioned ReBoneBase executable. Existing characters, attached joints, canvas growth and motion templates remain available.

## Choose a destination

- **2D rig / sprite artwork:** File → **Import animation for 2D rig…**. Select a source clip, choose Front / Side / ThreeQuarter projection, check the joint map and preview your character. Select one or several clips, then import. Save a retarget profile for repeated sources. Existing rigged meshes and attached artwork follow those bones. An unrigged PNG still needs a skeleton and mesh weights or attached parts; importing skeletal motion cannot animate individual painted limbs automatically.
- **3D model:** Production → **3D characters**, choose your character, then **Import animation for selected 3D character…**. Map its joints to source joints, preview, select clips and import. The existing model and materials stay intact. A static OBJ without a meaningful joint hierarchy/skin cannot become an articulated character from motion alone; use a rigged GLB/glTF or FBX character.
- **Both together:** In the 3D import dialog, enable **Also animate my 2D rig in the same clips**. This uses automatic 2D joint matching and the chosen projection. Use the separate 2D dialog when custom 2D mapping is needed.

Imported 3D motions are embedded in the project. Each main timeline clip stores its own model animation, speed, looping and time offset. Selecting a different timeline restores that clip's playback settings. Trim/split preserve the displayed time, including authored frame durations; speed edits keep a trimmed clip's start pose, and duplicate/reorder/delete keep the correct motion assignments. Use **Create timeline for 3D animation** for clips already included in a model. Save/load and Undo include the imported animation data.

Limbs keep the target skeleton's rest positions and proportions. 3D retargeting transfers rest-relative rotations. 3D root travel is off by default and can be enabled explicitly; the simultaneous 2D import retains the existing planar root-motion behavior; the Move Rig tool still controls manual whole-character placement. If source and target use different bone naming, inspect and correct the automatic mapping. Additional twist/finger/accessory joints may need manual mapping. Check the preview before importing a large batch.

## File formats

| Source | Direct import | When an engine export is needed |
| --- | --- | --- |
| FBX | Skeletal animation, including files with no mesh; multiple animation stacks | Bake engine constraints, procedural graphs or Control Rig into a skeleton clip first |
| GLB / glTF | Named skeletal clips, including animation-only files | Export the evaluated skeleton animation |
| BVH | One skeleton motion per file | Use an external/engine exporter if your asset is not already BVH |
| Unity `.anim` | Text YAML transform rotation, Euler and position curves | Binary, compressed or Humanoid muscle-only clips require the original Unity avatar/character and a baked FBX export |
| Unreal `.uasset` | Import selection displays an actionable export message | Open the owning Unreal project and export an Animation Sequence as FBX; standalone asset binaries do not contain a portable complete skeleton |

Unity `.anim` files do not include the character's bind pose. Direct text import uses the first available sampled pose as its reference, with a warning; exporting the character skeleton/rest pose and animation together as FBX gives more reliable retargeting. Non-skeletal channels (material/sprite swaps, blendshapes and engine events) are not skeletal motion and are not transferred. Recreate events in the app's Audio & events tools.

## Unity export

Use the original Unity project and matching character/avatar. Install Unity's **FBX Exporter** package if needed. Bake the desired animation onto that character, select the animated object, and use **GameObject → Export To FBX**. Choose **Animation Only** or **Model(s) + Animation** in the export options. Import the FBX into ReBoneBase using either destination above. Use the original source FBX directly when it already contains the required animation.

Unity official references: [FBX export options](https://github.com/Unity-Technologies/com.unity.formats.fbx/blob/main/com.unity.formats.fbx/Documentation~/ref-export-options.md), [baking animation for export](https://learn.unity.com/tutorial/baking-animation-for-fbx-export). An Animator Controller/state machine is not a clip: export the evaluated motion you need.

## Unreal export

In the owning Unreal project, select the **Animation Sequence** in the Content Browser and use **Asset Actions → Export…** to write FBX. Bake Animation Blueprints, Sequencer or Control Rig results to an Animation Sequence before export. Keep the skeleton hierarchy and include the bind/reference pose where the export options permit it.

For a batch, enable the **Python Editor Script Plugin**, select Animation Sequence assets, and run the included `integrations/unreal/export_animations.py` through Unreal's **Tools → Execute Python Script**. It exports to the project's `Saved/ReBoneBaseExports`, keeps existing FBX files by adding suffixes, and leaves source assets unchanged. Other asset types are skipped. This helper has local mocked API checks; it has not been executed in your Unreal installation.

Epic official references: [FBX animation pipeline](https://dev.epicgames.com/documentation/en-us/unreal-engine/fbx-animation-pipeline-in-unreal-engine), [Animation Sequence FBX exporter](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/AnimSequenceExporterFBX?application_version=4.27).

## Limits and portability

Motion sources are limited to 32 MiB, 512 source joints and 240 sampled frames per clip. Oversized/malformed input is rejected before modifying the project. The model/project animation and rendering budgets also apply; select fewer clips or reduce source complexity when a limit is reported. Files imported into a project are embedded, so reopening the saved `.rebone` does not require the original FBX/BVH/glTF/Unity file. Keep external buffers alongside `.gltf` files during the initial import.

`MotionImportDemo.rebone` demonstrates one BVH sway driving a 2D humanoid and a small skinned 3D checker sample together. `SharedSway.bvh` is its editable source motion.
