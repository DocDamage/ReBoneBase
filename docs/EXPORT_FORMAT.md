# Sprite export format

Batch exports (`atlas.json`) and game engine packages (`package.json`) write the same JSON document. It is identified by `"format": "rebonebase-sprites"` and versioned by `format_version`. This version is independent of the native `.rebone` project schema: a project-format change does not change the export format, and the reverse.

The reference reader is `rebonebase::sprite_package::parse`. It accepts every version listed below and upgrades it to the current shape. Documents with a newer `format_version` are rejected rather than partly read.

| Version | Written by | Identified by |
| --- | --- | --- |
| 0 | Batch `atlas.json` before version 2 | `"app": "ReBoneBase"`, no version field |
| 1 | Engine `package.json` before version 2 | `"version": 1` |
| 2 | Both exporters | `"format": "rebonebase-sprites", "format_version": 2` |

The generated Godot and Unity helpers read versions 1 and 2.

## Coordinates

Pixels, top-left origin, y down; angles are degrees clockwise. A frame's **source space** is the full, untrimmed canvas of one rendered frame (`frame_size`). The pivot and every collision region are in source space, so they remain valid however the frame's pixels are trimmed or placed on a page.

## Document

| Field | Meaning |
| --- | --- |
| `generator`, `name` | Producing tool and rig name. |
| `frame_size` `{w,h}` | Untrimmed frame size shared by the export. |
| `pivot` `{x,y}` | Anchor in source-space pixels. Identical for every frame and clip. (Version 1 stored it normalized.) |
| `canvas_origin` | Canvas origin used to place the rig, for tools that relate exports to world space. |
| `pages[]` | Image files: `file`, `kind` (`atlas` or `animation`), `w`, `h`. |
| `clips[]` | Exported animations, below. |
| `audio` | Optional `file`, `start_ms`, `volume`, `enabled`. |

`atlas` pages are still images holding frame rectangles. `animation` pages are GIF or Aseprite files; their frames carry no `rect` and follow the file's own frame order.

## Clips

| Field | Meaning |
| --- | --- |
| `id` | Stable identifier for the export, such as `clip-01`. |
| `animation` | Unique, nonempty engine animation name. Duplicate or empty clip names receive suffixes such as `Walk (2)`. |
| `name` | Authored clip name. |
| `direction` | `null`, or `{id, angle}` for directional sets (angle in degrees clockwise from screen right). |
| `fps`, `looped`, `playback` | Authored clip settings. Reverse and ping-pong are already expanded into frame order. |
| `frames[]` | Ordered frames, below. |

## Frames

| Field | Meaning |
| --- | --- |
| `page` | Index into `pages`. |
| `rect` `{x,y,w,h}` | Pixels inside the page, or `null` on an animation page. |
| `source_size` `{w,h}` | Untrimmed frame size. |
| `trim` `{x,y}` | Where `rect`'s top-left sits inside the untrimmed frame. |
| `source_frame` | Authored frame number this entry was rendered from. |
| `source_time_ms` | Retained source time, for aligning audio and references after trimming or splitting clips. |
| `duration_ms` | How long this frame is held. |
| `events[]` | Markers on this frame, in authored order: `clip`, `frame`, `name`, `payload`. |
| `boxes[]` | Collision regions in source space: `id`, `name`, `kind`, `center`, `size`, `angle`, `corners`. |

To draw a frame, copy `rect` from its page to `trim` inside a transparent `source_size` canvas, then place that canvas so `pivot` lands on the actor's position.

Several frames may point to the same page pixels. Each frame entry still carries its own timing, events and regions; consumers must never merge entries because their images match.
