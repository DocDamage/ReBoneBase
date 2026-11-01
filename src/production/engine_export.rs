use super::*;
use crate::{
    jobs::Progress,
    sprite_package::{self, Offset, PageKind, SpritePackage},
};
use std::path::{Path, PathBuf};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum EngineTarget {
    #[default]
    Godot,
    Unity,
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
/// Render the whole package in memory without touching the destination.
pub fn plan_package(
    project: &Project,
    engine: EngineTarget,
    progress: &Progress,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    progress.set_stage("Preparing canvas");
    let prepared = crate::project::prepare_export(project)?;
    let project = &prepared;
    let total_frames: u64 = project
        .rig
        .clips
        .iter()
        .map(|c| c.frame_sequence().len() as u64)
        .sum();
    crate::project::project_export_budget(project, total_frames)?;
    let mut files: Vec<(PathBuf, Vec<u8>)> = vec![];
    let mut package = SpritePackage::new(project, "ReBoneBase Production");
    let animations =
        sprite_package::unique_names(project.rig.clips.iter().map(|c| c.name.as_str()));
    let clip_count = project.rig.clips.len();
    for (i, animation) in animations.into_iter().enumerate() {
        progress.checkpoint()?;
        let mut p = project.clone();
        p.rig.active_clip = i;
        let clip = p.rig.active_clip();
        progress.set_stage(format!(
            "Rendering clip {} of {clip_count} · {}",
            i + 1,
            clip.name
        ));
        let sequence = clip.frame_sequence();
        let columns = 8u32.min(sequence.len() as u32).max(1);
        let sheet = crate::project::render_sheet(&p, columns, progress)?;
        let file = format!("clip-{:02}.png", i + 1);
        let page = package.add_page(&file, PageKind::Atlas, sheet.width(), sheet.height());
        let mut bytes = std::io::Cursor::new(Vec::new());
        sheet.write_to(&mut bytes, image::ImageFormat::Png)?;
        files.push((PathBuf::from(&file), bytes.into_inner()));
        let frames = sequence
            .iter()
            .enumerate()
            .map(|(j, source)| {
                let rect = sprite_package::grid_rect(j, columns, p.rig.width, p.rig.height);
                sprite_package::frame_in_active(&p, *source, page, Some(rect), Offset::default())
            })
            .collect();
        package.clips.push(sprite_package::Clip {
            id: format!("clip-{:02}", i + 1),
            animation,
            name: clip.name.clone(),
            direction: None,
            fps: clip.fps,
            looped: clip.looped,
            playback: clip.playback,
            frames,
        });
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
        clips.append({"id": clip.get("id", ""), "animation": clip.animation, "looped": clip.looped, "fps": clip.fps, "frames": frames})
    return {"pivot": {"x": data.width * data.pivot.x, "y": data.height * data.pivot.y}, "pages": pages, "clips": clips}
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
 [Serializable] public class Clip { public string name,file,id,animation; public bool looped; public float fps; public Frame[] frames; }
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
