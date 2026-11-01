using System.IO;
using System.Linq;
using UnityEditor;
using UnityEngine;

/// Creates or updates a ReBonePlayer actor from an exported package.json.
public static class ReBoneImporter
{
    [MenuItem("Assets/ReBoneBase/Create Actor")]
    public static void CreateActor()
    {
        var path = AssetDatabase.GetAssetPath(Selection.activeObject);
        if (!path.EndsWith("package.json"))
        {
            EditorUtility.DisplayDialog("ReBoneBase", "Select the exported package.json file.", "OK");
            return;
        }
        var actor = Import(path);
        if (actor == null)
        {
            EditorUtility.DisplayDialog("ReBoneBase", "This package version is not supported by this importer.", "OK");
            return;
        }
        Selection.activeGameObject = actor.gameObject;
    }

    /// Import `packageJson` (an Assets/... path). Importing the same package again
    /// updates the existing actor in the open scene instead of adding another.
    public static ReBonePlayer Import(string packageJson)
    {
        AssetDatabase.ImportAsset(packageJson);
        var manifest = AssetDatabase.LoadAssetAtPath<TextAsset>(packageJson);
        var data = manifest ? ReBonePlayer.Load(manifest.text) : null;
        if (data == null) return null;
        var folder = Path.GetDirectoryName(packageJson).Replace('\\', '/');
        var sheets = new Texture2D[data.pages.Length];
        for (int i = 0; i < data.pages.Length; i++)
        {
            var image = folder + "/" + data.pages[i].file;
            if (AssetImporter.GetAtPath(image) is TextureImporter importer)
            {
                // Pixel art: exact texels. Pages are not powers of two and must not be rescaled.
                importer.textureType = TextureImporterType.Default;
                importer.npotScale = TextureImporterNPOTScale.None;
                importer.filterMode = FilterMode.Point;
                importer.mipmapEnabled = false;
                importer.alphaIsTransparency = true;
                importer.textureCompression = TextureImporterCompression.Uncompressed;
                importer.maxTextureSize = 8192;
                importer.SaveAndReimport();
            }
            sheets[i] = AssetDatabase.LoadAssetAtPath<Texture2D>(image);
        }
        AudioClip clip = null;
        if (data.audio != null && !string.IsNullOrEmpty(data.audio.file))
            clip = AssetDatabase.LoadAssetAtPath<AudioClip>(folder + "/" + data.audio.file);
        // Scene actors only (not prefabs or assets); works across Unity versions.
        var player = Resources.FindObjectsOfTypeAll<ReBonePlayer>()
            .FirstOrDefault(p => p.gameObject.scene.IsValid() && p.manifest == manifest);
        if (player == null)
        {
            var obj = new GameObject(Path.GetFileName(folder));
            Undo.RegisterCreatedObjectUndo(obj, "Create ReBoneBase Actor");
            player = obj.AddComponent<ReBonePlayer>();
        }
        else
        {
            Undo.RecordObject(player, "Update ReBoneBase Actor");
        }
        player.manifest = manifest;
        player.sheets = sheets;
        player.audioClip = clip;
        EditorUtility.SetDirty(player);
        return player;
    }
}
