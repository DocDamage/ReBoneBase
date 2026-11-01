use super::*;
use crate::{
    atlas::{self, AtlasOptions},
    jobs::Progress,
    sprite_package::{self, Offset, PageKind, SpritePackage},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum EngineTarget {
    #[default]
    Godot,
    Unity,
    Unreal,
    Generic,
}
pub fn export_package(
    project: &Project,
    folder: &Path,
    engine: EngineTarget,
    overwrite: bool,
) -> Result<Vec<PathBuf>> {
    export_package_with(project, folder, engine, overwrite, &Progress::default())
}
pub fn export_package_with(
    project: &Project,
    folder: &Path,
    engine: EngineTarget,
    overwrite: bool,
    progress: &Progress,
) -> Result<Vec<PathBuf>> {
    let files = plan_package(project, engine, progress)?;
    progress.commit("Writing files")?;
    crate::export_transaction::publish(folder, files, overwrite)
}
/// Render the whole package, one grid sheet per clip, in memory without
/// touching the destination.
pub fn plan_package(
    project: &Project,
    engine: EngineTarget,
    progress: &Progress,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    plan_package_with(project, engine, &AtlasOptions::default(), progress)
}
/// [`plan_package`] with a chosen page layout.
pub fn plan_package_with(
    project: &Project,
    engine: EngineTarget,
    layout: &AtlasOptions,
    progress: &Progress,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    progress.set_stage("Preparing canvas");
    let prepared = Arc::new(crate::project::prepare_export(project)?);
    let names = sprite_package::unique_names(prepared.rig.clips.iter().map(|c| c.name.as_str()));
    let jobs = names
        .into_iter()
        .enumerate()
        .map(|(i, animation)| ClipJob {
            project: prepared.clone(),
            clip: i,
            id: format!("clip-{:02}", i + 1),
            name: prepared.rig.clips[i].name.clone(),
            animation,
            direction: None,
            mirror: false,
        })
        .collect();
    plan_jobs(&prepared, jobs, engine, layout, progress)
}
/// One exported animation: a clip of an already prepared project, optionally
/// mirrored horizontally around the canvas center.
pub struct ClipJob {
    pub project: Arc<Project>,
    pub clip: usize,
    pub id: String,
    /// Authored name recorded with the clip.
    pub name: String,
    pub animation: String,
    pub direction: Option<sprite_package::Direction>,
    pub mirror: bool,
}
/// Render jobs into a package. `project` supplies the shared canvas, pivot and
/// audio; every job's project must use the same canvas.
pub(crate) fn plan_jobs(
    project: &Project,
    jobs: Vec<ClipJob>,
    engine: EngineTarget,
    layout: &AtlasOptions,
    progress: &Progress,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let total_frames: u64 = jobs
        .iter()
        .map(|j| j.project.rig.clips[j.clip].frame_sequence().len() as u64)
        .sum();
    crate::project::project_export_budget(project, total_frames)?;
    let mut files: Vec<(PathBuf, Vec<u8>)> = vec![];
    let mut packer = atlas::Packer::new(*layout)?;
    let mut pending = atlas::Pending::default();
    let mut package = SpritePackage::new(project, "ReBoneBase Production");
    let job_count = jobs.len();
    for (i, job) in jobs.into_iter().enumerate() {
        progress.checkpoint()?;
        let mut p = (*job.project).clone();
        p.rig.active_clip = job.clip;
        ensure!(
            p.rig.width == project.rig.width
                && p.rig.height == project.rig.height
                && p.extras.canvas_origin == project.extras.canvas_origin
                && p.extras.production.pivot == project.extras.production.pivot,
            "Every exported animation must share one canvas and pivot"
        );
        let clip = p.rig.active_clip();
        progress.set_stage(format!(
            "Rendering {} of {job_count} · {}",
            i + 1,
            job.animation
        ));
        let sequence = clip.frame_sequence();
        let mut frames: Vec<_> = if layout.packed {
            pending.add_clip(&mut packer, &p, i, &sequence, job.mirror, progress)?;
            sequence
                .iter()
                .map(|f| atlas::unplaced_frame(&p, *f))
                .collect()
        } else {
            let columns = 8u32.min(sequence.len() as u32).max(1);
            let mut sheet = crate::project::render_sheet(&p, columns, progress)?;
            if job.mirror {
                mirror_cells(&mut sheet, p.rig.width, p.rig.height);
            }
            let file = format!("{}.png", job.id);
            let page = package.add_page(&file, PageKind::Atlas, sheet.width(), sheet.height());
            files.push((PathBuf::from(&file), png(&sheet)?));
            sequence
                .iter()
                .enumerate()
                .map(|(j, source)| {
                    let rect = sprite_package::grid_rect(j, columns, p.rig.width, p.rig.height);
                    sprite_package::frame_in_active(
                        &p,
                        *source,
                        page,
                        Some(rect),
                        Offset::default(),
                    )
                })
                .collect()
        };
        if job.mirror {
            for frame in &mut frames {
                for b in &mut frame.boxes {
                    mirror_region(b, p.rig.width as f32);
                }
            }
        }
        package.clips.push(sprite_package::Clip {
            id: job.id,
            animation: job.animation,
            name: job.name,
            direction: job.direction,
            fps: clip.fps,
            looped: clip.looped,
            playback: clip.playback,
            frames,
        });
    }
    if layout.packed {
        files.extend(pending.place(
            packer,
            &mut package,
            |i| format!("atlas-{:02}.png", i + 1),
            png,
            progress,
        )?);
    }
    if let Some(audio) = project.extras.production.audio.as_ref() {
        files.push((
            PathBuf::from("reference.wav"),
            audio::encode_wav(&audio.samples(), audio.sample_rate, audio.channels),
        ));
        package.audio = Some(sprite_package::Audio {
            file: "reference.wav".into(),
            start_ms: audio.start_ms,
            volume: audio.volume,
            enabled: audio.enabled,
        });
    }
    package.validate()?;
    files.push((PathBuf::from("package.json"), package.to_json()?));
    let instructions = match engine {
        EngineTarget::Godot => {
            files.push((PathBuf::from("ReBonePlayer.gd"), GODOT.as_bytes().to_vec()));
            "Copy this folder into a Godot 4 project. Add an AnimatedSprite2D, attach ReBonePlayer.gd, and set Manifest to this folder's package.json (res:// path). The script creates SpriteFrames, emits marker(name,payload) for every frame entered in order, and moves one reused CollisionPolygon2D per region under an Area2D. It plays the reference WAV in sync (position = frame source time - audio start), following pause_playback(), resume_playback(), restart(), looping and play_direction(name, direction). Artwork is nearest-neighbor. Sprites use the exported pivot. The script also reads version 1 packages."
        }
        EngineTarget::Unity => {
            files.push((PathBuf::from("ReBonePlayer.cs"), UNITY.as_bytes().to_vec()));
            files.push((
                PathBuf::from("Editor/ReBoneImporter.cs"),
                UNITY_EDITOR.as_bytes().to_vec(),
            ));
            "Copy this folder under Assets in Unity; when a project already has ReBonePlayer.cs and Editor/ReBoneImporter.cs from another package, keep only one copy. Select package.json and choose Assets/ReBoneBase/Create Actor. The editor helper imports pages with point filtering, no mipmaps, no compression and no power-of-two rescaling, and creates (or, when importing the same package again, updates) a ReBonePlayer with SpriteRenderer. Set Pixels Per Unit as needed. marker is a UnityEvent(name,payload) invoked for every frame entered, in order. One reused trigger BoxCollider2D per region follows the exported boxes. The reference WAV plays through an AudioSource in sync with each frame (position = frame source time - audio start). Use Play(animation), Play(name, direction), Pause(), Resume(), Restart() and Step(seconds). The player also reads version 1 packages."
        }
        EngineTarget::Unreal => {
            for (path, text) in UNREAL {
                files.push((PathBuf::from(path), text.as_bytes().to_vec()));
            }
            "Unreal Engine 5.8 (tested 5.8.3) with Paper2D. Once per project: copy Plugin/ReBoneBase into the project's Plugins folder and open the project so Unreal builds it (Visual Studio 2022 is required for that build). Then, each time you export: run ReBoneImport.py from this folder with Tools > Execute Python Script, or headless with UnrealEditor-Cmd <Project>.uproject -run=pythonscript -script=\"<this folder>/ReBoneImport.py\". It imports pages as nearest-filtered, uncompressed textures without mipmaps, creates one Paper2D sprite per distinct frame image with the exported pivot, one flipbook per animation and a ReBoneAnimationSet data asset under /Game/ReBoneBase/<this folder name> (or a destination given after the script path). Running it again updates those assets in place and removes generated assets the package no longer needs. Add a ReBoneFlipbookComponent, set its Animation Set, and bind OnMarker(Name, Payload, Frame). It plays clips with their looping, fires markers in order, keeps one reused box component per collision region, and offers PlayAnimation, PlayDirection(Name, Direction) and Restart. Timing: each flipbook runs at 1000/gcd(holds) frames per second with whole runs, so holds are exact to the millisecond. Flipbooks do not store looping; the component applies each clip's setting. A plain PaperFlipbookComponent plays the art and holds but not markers, regions or looping choices. Audio is imported as a SoundWave referenced by the data asset with its start time; scheduling it is left to your game."
        }
        EngineTarget::Generic => {
            "Read package.json (format rebonebase-sprites, format_version 2). pages lists the PNG files. Each clip has a stable id, a unique animation name, an optional direction and ordered frames. A frame names its page and rect inside that page, its untrimmed source_size, and trim: where rect's top-left sits inside the untrimmed frame. pivot (pixels) and every frame's boxes are in untrimmed source-frame space, top-left origin, y down. Reverse/ping-pong playback is already expanded into frame order; duration_ms is each frame's hold. source_time_ms aligns audio and references with retained source timing after trimming or splitting. audio, when present, names the WAV with start_ms and volume."
        }
    };
    files.push((
        PathBuf::from("README.txt"),
        instructions.as_bytes().to_vec(),
    ));
    ensure!(
        files.iter().map(|(_, b)| b.len()).sum::<usize>() <= 128 * 1024 * 1024,
        "Game package exceeds 128 MB"
    );
    Ok(files)
}
/// Flip every `w` x `h` cell of a grid sheet horizontally in place.
fn mirror_cells(sheet: &mut image::RgbaImage, w: u32, h: u32) {
    for y in (0..sheet.height()).step_by(h as usize) {
        for x in (0..sheet.width()).step_by(w as usize) {
            let cell = image::imageops::crop_imm(sheet, x, y, w, h).to_image();
            let flipped = image::imageops::flip_horizontal(&cell);
            image::imageops::replace(sheet, &flipped, x as i64, y as i64);
        }
    }
}
/// Mirror a collision region across the vertical center of a `width` frame.
pub(crate) fn mirror_region(region: &mut CollisionShape, width: f32) {
    region.center.x = width - region.center.x;
    region.angle = -region.angle;
    for corner in &mut region.corners {
        corner.x = width - corner.x;
    }
    // A reflection reverses winding; reversing the corners restores it.
    region.corners.reverse();
}
/// Unreal importer and plugin, shipped verbatim with every Unreal package.
const UNREAL: &[(&str, &str)] = &[
    (
        "ReBoneImport.py",
        include_str!("../../integrations/unreal/ReBoneImport.py"),
    ),
    (
        "Plugin/ReBoneBase/ReBoneBase.uplugin",
        include_str!("../../integrations/unreal/ReBoneBase/ReBoneBase.uplugin"),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBase/ReBoneBase.Build.cs",
        include_str!("../../integrations/unreal/ReBoneBase/Source/ReBoneBase/ReBoneBase.Build.cs"),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBase/Public/ReBoneAnimationSet.h",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBase/Public/ReBoneAnimationSet.h"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBase/Public/ReBoneFlipbookComponent.h",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBase/Public/ReBoneFlipbookComponent.h"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBase/Private/ReBoneBase.cpp",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBase/Private/ReBoneBase.cpp"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBase/Private/ReBoneAnimationSet.cpp",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBase/Private/ReBoneAnimationSet.cpp"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBase/Private/ReBoneFlipbookComponent.cpp",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBase/Private/ReBoneFlipbookComponent.cpp"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBaseEditor/ReBoneBaseEditor.Build.cs",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBaseEditor/ReBoneBaseEditor.Build.cs"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBaseEditor/Public/ReBoneImportLibrary.h",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBaseEditor/Public/ReBoneImportLibrary.h"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBaseEditor/Private/ReBoneBaseEditor.cpp",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBaseEditor/Private/ReBoneBaseEditor.cpp"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBaseEditor/Private/ReBoneImportLibrary.cpp",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBaseEditor/Private/ReBoneImportLibrary.cpp"
        ),
    ),
    (
        "Plugin/ReBoneBase/Source/ReBoneBaseEditor/Private/ReBoneBaseTests.cpp",
        include_str!(
            "../../integrations/unreal/ReBoneBase/Source/ReBoneBaseEditor/Private/ReBoneBaseTests.cpp"
        ),
    ),
];
fn png(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(bytes.into_inner())
}
const GODOT: &str = r#"extends AnimatedSprite2D
## Emitted for every marker of every frame entered, in frame order.
signal marker(name: String, payload: String)
@export_file("*.json") var manifest: String
## Play the package's reference audio in sync with the animation.
@export var play_audio := true
var metadata: Dictionary
var regions: Area2D
var audio: AudioStreamPlayer
var _shapes: Array[CollisionPolygon2D] = []
var _quiet := false
var _audio_start := 0.0
func _ready() -> void:
    texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
    var parsed = JSON.parse_string(FileAccess.get_file_as_string(manifest))
    metadata = _normalize(parsed if parsed is Dictionary else {})
    if metadata.is_empty():
        push_error("Invalid or unsupported ReBoneBase manifest")
        return
    sprite_frames = SpriteFrames.new()
    sprite_frames.remove_animation("default")
    centered = false
    offset = -Vector2(metadata.pivot.x, metadata.pivot.y)
    regions = Area2D.new()
    regions.name = "ReBoneRegions"
    add_child(regions)
    var textures = []
    for page in metadata.pages:
        textures.append(load(manifest.get_base_dir().path_join(page.file)) as Texture2D)
    for clip in metadata.clips:
        var clip_name = StringName(clip.animation)
        sprite_frames.add_animation(clip_name)
        sprite_frames.set_animation_loop(clip_name, clip.looped)
        sprite_frames.set_animation_speed(clip_name, clip.fps)
        for item in clip.frames:
            var atlas = AtlasTexture.new()
            atlas.atlas = textures[int(item.page)]
            atlas.region = Rect2(item.rect.x, item.rect.y, item.rect.w, item.rect.h)
            # Restore trimmed transparent space so every frame keeps its source size.
            atlas.margin = Rect2(item.trim.x, item.trim.y, item.source_size.w - item.rect.w, item.source_size.h - item.rect.h)
            sprite_frames.add_frame(clip_name, atlas, item.duration_ms * clip.fps / 1000.0)
    _setup_audio()
    frame_changed.connect(_on_frame_changed)
    play_clip(metadata.clips[0].animation)
# Upgrade a version 1 package to the version 2 shape; reject newer versions.
func _normalize(data: Dictionary) -> Dictionary:
    if data.get("format", "") == "rebonebase-sprites":
        return data if int(data.get("format_version", 0)) <= 2 else {}
    if int(data.get("version", 0)) != 1:
        return {}
    var pages = []
    var clips = []
    for clip in data.clips:
        var page = pages.size()
        pages.append({"file": clip.file})
        var frames = []
        for item in clip.frames:
            frames.append({"page": page, "rect": {"x": item.x, "y": item.y, "w": item.w, "h": item.h}, "source_size": {"w": data.width, "h": data.height}, "trim": {"x": 0, "y": 0}, "duration_ms": item.duration_ms, "source_time_ms": item.get("source_time_ms", 0), "events": item.events, "boxes": item.boxes})
        clips.append({"id": clip.get("id", ""), "name": clip.get("name", ""), "animation": clip.animation, "looped": clip.looped, "fps": clip.fps, "frames": frames})
    var result = {"pivot": {"x": data.width * data.pivot.x, "y": data.height * data.pivot.y}, "pages": pages, "clips": clips}
    if data.get("audio_file") != null:
        result["audio"] = {"file": data.audio_file, "start_ms": data.get("audio_start_ms", 0), "volume": data.get("audio_volume", 1.0), "enabled": data.get("audio_enabled", false)}
    return result
func _setup_audio() -> void:
    var info = metadata.get("audio")
    if not play_audio or not (info is Dictionary) or not info.get("enabled", false):
        return
    var stream = load(manifest.get_base_dir().path_join(info.file)) as AudioStream
    if stream == null:
        push_warning("ReBoneBase audio could not be loaded: " + str(info.file))
        return
    audio = AudioStreamPlayer.new()
    audio.name = "ReBoneAudio"
    audio.stream = stream
    audio.volume_db = linear_to_db(maxf(float(info.volume), 0.0001))
    add_child(audio)
    _audio_start = float(info.start_ms) / 1000.0
## Start an animation from its first frame.
func play_clip(animation_name: String) -> bool:
    if _clip(animation_name) == null:
        return false
    _quiet = true
    play(StringName(animation_name))
    set_frame_and_progress(0, 0.0)
    _quiet = false
    _enter(true)
    return true
## Return to the first frame, firing its markers, and play.
func restart() -> void:
    play_clip(str(animation))
## Pause the animation and its audio.
func pause_playback() -> void:
    pause()
    if audio:
        audio.stream_paused = true
## Continue from the current frame.
func resume_playback() -> void:
    play()
    if audio:
        audio.stream_paused = false
        _sync_audio(_current_frame())
## Play `base` (the authored clip name) facing `direction` (e, se, s, ...).
## Changing only the direction keeps the current frame and its progress and fires nothing.
func play_direction(base: String, direction: String) -> bool:
    var current = _clip(str(animation))
    for clip in metadata.clips:
        var facing = clip.get("direction")
        if clip.name == base and facing is Dictionary and facing.id == direction:
            var keep = current != null and current.name == base and current.frames.size() == clip.frames.size()
            if not keep:
                return play_clip(clip.animation)
            var keep_frame = frame
            var keep_progress = frame_progress
            _quiet = true
            play(StringName(clip.animation))
            set_frame_and_progress(keep_frame, keep_progress)
            _quiet = false
            _enter(false)
            return true
    return false
func _clip(animation_name: String):
    for clip in metadata.clips:
        if clip.animation == animation_name:
            return clip
    return null
func _current_frame():
    var clip = _clip(str(animation))
    return null if clip == null else clip.frames[mini(frame, clip.frames.size() - 1)]
func _on_frame_changed() -> void:
    if not _quiet:
        _enter(true)
# Start audio that becomes due inside a frame; stop it when the track or a one-shot clip ends.
func _process(_delta: float) -> void:
    if audio == null:
        return
    if not is_playing():
        if audio.playing and not audio.stream_paused:
            audio.stop()
        return
    var at = audio_target()
    var due = at >= 0.0 and at < audio.stream.get_length()
    if due and not audio.playing:
        audio.play(at)
    elif not due and audio.playing:
        audio.stop()
func _enter(fire: bool) -> void:
    var item = _current_frame()
    if item == null:
        return
    if fire:
        for event in item.events:
            marker.emit(event.name, event.payload)
    _apply_regions(item.boxes)
    _sync_audio(item)
## Where the audio should be for the current frame: the frame's retained source
## time minus the track's start. Negative before the track starts.
func audio_target() -> float:
    var item = _current_frame()
    if audio == null or item == null:
        return -1.0
    return float(item.get("source_time_ms", 0)) / 1000.0 - _audio_start + frame_progress * float(item.duration_ms) / 1000.0
func _sync_audio(item) -> void:
    if audio == null or item == null:
        return
    var at = audio_target()
    if at < 0.0 or at >= audio.stream.get_length():
        audio.stop()
    elif is_playing() and (not audio.playing or absf(audio_position() - at) > 0.08):
        audio.play(at)
## Audible audio position, compensated for mix granularity and output latency.
func audio_position() -> float:
    if audio == null or not audio.playing:
        return -1.0
    return audio.get_playback_position() + AudioServer.get_time_since_last_mix() - AudioServer.get_output_latency()
# One reused polygon per region; unused polygons are disabled, never freed.
func _apply_regions(boxes: Array) -> void:
    while _shapes.size() < boxes.size():
        var shape = CollisionPolygon2D.new()
        regions.add_child(shape)
        _shapes.append(shape)
    for i in _shapes.size():
        var shape = _shapes[i]
        if i >= boxes.size():
            shape.disabled = true
            continue
        var region = boxes[i]
        var points = PackedVector2Array()
        for point in region.corners:
            points.append(Vector2(point.x, point.y) + offset)
        shape.polygon = points
        shape.disabled = false
        shape.set_meta("id", region.id)
        shape.set_meta("kind", region.kind)
        shape.set_meta("region_name", region.name)
"#;
const UNITY: &str = include_str!("../../integrations/unity/ReBonePlayer.cs");
const UNITY_EDITOR: &str = include_str!("../../integrations/unity/Editor/ReBoneImporter.cs");
