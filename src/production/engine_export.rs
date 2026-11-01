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
            "Copy this folder into a Godot 4 project. Add an AnimatedSprite2D, attach ReBonePlayer.gd, and set Manifest to this folder's package.json (res:// path). The script creates SpriteFrames, emits marker(name,payload), and updates Area2D collision polygons. Artwork is nearest-neighbor. Sprites use the exported pivot. Audio WAV and its timing are in the manifest for your sound player. The script also reads version 1 packages."
        }
        EngineTarget::Unity => {
            files.push((PathBuf::from("ReBonePlayer.cs"), UNITY.as_bytes().to_vec()));
            files.push((
                PathBuf::from("Editor/ReBoneImporter.cs"),
                UNITY_EDITOR.as_bytes().to_vec(),
            ));
            "Copy this folder under Assets in Unity. Select package.json and choose Assets/ReBoneBase/Create Actor. The editor helper imports textures with point filtering and creates a ReBonePlayer with SpriteRenderer. Set Pixels Per Unit as needed; marker is a UnityEvent(name,payload). Trigger colliders follow exported boxes. Audio WAV and its timing are in the manifest for your AudioSource. The player also reads version 1 packages."
        }
        EngineTarget::Unreal => {
            for (path, text) in UNREAL {
                files.push((PathBuf::from(path), text.as_bytes().to_vec()));
            }
            "Unreal Engine 5.8 (tested 5.8.3) with Paper2D. Once per project: copy Plugin/ReBoneBase into the project's Plugins folder and open the project so Unreal builds it (Visual Studio 2022 is required for that build). Then, each time you export: run ReBoneImport.py from this folder with Tools > Execute Python Script, or headless with UnrealEditor-Cmd <Project>.uproject -run=pythonscript -script=\"<this folder>/ReBoneImport.py\". It imports pages as nearest-filtered, uncompressed textures without mipmaps, creates one Paper2D sprite per distinct frame image with the exported pivot, one flipbook per animation and a ReBoneAnimationSet data asset under /Game/ReBoneBase/<name>. Running it again updates those assets in place and removes generated assets the package no longer needs. Add a ReBoneFlipbookComponent, set its Animation Set, and bind OnMarker(Name, Payload, Frame). It plays clips with their looping, fires markers in order, keeps one reused box component per collision region, and offers PlayAnimation, PlayDirection(Name, Direction) and Restart. Timing: each flipbook runs at 1000/gcd(holds) frames per second with whole runs, so holds are exact to the millisecond. Flipbooks do not store looping; the component applies each clip's setting. A plain PaperFlipbookComponent plays the art and holds but not markers, regions or looping choices. Audio is imported as a SoundWave referenced by the data asset with its start time; scheduling it is left to your game."
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
signal marker(name: String, payload: String)
@export_file("*.json") var manifest: String
var metadata: Dictionary
var regions: Area2D
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
    frame_changed.connect(_on_frame)
    play(StringName(metadata.clips[0].animation))
    _on_frame()
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
            frames.append({"page": page, "rect": {"x": item.x, "y": item.y, "w": item.w, "h": item.h}, "source_size": {"w": data.width, "h": data.height}, "trim": {"x": 0, "y": 0}, "duration_ms": item.duration_ms, "events": item.events, "boxes": item.boxes})
        clips.append({"id": clip.get("id", ""), "name": clip.get("name", ""), "animation": clip.animation, "looped": clip.looped, "fps": clip.fps, "frames": frames})
    return {"pivot": {"x": data.width * data.pivot.x, "y": data.height * data.pivot.y}, "pages": pages, "clips": clips}
# Play `base` (the authored clip name) facing `direction` (e, se, s, ...).
# Changing only the direction keeps the current frame and its progress.
func play_direction(base: String, direction: String) -> bool:
    var current = _clip(str(animation))
    for clip in metadata.clips:
        var facing = clip.get("direction")
        if clip.name == base and facing is Dictionary and facing.id == direction:
            var keep = current != null and current.name == base and current.frames.size() == clip.frames.size()
            var keep_frame = frame
            var keep_progress = frame_progress
            play(StringName(clip.animation))
            if keep:
                set_frame_and_progress(keep_frame, keep_progress)
            return true
    return false
func _clip(animation_name: String):
    for clip in metadata.clips:
        if clip.animation == animation_name:
            return clip
    return null
func _on_frame() -> void:
    for clip in metadata.clips:
        if clip.animation == str(animation):
            var item = clip.frames[mini(frame, clip.frames.size()-1)]
            for event in item.events:
                marker.emit(event.name, event.payload)
            _regions.call_deferred(item.boxes)
            break
func _regions(boxes: Array) -> void:
    for child in regions.get_children():
        regions.remove_child(child)
        child.queue_free()
    for region in boxes:
        var shape = CollisionPolygon2D.new()
        shape.name = str(region.id)
        shape.set_meta("kind", region.kind)
        shape.set_meta("region_name", region.name)
        var points = PackedVector2Array()
        for point in region.corners:
            points.append(Vector2(point.x, point.y) + offset)
        shape.polygon = points
        regions.add_child(shape)
"#;
const UNITY: &str = r#"using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Events;
[RequireComponent(typeof(SpriteRenderer))]
public sealed class ReBonePlayer : MonoBehaviour {
 [Serializable] public class Point { public float x,y; }
 [Serializable] public class Size { public int w,h; }
 [Serializable] public class Area { public int x,y,w,h; }
 [Serializable] public class Marker { public string name,payload; }
 [Serializable] public class Region { public string name,kind; public Point center,size; public float angle; public Point[] corners; }
 [Serializable] public class Page { public string file; public int w,h; }
 // x,y,w,h, Clip.file, width/height and a normalized pivot are version 1 fields that Load upgrades.
 [Serializable] public class Frame { public int page,x,y,w,h,source_frame,duration_ms; public long source_time_ms; public Area rect; public Size source_size; public Point trim; public Marker[] events; public Region[] boxes; }
 [Serializable] public class Direction { public string id; public float angle; }
 [Serializable] public class Clip { public string name,file,id,animation; public bool looped; public float fps; public Direction direction; public Frame[] frames; }
 [Serializable] public class Manifest { public string format; public int format_version,version,width,height; public Size frame_size; public Point pivot; public Page[] pages; public Clip[] clips; }
 [Serializable] public class MarkerEvent : UnityEvent<string,string> {}
 public TextAsset manifest; public Texture2D[] sheets; public float pixelsPerUnit=16; public MarkerEvent marker=new MarkerEvent();
 Manifest data; SpriteRenderer output; Sprite[][] sprites; int clipIndex,frameIndex; float elapsed; bool playing=true;
 readonly List<GameObject> regions=new List<GameObject>();
 // Parse a manifest, upgrading version 1 to the version 2 shape; null when unsupported.
 public static Manifest Load(string json){var m=JsonUtility.FromJson<Manifest>(json);if(m==null||m.clips==null)return null;
  if(m.format=="rebonebase-sprites")return m.format_version<=2?m:null;if(m.version!=1)return null;
  m.frame_size=new Size{w=m.width,h=m.height};m.pivot=new Point{x=m.pivot.x*m.width,y=m.pivot.y*m.height};m.pages=new Page[m.clips.Length];
  for(int c=0;c<m.clips.Length;c++){m.pages[c]=new Page{file=m.clips[c].file};foreach(var f in m.clips[c].frames){f.page=c;f.rect=new Area{x=f.x,y=f.y,w=f.w,h=f.h};f.source_size=m.frame_size;f.trim=new Point();}}
  return m;}
 void Start(){
  data=manifest?Load(manifest.text):null;if(data==null){enabled=false;return;}output=GetComponent<SpriteRenderer>();
  sprites=new Sprite[data.clips.Length][];
  for(int c=0;c<data.clips.Length;c++){sprites[c]=new Sprite[data.clips[c].frames.Length];
   for(int i=0;i<sprites[c].Length;i++){var f=data.clips[c].frames[i];var r=f.rect;var texture=sheets[f.page];texture.filterMode=FilterMode.Point;if(r.w<=0||r.h<=0)continue;
    // The pivot is in untrimmed frame pixels; express it relative to the trimmed rect, bottom-up.
    var pivot=new Vector2((data.pivot.x-f.trim.x)/r.w,(r.h-(data.pivot.y-f.trim.y))/r.h);
    sprites[c][i]=Sprite.Create(texture,new Rect(r.x,texture.height-r.y-r.h,r.w,r.h),pivot,pixelsPerUnit,0,SpriteMeshType.FullRect);}}
  ShowFrame();
 }
 // Play the authored clip `name` facing `direction` (e, se, s, ...). Changing only the direction keeps frame and timing.
 public bool Play(string name,string direction){if(data==null)return false;var current=data.clips[clipIndex];for(int i=0;i<data.clips.Length;i++){var c=data.clips[i];if(c.name==name&&c.direction!=null&&c.direction.id==direction){bool same=current.name==name&&current.frames.Length==c.frames.Length;clipIndex=i;if(!same){frameIndex=0;elapsed=0;}playing=true;ShowFrame();return true;}}return false;}
 public void Play(string name){if(data==null)return;for(int i=0;i<data.clips.Length;i++)if(data.clips[i].animation==name||data.clips[i].id==name){clipIndex=i;frameIndex=0;elapsed=0;playing=true;ShowFrame();return;}}
 public void Pause(){playing=false;}
 void Update(){if(!playing||data==null)return;elapsed+=Time.deltaTime*1000;var clip=data.clips[clipIndex];int guard=0;
  while(elapsed>=clip.frames[frameIndex].duration_ms && guard++<1024){elapsed-=clip.frames[frameIndex].duration_ms;frameIndex++;
   if(frameIndex>=clip.frames.Length){if(clip.looped)frameIndex=0;else{frameIndex=clip.frames.Length-1;playing=false;elapsed=0;break;}}ShowFrame();}}
 void ShowFrame(){var f=data.clips[clipIndex].frames[frameIndex];output.sprite=sprites[clipIndex][frameIndex];foreach(var e in f.events)marker.Invoke(e.name,e.payload);
  foreach(var obj in regions)Destroy(obj);regions.Clear();foreach(var b in f.boxes){var obj=new GameObject(b.name);obj.transform.SetParent(transform,false);
   obj.transform.localPosition=new Vector3((b.center.x-data.pivot.x)/pixelsPerUnit,(data.pivot.y-b.center.y)/pixelsPerUnit,0);
   obj.transform.localRotation=Quaternion.Euler(0,0,-b.angle);var collider=obj.AddComponent<BoxCollider2D>();collider.isTrigger=true;collider.size=new Vector2(b.size.x,b.size.y)/pixelsPerUnit;regions.Add(obj);}}
 void OnDestroy(){if(sprites!=null)foreach(var clip in sprites)foreach(var sprite in clip)if(sprite)Destroy(sprite);}
}
"#;
const UNITY_EDITOR: &str = r#"using System.IO;
using UnityEngine;
using UnityEditor;
public static class ReBoneImporter {
 [MenuItem("Assets/ReBoneBase/Create Actor")]
 public static void CreateActor(){var path=AssetDatabase.GetAssetPath(Selection.activeObject);if(!path.EndsWith("package.json")){EditorUtility.DisplayDialog("ReBoneBase","Select the exported package.json file.","OK");return;}
  var manifest=AssetDatabase.LoadAssetAtPath<TextAsset>(path);var data=ReBonePlayer.Load(manifest.text);if(data==null){EditorUtility.DisplayDialog("ReBoneBase","This package version is not supported by this importer.","OK");return;}var obj=new GameObject(Path.GetFileName(Path.GetDirectoryName(path)));Undo.RegisterCreatedObjectUndo(obj,"Create ReBoneBase Actor");
  var player=obj.AddComponent<ReBonePlayer>();player.manifest=manifest;player.sheets=new Texture2D[data.pages.Length];
  for(int i=0;i<data.pages.Length;i++){var image=Path.Combine(Path.GetDirectoryName(path),data.pages[i].file).Replace('\\','/');var importer=AssetImporter.GetAtPath(image) as TextureImporter;if(importer){importer.textureType=TextureImporterType.Default;importer.filterMode=FilterMode.Point;importer.mipmapEnabled=false;importer.textureCompression=TextureImporterCompression.Uncompressed;importer.SaveAndReimport();}player.sheets[i]=AssetDatabase.LoadAssetAtPath<Texture2D>(image);}
  Selection.activeGameObject=obj;
 }
}
"#;
