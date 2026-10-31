# 20 implemented Studio improvements

All 20 requested improvements are implemented in ReBoneBase Studio. Open **Studio** in the app; motion-import controls are under **File → Import 3D motion**. See [the Studio guide](STUDIO_GUIDE.md) for use and limits.

1. **More precise bone picking.** Highlight the exact joint/body under the cursor, cycle overlapping bones, and allow individual bones to be locked.
2. **Persistent hand and foot pins.** Keep planted feet and held objects fixed while moving the torso; add ground-contact controls.
3. **Joint constraints.** Set rotation limits, preferred bend directions, and optional bone-length locks to prevent broken-looking poses.
4. **Autosave and recovery.** Save recoverable snapshots automatically, with crash recovery and versioned backups.
5. **Multiple-bone selection.** Add Shift/box selection and coordinated rotation or movement of a selected limb or group.
6. **Direct timeline key editing.** Select, drag, copy, paste, snap, and retime keys or ranges of frames.
7. **Animation curve editor.** Edit channel curves, tangent handles, and exact values beyond the existing easing presets.
8. **Full clip management.** Duplicate, trim, split, reorder, and delete animation clips.
9. **Reparent without jumping.** Reattach, detach, and reconnect bones while preserving their visible world positions.
10. **Retarget preview on the mannequin.** Scrub the actual target rig before accepting imported motion, rather than previewing only its source skeleton.
11. **Reusable retarget profiles.** Save joint mappings, projection, scale, and axis conventions for each motion source.
12. **Multi-animation glTF import.** Select named animations from a file or import several as separate clips.
13. **Root-motion paths and in-place conversion.** Display the mannequin's travel path and convert moving animations into stationary walk/run cycles.
14. **Searchable cross-project libraries.** Browse custom rigs and poses with thumbnails, tags, favorites, and reusable categories.
15. **Expanded onion-skin controls.** Choose previous/next frame counts, tint, opacity, and spacing.
16. **Pixel-perfect outlines.** Add configurable outline color, thickness, and per-group settings.
17. **Shared palettes and indexed export.** Reuse color swatches across groups and export palette-based PNG assets.
18. **More playback timing controls.** Add reverse and ping-pong playback plus per-frame hold durations.
19. **Synchronized motion references.** Import timed image sequences or video reference alongside the animation timeline.
20. **Batch export presets.** Export all clips with consistent naming, atlas JSON metadata, and named Aseprite animation tags.
