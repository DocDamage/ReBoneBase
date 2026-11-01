using System.IO;
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
