"""Compare imported Paper2D assets with the package they came from. Prints VERIFY OK/FAIL lines."""
import json, os, sys
import unreal

lib = unreal.EditorAssetLibrary
problems = []
def check(cond, msg):
    if not cond:
        problems.append(msg)

def verify(folder, root):
    package = json.load(open(os.path.join(folder, "package.json"), encoding="utf-8"))
    name = [a for a in lib.list_assets(root, recursive=False) if "/DA_" in a][0].split(".")[0]
    data = lib.load_asset(name)
    check(data.get_editor_property("pivot") == unreal.Vector2D(package["pivot"]["x"], package["pivot"]["y"]), "pivot")
    clips = data.get_editor_property("clips")
    check(len(clips) == len(package["clips"]), "clip count")
    frames = 0
    for clip, want in zip(clips, package["clips"]):
        check(clip.get_editor_property("animation") == want["animation"], "animation " + want["animation"])
        check(clip.get_editor_property("looped") == want["looped"], "looped " + want["animation"])
        check(clip.get_editor_property("direction") == ((want.get("direction") or {}).get("id", "")), "direction")
        fb = clip.get_editor_property("flipbook")
        keys = fb.get_editor_property("key_frames")
        fps = fb.get_editor_property("frames_per_second")
        check(len(keys) == len(want["frames"]), "frame count " + want["animation"])
        for i, (key, f) in enumerate(zip(keys, want["frames"])):
            frames += 1
            hold = key.get_editor_property("frame_run") / fps * 1000
            check(abs(hold - f["duration_ms"]) < 1e-3, "%s %d hold %.4f vs %d" % (want["animation"], i, hold, f["duration_ms"]))
            sprite = key.get_editor_property("sprite")
            r, t, p = f["rect"], f["trim"], package["pivot"]
            check(sprite.get_editor_property("source_uv") == unreal.Vector2D(r["x"], r["y"]), "%s %d uv" % (want["animation"], i))
            check(sprite.get_editor_property("source_dimension") == unreal.Vector2D(r["w"], r["h"]), "%s %d size" % (want["animation"], i))
            check(sprite.get_editor_property("pivot_mode") == unreal.SpritePivotMode.CUSTOM, "pivot mode")
            pivot = sprite.get_editor_property("custom_pivot_point")
            check(abs(pivot.x - (r["x"] - t["x"] + p["x"])) < 1e-3 and abs(pivot.y - (r["y"] - t["y"] + p["y"])) < 1e-3, "%s %d pivot" % (want["animation"], i))
            tex = sprite.get_editor_property("source_texture")
            page = package["pages"][f["page"]]
            check(tex.get_name().endswith("_%02d" % (f["page"] + 1)), "page")
            check(tex.blueprint_get_size_x() == page["w"] and tex.blueprint_get_size_y() == page["h"], "texture size")
            check(tex.get_editor_property("filter") == unreal.TextureFilter.TF_NEAREST, "filter")
            check(tex.get_editor_property("mip_gen_settings") == unreal.TextureMipGenSettings.TMGS_NO_MIPMAPS, "mips")
            dframe = clip.get_editor_property("frames")[i]
            check([m.get_editor_property("name") for m in dframe.get_editor_property("markers")] == [e["name"] for e in f["events"]], "markers")
            check(len(dframe.get_editor_property("regions")) == len(f["boxes"]), "regions")
    print("VERIFY %s %s: %d clips, %d frames, %d problems" % ("OK" if not problems else "FAIL", root, len(clips), frames, len(problems)))
    for p in problems[:20]:
        print("VERIFY PROBLEM " + p)

args = sys.argv[1:]
for i in range(0, len(args), 2):
    verify(args[i], args[i + 1])
