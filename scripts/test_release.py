"""Offline checks for the bundled tunnel supply chain and release contents."""
import hashlib
import importlib.util
import io
import pathlib
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("fetch_cloudflared", ROOT / "fetch-cloudflared.py")
fetch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fetch)


class ReleaseTests(unittest.TestCase):
    def test_download_rejects_wrong_checksum(self):
        with unittest.mock.patch.object(fetch.urllib.request, "urlopen", return_value=io.BytesIO(b"tampered")):
            with self.assertRaises(ValueError):
                fetch.verified_download("https://example.org/binary", "0" * 64)
        with unittest.mock.patch.object(fetch.urllib.request, "urlopen", return_value=io.BytesIO(b"verified")):
            self.assertEqual(fetch.verified_download("https://example.org/binary", hashlib.sha256(b"verified").hexdigest()), b"verified")

    def test_all_six_release_platforms_have_pinned_assets(self):
        self.assertEqual(set(fetch.ASSETS), {"linux-x64", "linux-arm64", "macos-x64", "macos-arm64", "windows-x64", "windows-arm64"})
        for _, digest in fetch.ASSETS.values():
            self.assertEqual(len(bytes.fromhex(digest)), 32)

    def test_package_requires_sidecar_and_includes_it_in_checksum(self):
        for suffix in ["", ".exe"]:
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                binary = root / f"student-mcp{suffix}"
                binary.write_bytes(b"app-fixture")
                sidecar = root / "sidecar"
                sidecar.mkdir()
                command = [sys.executable, str(ROOT / "archive-release.py"), "--label", "fixture", "--binary", str(binary), "--sidecar-dir", str(sidecar)]
                result = subprocess.run(command, cwd=root, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((root / "release/student-mcp-fixture.zip").exists())
                (sidecar / f"cloudflared{suffix}").write_bytes(b"tunnel-fixture")
                (sidecar / "LICENSE-cloudflared.txt").write_text("license-fixture")
                subprocess.run(command, cwd=root, capture_output=True, check=True)
                archive = root / "release/student-mcp-fixture.zip"
                with zipfile.ZipFile(archive) as bundle:
                    self.assertEqual(set(bundle.namelist()), {f"student-mcp{suffix}", f"cloudflared{suffix}", "LICENSE-cloudflared.txt"})
                    self.assertEqual(bundle.read(f"cloudflared{suffix}"), b"tunnel-fixture")
                checksum = archive.with_name(archive.name + ".sha256").read_text().split()[0]
                self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), checksum)


if __name__ == "__main__":
    unittest.main()
