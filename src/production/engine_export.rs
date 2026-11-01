use super::*;
use crate::jobs::Progress;
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
    let mut clips = vec![];
    let mut used_names = std::collections::HashSet::new();
    let clip_count = project.rig.clips.len();
    for i in 0..clip_count {
        progress.checkpoint()?;
        let mut p = project.clone();
        p.rig.active_clip = i;
        let clip = p.rig.active_clip();
        progress.set_stage(format!(
            "Rendering clip {} of {clip_count} · {}",
            i + 1,
            clip.name
        ));
        let base = if clip.name.trim().is_empty() {
            "Animation"
        } else {
            clip.name.as_str()
        };
        let mut animation = base.to_string();
        let mut suffix = 2;
        while used_names.contains(&animation) {
            animation = format!("{base} ({suffix})");
            suffix += 1;
        }
        used_names.insert(animation.clone());
        let sequence = clip.frame_sequence();
        let columns = 8u32.min(sequence.len() as u32).max(1);
        let sheet = crate::project::render_sheet(&p, columns, progress)?;
        let file = format!("clip-{:02}.png", i + 1);
        let mut bytes = std::io::Cursor::new(Vec::new());
        sheet.write_to(&mut bytes, image::ImageFormat::Png)?;
        files.push((PathBuf::from(&file), bytes.into_inner()));
        let mut frames = vec![];
        for (j, source) in sequence.iter().enumerate() {
            let events: Vec<_> = p
                .extras
                .production
                .events
                .iter()
                .filter(|e| e.clip == i && (e.frame - *source as f32).abs() < 0.5)
                .collect();
            frames.push(serde_json::json!({"source_frame":source,"source_time_ms":clip.source_time_ms(*source as f32),"duration_ms":clip.duration_ms(*source),"x":j as u32%columns*p.rig.width,"y":j as u32/columns*p.rig.height,"w":p.rig.width,"h":p.rig.height,"events":events,"boxes":collision_pixels(&p,*source as f32)}));
        }
        clips.push(serde_json::json!({"id":format!("clip-{:02}",i+1),"animation":animation,"name":clip.name,"looped":clip.looped,"fps":clip.fps,"file":file,"frames":frames}));
    }
    let audio = project.extras.production.audio.as_ref();
    if let Some(audio) = audio {
        files.push((
            PathBuf::from("reference.wav"),
            audio::encode_wav(&audio.samples(), audio.sample_rate, audio.channels),
        ));
    }
    let manifest = serde_json::json!({"version":1,"generator":"ReBoneBase Production","name":project.rig.name,"width":project.rig.width,"height":project.rig.height,"pivot":project.extras.production.pivot,"canvas_origin":project.extras.canvas_origin,"coordinates":"pixels, top-left origin, y down; angles degrees clockwise","audio_file":audio.map(|_|"reference.wav"),"audio_start_ms":audio.map_or(0,|a|a.start_ms),"audio_volume":audio.map_or(1.,|a|a.volume),"audio_enabled":audio.is_some_and(|a|a.enabled),"clips":clips});
    files.push((
        PathBuf::from("package.json"),
        serde_json::to_vec_pretty(&manifest)?,
    ));
    let instructions = match engine {
        EngineTarget::Godot => {
            files.push((PathBuf::from("ReBonePlayer.gd"), GODOT.as_bytes().to_vec()));
            "Copy this folder into a Godot 4 project. Add an AnimatedSprite2D, attach ReBonePlayer.gd, and set Manifest to this folder's package.json (res:// path). The script creates SpriteFrames, emits marker(name,payload), and updates Area2D collision polygons. Artwork is nearest-neighbor. Sprites use the exported pivot. Audio WAV and its timing are in the manifest for your sound player."
        }
        EngineTarget::Unity => {
            files.push((PathBuf::from("ReBonePlayer.cs"), UNITY.as_bytes().to_vec()));
            files.push((
                PathBuf::from("Editor/ReBoneImporter.cs"),
                UNITY_EDITOR.as_bytes().to_vec(),
            ));
            "Copy this folder under Assets in Unity. Select package.json and choose Assets/ReBoneBase/Create Actor. The editor helper imports textures with point filtering and creates a ReBonePlayer with SpriteRenderer. Set Pixels Per Unit as needed; marker is a UnityEvent(name,payload). Trigger colliders follow exported boxes. Audio WAV and its timing are in the manifest for your AudioSource."
        }
        EngineTarget::Generic => {
            "Read package.json: clips contain ordered frame rectangles, duration_ms, events and image-space boxes. PNG coordinates start at top-left; pivot is normalized in the same coordinate system. Reverse/ping-pong playback is already expanded into frame order. WAV references, when present, have audio_start_ms and audio_volume. Each frame includes source_time_ms to align audio and references with retained source timing after trimming or splitting."
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
    metadata = JSON.parse_string(FileAccess.get_file_as_string(manifest))
    if metadata.is_empty():
        push_error("Invalid ReBoneBase manifest")
        return
    sprite_frames = SpriteFrames.new()
    sprite_frames.remove_animation("default")
    centered = false
    offset = -Vector2(metadata.width * metadata.pivot.x, metadata.height * metadata.pivot.y)
    regions = Area2D.new()
    regions.name = "ReBoneRegions"
    add_child(regions)
    for clip in metadata.clips:
        var clip_name = StringName(clip.animation)
        sprite_frames.add_animation(clip_name)
        sprite_frames.set_animation_loop(clip_name, clip.looped)
        sprite_frames.set_animation_speed(clip_name, clip.fps)
        var texture = load(manifest.get_base_dir().path_join(clip.file)) as Texture2D
        for item in clip.frames:
            var atlas = AtlasTexture.new()
            atlas.atlas = texture
            atlas.region = Rect2(item.x, item.y, item.w, item.h)
            sprite_frames.add_frame(clip_name, atlas, item.duration_ms * clip.fps / 1000.0)
    frame_changed.connect(_on_frame)
    play(StringName(metadata.clips[0].animation))
    _on_frame()
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
 [Serializable] public class Marker { public string name,payload; }
 [Serializable] public class Region { public string name,kind; public Point center,size; public float angle; public Point[] corners; }
 [Serializable] public class Frame { public int x,y,w,h,source_frame,duration_ms; public long source_time_ms; public Marker[] events; public Region[] boxes; }
 [Serializable] public class Clip { public string name,file,id,animation; public bool looped; public float fps; public Frame[] frames; }
 [Serializable] public class Manifest { public int width,height; public Point pivot; public Clip[] clips; }
 [Serializable] public class MarkerEvent : UnityEvent<string,string> {}
 public TextAsset manifest; public Texture2D[] sheets; public float pixelsPerUnit=16; public MarkerEvent marker=new MarkerEvent();
 Manifest data; SpriteRenderer output; Sprite[][] sprites; int clipIndex,frameIndex; float elapsed; bool playing=true;
 readonly List<GameObject> regions=new List<GameObject>();
 void Start(){
  if(!manifest){enabled=false;return;} data=JsonUtility.FromJson<Manifest>(manifest.text); output=GetComponent<SpriteRenderer>();
  sprites=new Sprite[data.clips.Length][];
  for(int c=0;c<data.clips.Length;c++){sprites[c]=new Sprite[data.clips[c].frames.Length];var texture=sheets[c];texture.filterMode=FilterMode.Point;
   for(int i=0;i<sprites[c].Length;i++){var f=data.clips[c].frames[i];sprites[c][i]=Sprite.Create(texture,new Rect(f.x,texture.height-f.y-f.h,f.w,f.h),new Vector2(data.pivot.x,1-data.pivot.y),pixelsPerUnit,0,SpriteMeshType.FullRect);}}
  ShowFrame();
 }
 public void Play(string name){if(data==null)return;for(int i=0;i<data.clips.Length;i++)if(data.clips[i].animation==name||data.clips[i].id==name){clipIndex=i;frameIndex=0;elapsed=0;playing=true;ShowFrame();return;}}
 public void Pause(){playing=false;}
 void Update(){if(!playing||data==null)return;elapsed+=Time.deltaTime*1000;var clip=data.clips[clipIndex];int guard=0;
  while(elapsed>=clip.frames[frameIndex].duration_ms && guard++<1024){elapsed-=clip.frames[frameIndex].duration_ms;frameIndex++;
   if(frameIndex>=clip.frames.Length){if(clip.looped)frameIndex=0;else{frameIndex=clip.frames.Length-1;playing=false;elapsed=0;break;}}ShowFrame();}}
 void ShowFrame(){var f=data.clips[clipIndex].frames[frameIndex];output.sprite=sprites[clipIndex][frameIndex];foreach(var e in f.events)marker.Invoke(e.name,e.payload);
  foreach(var obj in regions)Destroy(obj);regions.Clear();foreach(var b in f.boxes){var obj=new GameObject(b.name);obj.transform.SetParent(transform,false);
   obj.transform.localPosition=new Vector3((b.center.x-data.width*data.pivot.x)/pixelsPerUnit,(data.height*data.pivot.y-b.center.y)/pixelsPerUnit,0);
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
  var manifest=AssetDatabase.LoadAssetAtPath<TextAsset>(path);var data=JsonUtility.FromJson<ReBonePlayer.Manifest>(manifest.text);var obj=new GameObject(Path.GetFileName(Path.GetDirectoryName(path)));Undo.RegisterCreatedObjectUndo(obj,"Create ReBoneBase Actor");
  var player=obj.AddComponent<ReBonePlayer>();player.manifest=manifest;player.sheets=new Texture2D[data.clips.Length];
  for(int i=0;i<data.clips.Length;i++){var image=Path.Combine(Path.GetDirectoryName(path),data.clips[i].file).Replace('\\','/');var importer=AssetImporter.GetAtPath(image) as TextureImporter;if(importer){importer.textureType=TextureImporterType.Default;importer.filterMode=FilterMode.Point;importer.mipmapEnabled=false;importer.textureCompression=TextureImporterCompression.Uncompressed;importer.SaveAndReimport();}player.sheets[i]=AssetDatabase.LoadAssetAtPath<Texture2D>(image);}
  Selection.activeGameObject=obj;
 }
}
"#;
