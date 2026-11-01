# Unreal Paper2D import

Choose **Unreal** as the engine in **Export → Game engine package** (or a directional set). The package contains the sprite pages, `package.json` ([EXPORT_FORMAT.md](EXPORT_FORMAT.md)), the reference WAV when there is one, `ReBoneImport.py` and the `Plugin/ReBoneBase` source plugin.

**Tested with Unreal Engine 5.8.3** (Windows, Visual Studio 2022 17.14), Paper2D, Python Editor Script Plugin and Editor Scripting Utilities.

## Set up a project once

1. Copy `Plugin/ReBoneBase` into your project's `Plugins` folder.
2. Open the project. Unreal offers to build the plugin; accept. A Blueprint-only project needs Visual Studio 2022 for this one build. You can also build it ahead of time with `RunUAT BuildPlugin -Plugin=<path>/ReBoneBase.uplugin -Package=<output> -TargetPlatforms=Win64` and copy the output into `Plugins`.
3. The plugin enables Paper2D, Python Editor Script Plugin and Editor Scripting Utilities.

## Import, and import again

Run `ReBoneImport.py` from the exported folder: **Tools → Execute Python Script…** in the editor, or headless:

```
UnrealEditor-Cmd.exe <Project>.uproject -run=pythonscript -script="<package>/ReBoneImport.py [/Game/Destination]"
```

Use forward slashes in that path; Unreal treats backslashes as escapes when a script has arguments.

The importer writes to `/Game/ReBoneBase/<export folder name>`, or to the destination given after the script path. Packages exported to different folders, such as a character's clips and its directional set, therefore never replace each other. Asset names come from the rig name:

| Asset | Contents |
| --- | --- |
| `Textures/T_<name>_NN` | Each page, nearest filtering, no mipmaps, uncompressed (UserInterface2D), Pixels2D group, never streamed. |
| `Sprites/S_<name>_NNNN` | One sprite per distinct page region and trim. Custom pivot at the exported pivot; sprite collision off. |
| `Flipbooks/FB_<animation>` | One flipbook per exported animation, one key frame per exported frame. |
| `DA_<name>` | `ReBoneAnimationSet`: clips with flipbook, looping, direction, per-frame markers, regions and holds, plus pivot and audio. |
| `Audio/A_<name>` | The reference WAV, when the package has one. |

Every generated asset carries the metadata tag `ReBoneBaseGenerated` with its destination folder. Importing again updates the same assets in place. Generated assets of that destination the new package no longer needs are deleted; anything without the tag, or generated for another destination, is never touched. An asset still referenced elsewhere is kept and reported in the log.

Identical images share one sprite, but every exported frame keeps its own key frame and its own marker and region data.

## Playing

Add a **ReBoneFlipbookComponent** (a `PaperFlipbookComponent`) to an actor and set **Animation Set**. On BeginPlay it plays **Initial Animation**, or the first clip.

- `PlayAnimation(Animation)` starts an animation from frame 0. `Restart()` returns to frame 0.
- `PlayDirection(Name, Direction)` plays a directional set's clip (`e`, `se`, `s`, …). Changing only the direction keeps the current frame and time and does not re-fire markers.
- `OnMarker(Name, Payload, Frame)` fires for every marker of every frame entered, in order. Frames passed within one long tick still fire, in order.
- Collision regions use a pool of reused `BoxComponent`s (profile `OverlapAllDynamic` by default). They are tagged with the region kind and name and moved as frames change, never recreated per frame. Unused boxes have collision disabled.
- `PausePlayback()` and `ResumePlayback()` pause and continue the animation together with its audio. Play rate uses the standard flipbook functions.
- Audio: the data asset's reference sound plays through an `AudioComponent` at the frame's retained source time plus the time spent in the frame, minus the track start; it is silent before the track starts. The sound runs freely while frames follow each other on the source timeline and is started or sought again only where the timeline jumps: play, restart, resume, a loop wrap, or a frame whose source time does not continue from the previous one (as in ping-pong clips).

Sprites lie in the X–Z plane; one pixel is one Unreal unit (`PixelsPerUnit` in the data asset). Regions are converted from the package's y-down pixels: X right and Z up from the pivot.

## Timing and limitations

- **Holds are exact to the millisecond.** Each flipbook runs at `1000 / gcd(holds)` frames per second, and each key frame holds a whole number of those ticks. For example, holds of 100 ms and 370 ms become 100 fps with runs of 10 and 37. Holds with no common factor give 1000 fps; this costs nothing at runtime.
- Paper2D flipbooks do not store looping. The data asset stores each clip's looping, and `ReBoneFlipbookComponent` applies it. A plain `PaperFlipbookComponent` shows the art and holds, but not markers, regions or per-clip looping.
- Reverse and ping-pong playback are already expanded into frame order by the export. Playing with a negative rate shows the right frames but does not fire markers.
- When several frames pass in one tick, every marker fires in order, but only the last frame's regions are applied that tick.
- Unreal has no project-independent equivalent of the Godot and Unity helper scripts, so the runtime is a C++ plugin. It must be compiled once per engine version.

## Verification

With UE 5.8.3, two packages were imported into a new project: the production demo (2 clips, 36 frames, packed and trimmed page, markers, regions, audio) and the 3D demo as an eight-direction set (192 frames).

- **Assets:** `integrations/unreal/verify_import.py` compared every sprite's region, texture-space pivot, page and texture settings, and every flipbook hold, looping value, direction, marker and region count with `package.json`. Both packages passed.
- **Runtime:** the plugin's automation tests (`Automation RunTests ReBoneBase`) passed. They play every imported clip in 1/240 s steps and check:
  - the shown frame against the exported holds;
  - markers in order, including one long tick and restart;
  - each region box's corners against the exported corners;
  - that region boxes are reused;
  - looping;
  - that a direction switch keeps frame and time.
- **Reimport:** after removing a clip, changing a hold to 250 ms and turning off looping, the reimport updated the remaining assets, removed 25 stale generated assets, and both checks passed again.
