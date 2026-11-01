using System;
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
