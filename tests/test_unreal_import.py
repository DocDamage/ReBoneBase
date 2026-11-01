"""Importer planning checks; the Unreal side is exercised in a real editor (docs/UNREAL_GUIDE.md)."""
import importlib.util
import json
import pathlib
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "rebone_unreal_import", pathlib.Path(__file__).parents[1] / "integrations/unreal/ReBoneImport.py")
importer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(importer)


def frame(page, x, y, w, h, tx, ty, duration, events=()):
    return {"page": page, "rect": {"x": x, "y": y, "w": w, "h": h}, "trim": {"x": tx, "y": ty},
            "source_size": {"w": 32, "h": 24}, "source_frame": 1, "source_time_ms": 0,
            "duration_ms": duration, "events": list(events), "boxes": []}


PACKAGE = {
    "format": "rebonebase-sprites", "format_version": 2, "name": "Hero Knight!",
    "frame_size": {"w": 32, "h": 24}, "pivot": {"x": 16.0, "y": 22.0},
    "pages": [{"file": "atlas-01.png", "kind": "atlas", "w": 64, "h": 64},
              {"file": "atlas-02.png", "kind": "atlas", "w": 64, "h": 64}],
    "clips": [
        {"id": "clip-01", "animation": "Walk_e", "name": "Walk", "fps": 10, "looped": True,
         "frames": [frame(0, 1, 2, 10, 12, 5, 6, 100, [{"name": "step"}]),
                    frame(0, 20, 2, 9, 12, 6, 6, 370),
                    frame(0, 1, 2, 10, 12, 5, 6, 100, [{"name": "step"}])]},
        {"id": "clip-02", "animation": "Walk_w", "name": "Walk", "fps": 10, "looped": False,
         "frames": [frame(1, 0, 0, 10, 12, 5, 6, 33), frame(0, 1, 2, 10, 12, 4, 6, 34)]},
    ],
}


class PlanTests(unittest.TestCase):
    def test_holds_are_exact_whole_runs(self):
        fps, runs = importer.flipbook_timing([100, 370, 100])
        self.assertEqual((fps, runs), (100.0, [10, 37, 10]))
        fps, runs = importer.flipbook_timing([33, 34])
        self.assertEqual((fps, runs), (1000.0, [33, 34]))
        for durations in ([83, 83, 250], [1], [16, 17, 16, 17]):
            fps, runs = importer.flipbook_timing(durations)
            self.assertEqual([round(r * 1000 / fps) for r in runs], durations)

    def test_identical_regions_share_one_sprite_but_keep_their_frames(self):
        layout = importer.plan(PACKAGE)
        self.assertEqual(layout["name"], "Hero_Knight")
        self.assertEqual(layout["root"], "/Game/ReBoneBase/Hero_Knight")
        walk, back = layout["flipbooks"]
        self.assertEqual(walk["asset"], "FB_Walk_e")
        # Frames 1 and 3 reuse one sprite; each keeps its own key frame.
        self.assertEqual(walk["sprites"][0], walk["sprites"][2])
        self.assertEqual(len(walk["sprites"]), 3)
        # Same pixels with a different trim offset is a different sprite (different pivot).
        self.assertNotEqual(back["sprites"][1], walk["sprites"][0])
        self.assertEqual(len(layout["sprites"]), 4)
        self.assertEqual(layout["textures"][1], {"file": "atlas-02.png", "asset": "T_Hero_Knight_02"})

    def test_pivots_move_from_frame_space_to_texture_space(self):
        layout = importer.plan(PACKAGE)
        first = layout["sprites"][0]
        self.assertEqual(first["offset"], (1, 2))
        self.assertEqual(first["size"], (10, 12))
        # rect origin - trim + pivot: (1 - 5 + 16, 2 - 6 + 22)
        self.assertEqual(first["pivot"], (12.0, 18.0))
        self.assertEqual(layout["sprites"][2]["texture"], 1)

    def test_plans_are_stable_for_reimport(self):
        self.assertEqual(importer.plan(PACKAGE), importer.plan(json.loads(json.dumps(PACKAGE))))
        self.assertEqual(importer.plan(PACKAGE, "/Game/Characters/Hero")["root"], "/Game/Characters/Hero")

    def test_only_supported_packages_load(self):
        with tempfile.TemporaryDirectory() as folder:
            path = pathlib.Path(folder) / "package.json"
            path.write_text(json.dumps(PACKAGE), encoding="utf-8")
            self.assertEqual(importer.load_package(folder)["name"], "Hero Knight!")
            path.write_text(json.dumps(dict(PACKAGE, format_version=3)), encoding="utf-8")
            with self.assertRaises(ValueError):
                importer.load_package(folder)
            path.write_text(json.dumps({"version": 1, "clips": []}), encoding="utf-8")
            with self.assertRaises(ValueError):
                importer.load_package(folder)


if __name__ == "__main__":
    unittest.main()
