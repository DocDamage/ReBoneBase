# Roadmap

## 0.4.0 release follow-ups

- [ ] Move the older import workers onto the shared job system in `src/jobs.rs`: 3D character loading (`src/app/characters_ui.rs`), 3D motion import (`src/app/animation_import_ui.rs`) and 2D motion import (`src/app/import_ui.rs`).
- [ ] Visually check export progress and **Cancel** during an export slow enough to observe.

## Next milestone: correct imported animation inside ReBoneBase

Goal: import an animation, correct its poses and deformation, and export the finished result without returning to another animation tool.

1. **Editable 3D joint keyframes.** Per-character, per-clip joint rotation tracks layered additively over imported animation; key create/delete/copy/move in the Timeline; wraparound-safe interpolation; key and auto-key controls in the Inspector; preserved through save/load, undo, trim, split, duplicate and reorder. Limbs stay attached to their parents. Done when corrections scrub smoothly and match in the canvas, ordinary exports and directional exports, and existing projects look unchanged.
2. **Reusable 3D retarget-calibration profiles.** Saved source-to-target joint mappings, reference-pose calibration, facing and axis correction, per-joint offsets, explicit root-travel and root-scale policies, and reports for missing, ambiguous or unmapped joints, applied identically to preview and every clip in a batch. Target limb lengths and attachment positions stay intact.
3. **Editable 3D motion cleanup**, built on the correction tracks. Jitter reduction for selected joints and ranges, foot planting and floor alignment, contact corrections with blend-in/out, before/after preview, and key reduction with a pose-error tolerance, all stored as a reversible correction layer.
4. **Weight painting and explicit rebinding for 2D artwork meshes.** Influence heat maps; brush radius, strength and falloff; add, subtract and smooth; locked influences; multi-vertex bulk assignment; normalization and influence limits; and **Rebind to current rest pose** with a preview. One stroke is one undo step. Imported 3D skin weights are a later scope.
5. **Layered Aseprite export for production projects.** Independent layers for skin groups, props, weighted meshes, facial tracks, scene actors and rendered 3D characters, with clip tags, timing, transparency and animated draw order preserved. Any flattened component is reported. Flattening the file reproduces the native frames. Round-trip import follows later.
