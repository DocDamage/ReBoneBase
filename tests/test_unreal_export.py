"""Local API contract checks; does not require or simulate the Unreal FBX evaluator."""
import importlib.util
import pathlib
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

class ExportTests(unittest.TestCase):
    def test_selected_sequences_export_and_existing_files_are_preserved(self):
        self.check_export(True)

    def test_failed_exports_report_and_skip_results(self):
        self.check_export(False)

    def check_export(self, success):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            (root / 'ReBoneBaseExports').mkdir()
            existing = root / 'ReBoneBaseExports' / 'Walk.fbx'
            existing.write_text('keep', encoding='utf-8')
            class Sequence:
                def get_name(self): return 'Walk'
            tasks, messages = [], []
            def run(task):
                tasks.append(task)
                if success: pathlib.Path(task.filename).write_text('export', encoding='utf-8')
                return success
            fake = types.SimpleNamespace(AnimSequence=Sequence, Paths=types.SimpleNamespace(project_saved_dir=lambda:folder), EditorUtilityLibrary=types.SimpleNamespace(get_selected_assets=lambda:[object(),Sequence()]), AssetExportTask=types.SimpleNamespace, AnimSequenceExporterFBX=types.SimpleNamespace,FbxExportOption=types.SimpleNamespace,Exporter=types.SimpleNamespace(run_asset_export_task=run),log=messages.append,log_warning=messages.append)
            with patch.dict(sys.modules,unreal=fake):
                spec=importlib.util.spec_from_file_location('rebone_unreal_export',pathlib.Path(__file__).parents[1]/'integrations/unreal/export_animations.py')
                module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
                exported=module.export_selected()
            self.assertEqual(len(tasks),1)
            self.assertFalse(tasks[0].replace_identical)
            self.assertFalse(tasks[0].prompt)
            self.assertEqual(existing.read_text(encoding='utf-8'),'keep')
            self.assertTrue(tasks[0].filename.endswith('Walk_2.fbx'))
            self.assertEqual(len(exported),int(success))
            self.assertTrue(messages)

if __name__=='__main__': unittest.main()
