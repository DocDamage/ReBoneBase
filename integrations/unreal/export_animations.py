"""Run in Unreal Editor with selected Animation Sequences; exports to Saved/ReBoneBaseExports.
Requires Unreal's Python Editor Script Plugin. Does not modify the source assets.
"""
import os
import re
import unreal


def export_selected():
    output = os.path.join(unreal.Paths.project_saved_dir(), "ReBoneBaseExports")
    os.makedirs(output, exist_ok=True)
    exported = []
    for asset in unreal.EditorUtilityLibrary.get_selected_assets():
        if not isinstance(asset, unreal.AnimSequence):
            continue
        name = re.sub(r"[^A-Za-z0-9_.-]", "_", asset.get_name()) or "Animation"
        path = os.path.join(output, name + ".fbx")
        suffix = 2
        while os.path.exists(path):
            path = os.path.join(output, name + "_" + str(suffix) + ".fbx")
            suffix += 1
        task = unreal.AssetExportTask()
        task.object = asset
        task.filename = path
        task.automated = True
        task.prompt = False
        task.replace_identical = False
        task.exporter = unreal.AnimSequenceExporterFBX()
        task.options = unreal.FbxExportOption()
        if not unreal.Exporter.run_asset_export_task(task):
            unreal.log_warning("ReBoneBase animation export failed: " + asset.get_name())
            continue
        exported.append(path)
        unreal.log("ReBoneBase exported: " + path)
    if not exported:
        unreal.log_warning("Select Animation Sequence assets in the Content Browser, then run this script again.")
    return exported


if __name__ == "__main__":
    export_selected()
