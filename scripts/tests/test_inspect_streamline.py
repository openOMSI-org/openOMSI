import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

SPEC = importlib.util.spec_from_file_location("inspect_streamline", Path(__file__).parents[1] / "inspect-streamline.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ArchiveTests(unittest.TestCase):
    def report(self, entries):
        target = Path(__file__).parents[2] / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=target) as folder:
            path = Path(folder) / "sdk.zip"
            with zipfile.ZipFile(path, "w") as archive:
                for name, data in entries.items():
                    archive.writestr(name, data)
            return MODULE.inspect_archive(path)

    def test_declared_feature_is_not_a_distributed_runtime(self):
        report = self.report({"include/sl_core_types.h": "constexpr Feature kFeatureDLSS_NR = 1004;", "bin/x64/sl.dlss.dll": "fixture"})
        self.assertTrue(report["feature_declared"])
        self.assertFalse(report["candidate_files_present"])
        self.assertFalse(report["runtime_validated"])

    def test_header_and_dll_presence_still_do_not_certify_execution(self):
        report = self.report({"include/sl_dlss_nr.h": "fixture", "bin/x64/sl.dlss_nr.dll": "fixture"})
        self.assertTrue(report["candidate_files_present"])
        self.assertFalse(report["runtime_validated"])

    def test_other_dlss_features_are_not_neural_rendering(self):
        report = self.report({"include/sl_dlss_d.h": "fixture", "bin/x64/sl.dlss_g.dll": "fixture", "bin/x64/sl.deepdvc.dll": "fixture"})
        self.assertFalse(report["feature_declared"])
        self.assertFalse(report["candidate_files_present"])


if __name__ == "__main__":
    unittest.main()
