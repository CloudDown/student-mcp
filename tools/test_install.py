"""Exercise the Linux terminal install with local download fixtures, no network."""
import hashlib
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent


@unittest.skipUnless(os.name == "posix", "Requires a POSIX shell")
class InstallTests(unittest.TestCase):
    def run_fixture(self, valid_checksum, compatible=True):
        with tempfile.TemporaryDirectory(prefix="student-mcp install ") as directory:
            root = pathlib.Path(directory)
            tools = root / "tools"
            tools.mkdir()
            home = root / "home"
            home.mkdir()
            asset = root / "student-mcp-linux-x64"
            asset.write_text('#!/bin/sh\nprintf "fixture CLI %s\\n" "$1"\n' if compatible
                             else '#!/bin/sh\necho "incompatible system loader" >&2\nexit 126\n')
            installed = home / ".student-mcp/bin/student-mcp"
            if not compatible:
                installed.parent.mkdir(parents=True)
                installed.write_bytes(b"existing application")
            digest = hashlib.sha256(asset.read_bytes()).hexdigest() if valid_checksum else "0" * 64
            asset.with_name(asset.name + ".sha256").write_text(f"{digest}  {asset.name}\n")
            (tools / "uname").write_text(
                '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n'
            )
            # curl's URL and -o destination are its last three arguments.
            (tools / "curl").write_text(
                '#!/bin/bash\nset -euo pipefail\n'
                'while [ "$#" -gt 3 ]; do shift; done\n'
                'name="${1##*/}"\n'
                'printf "%s\\n" "$name" >> "$FIXTURE_ROOT/downloads"\n'
                'cp "$FIXTURE_ROOT/$name" "$3"\n'
            )
            for tool in tools.iterdir():
                tool.chmod(0o755)
            environment = dict(os.environ, HOME=str(home), FIXTURE_ROOT=str(root),
                               MOODLE_INSTALL_CHECK_ONLY="1",
                               PATH=f"{tools}{os.pathsep}{os.environ['PATH']}")
            result = subprocess.run(["bash", str(ROOT / "tools/packaging/install.sh")], env=environment,
                                    capture_output=True, timeout=10)
            self.assertEqual((root / "downloads").read_text().splitlines(),
                             [asset.name, asset.name + ".sha256"])
            if compatible:
                self.assertFalse((home / ".student-mcp").exists())
            else:
                self.assertEqual(installed.read_bytes(), b"existing application")
            return result

    def test_raw_download_is_verified_and_runs_without_unzip(self):
        result = self.run_fixture(valid_checksum=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertIn(b"fixture CLI --help", result.stdout)

    def test_bad_checksum_never_executes_download(self):
        result = self.run_fixture(valid_checksum=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"does not match its SHA-256", result.stderr)
        self.assertNotIn(b"fixture CLI", result.stdout)

    def test_incompatible_download_preserves_existing_app(self):
        result = self.run_fixture(valid_checksum=True, compatible=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"incompatible system loader", result.stderr)
        self.assertIn(b"existing installation was not changed", result.stderr)


if __name__ == "__main__":
    unittest.main()
