"""Import a ReBoneBase sprite package into Unreal as Paper2D assets.

Run in the Unreal Editor (Tools > Execute Python Script) from inside an exported
package folder, or headless:

    UnrealEditor-Cmd.exe Project.uproject -run=pythonscript -script="<package>/ReBoneImport.py"

Requires the ReBoneBase plugin (shipped in the package's Plugin folder) and the
Paper2D, Python Editor Script and Editor Scripting Utilities plugins.

Running it again updates the same assets in place: textures are reimported,
sprites and flipbooks are rebuilt, and generated assets the package no longer
needs are removed. Assets you made yourself are never touched.
"""
import json
import math
import os
import re
from functools import reduce

FORMAT = "rebonebase-sprites"
SUPPORTED_VERSION = 2
GENERATED_TAG = "ReBoneBaseGenerated"
PIXELS_PER_UNIT = 1.0


def asset_name(text, fallback):
    name = re.sub(r"[^A-Za-z0-9_]", "_", text or "").strip("_")
    return name[:64] or fallback


def load_package(folder):
    with open(os.path.join(folder, "package.json"), encoding="utf-8") as f:
        package = json.load(f)
    if package.get("format") != FORMAT:
        raise ValueError("package.json is not a ReBoneBase sprite package")
    if int(package.get("format_version", 0)) > SUPPORTED_VERSION:
        raise ValueError("This package is newer than the importer; export it again with a matching ReBoneBase")
    return package


def flipbook_timing(durations_ms):
    """Exact holds: a base rate of 1000/gcd frames per second and whole runs per frame."""
    holds = [max(1, int(d)) for d in durations_ms]
    step = reduce(math.gcd, holds)
    return 1000.0 / step, [h // step for h in holds]


def plan(package, destination=None):
    """Asset names and contents for a package, independent of Unreal."""
    name = asset_name(package.get("name"), "Sprites")
    root = destination or "/Game/ReBoneBase/" + name
    pivot = package["pivot"]
    textures = [
        {"file": page["file"], "asset": "T_%s_%02d" % (name, i + 1)}
        for i, page in enumerate(package["pages"])
    ]
    sprites, keys = [], {}
    flipbooks = []
    for clip in package["clips"]:
        frame_sprites = []
        for frame in clip["frames"]:
            r, trim = frame["rect"], frame["trim"]
            key = (frame["page"], r["x"], r["y"], r["w"], r["h"], trim["x"], trim["y"])
            if key not in keys:
                keys[key] = len(sprites)
                sprites.append({
                    "asset": "S_%s_%04d" % (name, len(sprites) + 1),
                    "texture": frame["page"],
                    "offset": (r["x"], r["y"]),
                    "size": (r["w"], r["h"]),
                    # The pivot is in untrimmed frame pixels; Paper2D wants texture pixels.
                    "pivot": (r["x"] - trim["x"] + pivot["x"], r["y"] - trim["y"] + pivot["y"]),
                })
            frame_sprites.append(keys[key])
        fps, runs = flipbook_timing([f["duration_ms"] for f in clip["frames"]])
        flipbooks.append({
            "asset": "FB_%s" % asset_name(clip["animation"], clip["id"]),
            "sprites": frame_sprites,
            "runs": runs,
            "fps": fps,
        })
    return {
        "name": name,
        "root": root,
        "textures": textures,
        "sprites": sprites,
        "flipbooks": flipbooks,
        "data": "DA_%s" % name,
        "audio": "A_%s" % name if package.get("audio") else None,
    }


def import_package(folder=None, destination=None):
    import unreal

    folder = os.path.abspath(folder or os.path.dirname(os.path.abspath(__file__)))
    package = load_package(folder)
    layout = plan(package, destination)
    root = layout["root"]
    lib = unreal.EditorAssetLibrary
    tools = unreal.AssetToolsHelpers.get_asset_tools()
    made = set()

    def mark(asset):
        lib.set_metadata_tag(asset, GENERATED_TAG, layout["name"])
        made.add(asset.get_path_name().split(".")[0])
        return asset

    def existing_or_new(path, name, cls, factory):
        full = "%s/%s" % (path, name)
        if lib.does_asset_exist(full):
            asset = lib.load_asset(full)
            if isinstance(asset, cls):
                return asset
            raise RuntimeError("%s exists and is not a %s; move it and import again" % (full, cls.__name__))
        return tools.create_asset(name, path, cls, factory)

    def import_file(source, path, name):
        task = unreal.AssetImportTask()
        task.filename = source
        task.destination_path = path
        task.destination_name = name
        task.replace_existing = True
        task.automated = True
        task.save = False
        tools.import_asset_tasks([task])
        asset = lib.load_asset("%s/%s" % (path, name))
        if asset is None:
            raise RuntimeError("Unreal could not import " + source)
        return asset

    textures = []
    for t in layout["textures"]:
        texture = import_file(os.path.join(folder, t["file"]), root + "/Textures", t["asset"])
        # Pixel art: exact texels, no mips, no lossy compression, no streaming.
        texture.set_editor_property("filter", unreal.TextureFilter.TF_NEAREST)
        texture.set_editor_property("compression_settings", unreal.TextureCompressionSettings.TC_EDITOR_ICON)
        texture.set_editor_property("mip_gen_settings", unreal.TextureMipGenSettings.TMGS_NO_MIPMAPS)
        texture.set_editor_property("lod_group", unreal.TextureGroup.TEXTUREGROUP_PIXELS2D)
        texture.set_editor_property("never_stream", True)
        texture.set_editor_property("srgb", True)
        textures.append(mark(texture))

    sprites = []
    for s in layout["sprites"]:
        sprite = existing_or_new(root + "/Sprites", s["asset"], unreal.PaperSprite, unreal.PaperSpriteFactory())
        ok = unreal.ReBoneImportLibrary.configure_sprite(
            sprite, textures[s["texture"]],
            unreal.IntPoint(*s["offset"]), unreal.IntPoint(*s["size"]),
            unreal.Vector2D(*s["pivot"]), PIXELS_PER_UNIT)
        if not ok:
            raise RuntimeError("Could not configure sprite " + s["asset"])
        sprites.append(mark(sprite))

    audio = None
    if layout["audio"]:
        audio = mark(import_file(os.path.join(folder, package["audio"]["file"]), root + "/Audio", layout["audio"]))

    clips = []
    for clip, fb in zip(package["clips"], layout["flipbooks"]):
        flipbook = existing_or_new(root + "/Flipbooks", fb["asset"], unreal.PaperFlipbook, unreal.PaperFlipbookFactory())
        if not unreal.ReBoneImportLibrary.configure_flipbook(flipbook, [sprites[i] for i in fb["sprites"]], fb["runs"], fb["fps"]):
            raise RuntimeError("Could not configure flipbook " + fb["asset"])
        mark(flipbook)
        frames = []
        for frame in clip["frames"]:
            markers = [unreal.ReBoneMarker(name=e["name"], payload=e["payload"]) for e in frame["events"]]
            regions = [
                unreal.ReBoneRegion(
                    id=int(b["id"]), name=b["name"], kind=str(b["kind"]),
                    center=unreal.Vector2D(b["center"]["x"], b["center"]["y"]),
                    size=unreal.Vector2D(b["size"]["x"], b["size"]["y"]),
                    angle=float(b["angle"]),
                    corners=[unreal.Vector2D(c["x"], c["y"]) for c in b["corners"]])
                for b in frame["boxes"]
            ]
            frames.append(unreal.ReBoneFrame(
                source_frame=frame["source_frame"], source_time_ms=frame["source_time_ms"],
                duration_ms=frame["duration_ms"], markers=markers, regions=regions))
        direction = clip.get("direction") or {}
        clips.append(unreal.ReBoneClip(
            id=clip["id"], animation=clip["animation"], name=clip["name"],
            direction=direction.get("id", ""), direction_angle=float(direction.get("angle", 0.0)),
            looped=clip["looped"], fps=float(clip["fps"]), flipbook=flipbook, frames=frames))

    data_factory = unreal.DataAssetFactory()
    data_factory.set_editor_property("data_asset_class", unreal.ReBoneAnimationSet)
    data = existing_or_new(root, layout["data"], unreal.ReBoneAnimationSet, data_factory)
    data.set_editor_property("format_version", int(package["format_version"]))
    data.set_editor_property("frame_size", unreal.IntPoint(package["frame_size"]["w"], package["frame_size"]["h"]))
    data.set_editor_property("pivot", unreal.Vector2D(package["pivot"]["x"], package["pivot"]["y"]))
    data.set_editor_property("pixels_per_unit", PIXELS_PER_UNIT)
    data.set_editor_property("clips", clips)
    data.set_editor_property("audio", audio)
    if package.get("audio"):
        data.set_editor_property("audio_start_ms", int(package["audio"]["start_ms"]))
        data.set_editor_property("audio_volume", float(package["audio"]["volume"]))
    mark(data)

    # Remove generated assets this package no longer produces.
    removed = []
    for path in lib.list_assets(root, recursive=True, include_folder=False):
        path = path.split(".")[0]
        if path in made:
            continue
        asset = lib.load_asset(path)
        if asset is not None and lib.get_metadata_tag(asset, GENERATED_TAG) == layout["name"]:
            if lib.delete_asset(path):
                removed.append(path)
            else:
                unreal.log_warning("ReBoneBase: could not remove stale asset (still referenced?): " + path)
    lib.save_directory(root, only_if_is_dirty=False, recursive=True)
    unreal.log("ReBoneBase imported %d clips, %d sprites, %d textures into %s; removed %d stale assets"
               % (len(clips), len(sprites), len(textures), root, len(removed)))
    return root


if __name__ == "__main__":
    import_package()
