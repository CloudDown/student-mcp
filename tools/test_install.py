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
    def run_fixture(self, valid_checksum, compatible=True, install=False, custom_data=False,
                    legacy=False, in_path=False, conflict=False, upgrade=False):
        with tempfile.TemporaryDirectory(prefix="student-mcp install ") as directory:
            root = pathlib.Path(directory)
            tools = root / "tools"
            tools.mkdir()
            home = root / "home"
            home.mkdir()
            asset = root / "student-mcp-linux-x64"
            asset.write_text('#!/bin/sh\nif [ "$1" = --app-icon ]; then ' +
                             ('exit 2; ' if legacy else 'printf fixture-icon; ') +
                             'else printf "fixture CLI %s\\n" "$1"; fi\n' if compatible
                             else '#!/bin/sh\necho "incompatible system loader" >&2\nexit 126\n')
            installed = home / ".student-mcp/bin/student-mcp"
            cli = home / ".local/bin/student-mcp"
            preserved = {}
            if install:
                for relative, data in [(".student-mcp.json", b'{"accounts":[{"id":"keep","enabled":false}],"gateway":{"public_enabled":false}}'),
                                       (".student-mcp.browsers/fixture/session", b"private session"),
                                       (".student-mcp.ui-preferences", b"user preferences")]:
                    path = home / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
                    preserved[path] = data
                if conflict:
                    cli.parent.mkdir(parents=True, exist_ok=True)
                    cli.write_bytes(b"another program")
                    preserved[cli] = b"another program"
            if not compatible:
                installed.parent.mkdir(parents=True)
                installed.write_bytes(b"existing application")
            digest = hashlib.sha256(asset.read_bytes()).hexdigest() if valid_checksum else "0" * 64
            (root / "SHA256SUMS.txt").write_text(f"{digest}  {asset.name}\n")
            (root / "app-icon.png").write_bytes((ROOT / "src/resources/assets/app-icon.png").read_bytes())
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
                               PATH=f"{tools}{os.pathsep}{str(cli.parent) + os.pathsep if in_path else ''}{os.environ['PATH']}")
            environment.pop("XDG_DATA_HOME", None)
            environment.pop("MOODLE_INSTALL_CHECK_ONLY", None)
            if not install:
                environment["MOODLE_INSTALL_CHECK_ONLY"] = "1"
            if custom_data:
                environment["XDG_DATA_HOME"] = str(root / "custom data")
            result = subprocess.run(["bash", str(ROOT / "tools/packaging/install.sh")], env=environment,
                                    capture_output=True, timeout=10)
            self.assertEqual((root / "downloads").read_text().splitlines(),
                              [asset.name, "SHA256SUMS.txt"] +
                              (["app-icon.png"] if legacy and install and valid_checksum else []))
            if compatible and install and valid_checksum:
                if upgrade:
                    result = subprocess.run(["bash", str(ROOT / "tools/packaging/install.sh")], env=environment,
                                            capture_output=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr.decode())
                if not conflict:
                    self.assertTrue(cli.is_symlink())
                    self.assertEqual(cli.resolve(), installed)
                    command = "student-mcp" if in_path else str(cli)
                    version = subprocess.run([command, "--version"], env=environment, capture_output=True, check=True)
                    self.assertIn(b"fixture CLI --version", version.stdout)
                self.assertEqual(b'Add ~/.local/bin to PATH' in result.stderr, not in_path)
                self.assertIn(b"settings-url", result.stdout)
                self.assertEqual((home / ".student-mcp/window.log").stat().st_mode & 0o077, 0)
                data = pathlib.Path(environment.get("XDG_DATA_HOME", home / ".local/share"))
                launcher = data / "applications/student-mcp.desktop"
                icon = data / "icons/hicolor/256x256/apps/student-mcp.png"
                self.assertEqual(icon.read_bytes(), (root / "app-icon.png").read_bytes()
                                 if legacy else b"fixture-icon")
                self.assertIn(f'Exec="{installed}" up', launcher.read_text())
                self.assertIn("Icon=student-mcp\n", launcher.read_text())
                validator = __import__("shutil").which("desktop-file-validate")
                if validator:
                    subprocess.run([validator, str(launcher)], check=True, capture_output=True)
            elif compatible:
                self.assertFalse((home / ".student-mcp").exists())
            else:
                self.assertEqual(installed.read_bytes(), b"existing application")
            for path, data in preserved.items():
                self.assertEqual(path.read_bytes(), data)
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

    def test_install_creates_desktop_launcher_and_icon(self):
        result = self.run_fixture(valid_checksum=True, install=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())

    def test_install_respects_xdg_data_home(self):
        result = self.run_fixture(valid_checksum=True, install=True, custom_data=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())

    def test_command_is_on_existing_path_and_upgrade_preserves_settings(self):
        result = self.run_fixture(True, install=True, in_path=True, upgrade=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())

    def test_existing_unrelated_command_is_preserved(self):
        result = self.run_fixture(True, install=True, conflict=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertIn(b"was preserved", result.stderr)

    def test_install_supports_preexisting_releases_without_embedded_icon(self):
        result = self.run_fixture(valid_checksum=True, install=True, legacy=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())


if __name__ == "__main__":
    unittest.main()
