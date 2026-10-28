"""Offline checks for raw binary release assets (no ZIPs)."""
import hashlib
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent


class StageReleaseTests(unittest.TestCase):
    def test_raw_binary_and_checksum_are_staged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            source = root / "input"
            source.mkdir()
            binary = source / "student-mcp"
            binary.write_bytes(b"app-with-embedded-tunnel-fixture")
            command = [
                sys.executable, str(ROOT / "stage-release.py"),
                "--binary", str(binary), "--name", "student-mcp-linux-x64",
            ]
            subprocess.run(command, cwd=root, capture_output=True, check=True)
            staged = root / "release/student-mcp-linux-x64"
            self.assertEqual(staged.read_bytes(), binary.read_bytes())
            if os.name == "posix":
                self.assertEqual(staged.stat().st_mode & 0o777, 0o755)
            checksum = staged.with_name(staged.name + ".sha256").read_text().split()[0]
            self.assertEqual(hashlib.sha256(staged.read_bytes()).hexdigest(), checksum)
            self.assertEqual(
                sorted(path.name for path in (root / "release").iterdir()),
                ["student-mcp-linux-x64", "student-mcp-linux-x64.sha256"],
            )

    def test_missing_binary_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            result = subprocess.run([
                sys.executable, str(ROOT / "stage-release.py"),
                "--binary", str(root / "missing"), "--name", "student-mcp-linux-x64",
            ], cwd=root, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "release/student-mcp-linux-x64").exists())

    def test_windows_powershell_assets_have_checksums_for_both_architectures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for arch in ("x64", "arm64"):
                for name in ("student-mcp", "student-mcp-desktop"):
                    asset = f"{name}-windows-{arch}.exe"
                    source = root / f"{asset}.input"
                    source.write_bytes(asset.encode())
                    subprocess.run([
                        sys.executable, str(ROOT / "stage-release.py"),
                        "--binary", str(source), "--name", asset,
                    ], cwd=root, check=True, capture_output=True)
                    staged = root / "release" / asset
                    self.assertEqual(staged.read_bytes(), source.read_bytes())
                    checksum = (root / "release" / f"{asset}.sha256").read_text()
                    self.assertEqual(checksum, f"{hashlib.sha256(source.read_bytes()).hexdigest()}  {asset}\n")

    def test_asset_name_cannot_escape_the_output_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            binary = root / "student-mcp"
            binary.write_bytes(b"fixture")
            result = subprocess.run([
                sys.executable, str(ROOT / "stage-release.py"),
                "--binary", str(binary), "--name", "../student-mcp",
            ], cwd=root, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"plain file name", result.stderr)
            self.assertEqual(binary.read_bytes(), b"fixture")


if __name__ == "__main__":
    unittest.main()
