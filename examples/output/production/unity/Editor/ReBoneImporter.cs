using System.IO;
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
