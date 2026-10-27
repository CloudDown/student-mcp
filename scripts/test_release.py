"""Offline checks for the single-binary release and its archive checksum."""
import hashlib
import pathlib
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent


class ReleaseTests(unittest.TestCase):
    def test_package_is_self_contained_and_checksum_matches(self):
        for suffix in ["", ".exe"]:
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                binary = root / f"student-mcp{suffix}"
                binary.write_bytes(b"app-with-embedded-tunnel-fixture")
                command = [sys.executable, str(ROOT / "archive-release.py"), "--label", "fixture", "--binary", str(binary)]
                subprocess.run(command, cwd=root, capture_output=True, check=True)
                archive = root / "release/student-mcp-fixture.zip"
                with zipfile.ZipFile(archive) as bundle:
                    self.assertEqual(set(bundle.namelist()), {f"student-mcp{suffix}"})
                    self.assertEqual(bundle.read(f"student-mcp{suffix}"), binary.read_bytes())
                checksum = archive.with_name(archive.name + ".sha256").read_text().split()[0]
                self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), checksum)

    def test_missing_binary_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            command = [sys.executable, str(ROOT / "archive-release.py"), "--label", "fixture", "--binary", str(root / "missing")]
            result = subprocess.run(command, cwd=root, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "release/student-mcp-fixture.zip").exists())


if __name__ == "__main__":
    unittest.main()
