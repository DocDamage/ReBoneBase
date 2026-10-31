# ReBoneBase 0.3.0 workspace guide

This release brings everyday rig and animation work into the main window. Existing Studio and Production tools remain available, and projects still use schema 2.

## Choose a workspace

Open **Workspace** in the main toolbar. The built-in **Rigging**, **Animation**, and **Scene** layouts arrange panels for each task. They change the workspace without changing the project or adding an undo entry.

Use **Workspace → Panels** to reopen Skeleton, Inspector, Timeline, or Production after hiding them. Choose **Recover hidden and floating panels** when a panel is difficult to reach, or **Reset panel layout** to restore the default arrangement. Saved layouts and detailed docking controls remain in **Production → Workspace**.

The menu also links directly to common Studio and Production sections. **Studio** and **Production** in the toolbar continue to open their existing windows. Current layout preferences are saved when the application exits normally; use **Save current layout** to keep a named layout. Saved layouts remember whether Production is open.

## Find and select bones

The **Skeleton** panel follows parent relationships even when the file stores bones in a different order.

- Type part of a bone name in **Find a bone**. Search ignores capitalization and keeps matching bones' ancestors visible for context.
- Expand or collapse branches with the arrow beside a parent bone. Searching temporarily exposes matching branches without discarding their collapsed state.
- Use **Reveal selected** to clear the search, expand the selected bone's ancestors, and scroll to it. **Expand all** opens the entire hierarchy.
- Click a bone to select it. Shift-click adds or removes a bone from the selection. Locked bones retain an `[L]` indicator.
- **Starter rigs…** opens the original template list; **+ Draw bone** activates the existing Bone tool.

Changing search, branch expansion, or selection does not edit the document.

## Use the contextual inspector

The Inspector has three sections:

| Section | Controls |
| --- | --- |
| **Selection** | The selected bone's rest geometry or animated pose, a lock toggle, and links to Rig & pins and animation tools. Pixel and collision tools show their relevant controls here. |
| **Appearance** | Group names, colors, and draw order, with direct links to skin/shading, outfits, and artwork. |
| **Project** | Project name, canvas size and automatic growth, symmetry, saved poses, reference images, and recovery tools. |

In **Rig** mode, Selection edits length, angle, radii, group, and existing skin bulges. In **Animate** mode, it displays the current frame and makes auto-key behavior explicit: changing rotation or scale writes a key at that frame. **Key current pose** and **Reset pose here** remain available.

When several bones are selected, the inspector identifies the primary bone it is editing and links to the existing group-transform tools. Locking a bone protects it from editing; unlock it in the inspector or Rig & pins when needed.

Select **Pixels** on the canvas toolbar to access correction color, brush size, Paint, Erase ink, and Restore generated modes in the inspector. Select **Boxes** for collision overlay, attachment, and region-kind controls. The complete correction and collision editors remain in **Pixels & boxes…**.

## Navigate time reliably

The Timeline exposes playback, start/end navigation, a numeric frame field, and previous/next key controls. Seeking stops playback and active audio, updates the playback clock, and keeps the selected frame instead of allowing the next playback tick to replace it. If audio audition is enabled, scrubbing still plays a short sample at the new frame.

| Command | Default shortcut |
| --- | --- |
| Play / pause | Space |
| Previous / next frame | Left / Right |
| Start / end of clip | Home / End |
| Previous / next keyed frame | Shift+Left / Shift+Right |
| Copy / paste selected keys | Ctrl+C / Ctrl+V |
| Command palette | Ctrl+P |

Key jumps use visible keyframes in the active clip. Hidden source keys retained by trimming do not become navigation targets. Clip changes clear stale key and curve gestures and restart transport at the beginning.

Clip name, timing, length, and loop controls are available from **Clip settings**. Existing trim, split, duplicate, playback-direction, frame-hold, and retiming tools remain in Studio's Animation section.

## Work in the dope sheet or curves

Choose **Dope Sheet** or **Curves** in the main Timeline.

### Dope sheet

The track filter is explicit:

- **All** shows the rig's bone tracks.
- **Selected** limits bone tracks to the current selection.
- **Animated** shows tracks with animation keys, including retained source keys that drive interpolation inside a trimmed clip.

The name filter narrows the list further. Group tracks are labeled separately. Click a bone track label to select that bone; Shift-click preserves multi-selection behavior. Changing the track filter does not delete or deselect keys.

Zoom changes frame spacing. **Fit** fits the clip to the available time area, and **Show playhead** brings the current frame into view. Track names remain fixed while you scroll time; the frame ruler remains visible when scrolling tracks. Dense frame labels use wider tick intervals at small zoom levels.

Click diamonds to select keys and Shift-click to extend or toggle the selection. Drag selected keys to move them; one drag remains one undo step. Drag empty time space to scrub. Copy, paste, and delete are available near the tabs, with less frequent key actions in the **Keys** menu.

### Curves

Curves uses the existing curve editor directly in the Timeline. Select a bone, choose its channel, and edit points, exact values, easing, or Cubic tangent handles. Its editing and undo behavior matches Studio's curve editor. You can keep both editors open, and their controls remain independent.

Trimming, splitting, and held-frame authoring retain the behavior documented in [Reliability](RELIABILITY.md). Keys outside a retained source window remain hidden, and edits remain validated before being accepted.

## Find commands with the keyboard

Open **Commands**, type a command name, then use **Up / Down** to choose a result and **Enter** to run it. **Escape** closes the palette. The selected result is highlighted, and an empty search result explains that no command matches.

The palette includes panel, workspace, and Studio/Production destinations as well as editing commands. File-menu shortcut labels and toolbar hints use your current bindings. Remap commands in **Production → Workspace**; shortcuts respect text entry and the palette. Existing custom bindings are preserved during upgrade. New navigation defaults are added only when both their action and key combination are unused.

## Save state and window size

The header identifies the project and explicitly displays **Saved**, **Unsaved changes**, **Not saved yet**, or recovered-project status. Undo and Redo are disabled when unavailable. Long project names and status messages are truncated in place, with the complete text available on hover.

Toolbar and timeline controls wrap at smaller widths. Panels remain resizable; a short or narrow dock may need scrolling or a larger divider position for its full editor. Built-in layouts provide a quick starting point after resizing the window.
