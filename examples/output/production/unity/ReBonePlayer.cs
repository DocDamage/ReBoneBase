using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Events;
[RequireComponent(typeof(SpriteRenderer))]
public sealed class ReBonePlayer : MonoBehaviour {
 [Serializable] public class Point { public float x,y; }
 [Serializable] public class Marker { public string name,payload; }
 [Serializable] public class Region { public string name,kind; public Point center,size; public float angle; public Point[] corners; }
 [Serializable] public class Frame { public int x,y,w,h,source_frame,duration_ms; public Marker[] events; public Region[] boxes; }
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
