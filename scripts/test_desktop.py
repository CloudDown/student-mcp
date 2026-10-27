"""Offline bundle/packaging contracts; native tools are exercised by release CI."""
import importlib.util
import os
import pathlib
import plistlib
import shutil
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("desktop", ROOT / "scripts/package-desktop.py")
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)


class DesktopTests(unittest.TestCase):
    def fixtures(self, root, label):
        binary = root / "student-mcp"
        launcher = root / "student-mcp-desktop"
        binary.write_bytes(b"cli-fixture")
        launcher.write_bytes(b"launcher-fixture")
        output = root / "release"
        output.mkdir()
        return SimpleNamespace(label=label, binary=binary, launcher=launcher, iscc="ISCC"), output

    def test_macos_bundle_keeps_stdio_binary_separate_from_desktop_entry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            args, _ = self.fixtures(root, "macos-arm64")
            app = desktop.macos_bundle(root, args.binary, args.launcher, "0.3.0")
            with (app / "Contents/Info.plist").open("rb") as source:
                info = plistlib.load(source)
            self.assertEqual(info["CFBundleExecutable"], "student-mcp-desktop")
            self.assertEqual(info["CFBundleIdentifier"], "com.clouddown.student-mcp")
            self.assertEqual(info["CFBundleShortVersionString"], "0.3.0")
            for original in (args.binary, args.launcher):
                installed = app / "Contents/MacOS" / original.name
                self.assertEqual(installed.read_bytes(), original.read_bytes())
                if os.name == "posix":
                    self.assertEqual(installed.stat().st_mode & 0o777, 0o755)

    def test_dmg_contains_installer_and_embedded_app_and_is_checksummed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            args, output = self.fixtures(root, "macos-arm64")
            calls = []

            def native_tool(command, **kwargs):
                self.assertTrue(kwargs["check"])
                calls.append(command)
                if command[0] == "osacompile":
                    contents = pathlib.Path(command[2]) / "Contents"
                    (contents / "Resources").mkdir(parents=True)
                    with (contents / "Info.plist").open("wb") as destination:
                        plistlib.dump({}, destination)
                elif command[0] == "hdiutil":
                    image_root = pathlib.Path(command[command.index("-srcfolder") + 1])
                    installer = image_root / "Install student-mcp.app"
                    resources = installer / "Contents/Resources"
                    payload = resources / "student-mcp.app/Contents/MacOS"
                    self.assertEqual((payload / "student-mcp").read_bytes(), b"cli-fixture")
                    self.assertEqual((payload / "student-mcp-desktop").read_bytes(), b"launcher-fixture")
                    self.assertTrue((resources / "install-macos.sh").is_file())
                    with (installer / "Contents/Info.plist").open("rb") as source:
                        self.assertEqual(plistlib.load(source)["CFBundleIdentifier"],
                                         "com.clouddown.student-mcp.installer")
                    pathlib.Path(command[-1]).write_bytes(b"dmg-fixture")

            with mock.patch.object(desktop.subprocess, "run", side_effect=native_tool):
                artifact = desktop.package_macos(args, "0.3.0", output)
            self.assertEqual(artifact.name, "student-mcp-macos-arm64.dmg")
            signatures = [command for command in calls if command[:2] == ["codesign", "--force"]]
            self.assertEqual(len(signatures), 4)  # binaries, inner app, installer app
            self.assertEqual(signatures[-1][-1].split("/")[-1], "Install student-mcp.app")
            self.assertTrue(any(command[:2] == ["codesign", "--verify"] for command in calls))
            desktop.checksum(artifact)
            self.assertEqual(artifact.with_name(artifact.name + ".sha256").read_text(),
                             f"{desktop.hashlib.sha256(b'dmg-fixture').hexdigest()}  {artifact.name}\n")

    def test_windows_uses_correct_architecture_and_both_payloads(self):
        for label, architecture in (("windows-x64", "x64compatible"), ("windows-arm64", "arm64")):
            with self.subTest(label=label), tempfile.TemporaryDirectory() as directory:
                args, output = self.fixtures(pathlib.Path(directory), label)
                with mock.patch.object(desktop.subprocess, "run") as run:
                    artifact = desktop.package_windows(args, "0.3.0", output)
                command = run.call_args.args[0]
                self.assertEqual(command[0], "ISCC")
                for definition in (f"/DArchitecture={architecture}", "/DAppVersion=0.3.0",
                                   f"/DBinary={args.binary}", f"/DLauncher={args.launcher}"):
                    self.assertIn(definition, command)
                self.assertTrue(run.call_args.kwargs["check"])
                self.assertEqual(artifact.name, f"student-mcp-{label}-setup.exe")

    def test_failed_native_packaging_does_not_emit_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            args, output = self.fixtures(root, "macos-x64")
            with mock.patch.object(desktop.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "osacompile")):
                with self.assertRaises(subprocess.CalledProcessError):
                    desktop.package_macos(args, "0.3.0", output)
            self.assertEqual(list(output.iterdir()), [])

    def test_missing_payload_is_refused_before_packaging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            args, output = self.fixtures(root, "windows-x64")
            result = subprocess.run([
                os.sys.executable, str(ROOT / "scripts/package-desktop.py"),
                "--label", args.label, "--binary", str(args.binary),
                "--launcher", str(root / "missing"), "--output", str(output),
            ], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"Missing launcher", result.stderr)
            self.assertEqual(list(output.iterdir()), [])

    @unittest.skipUnless(os.name == "posix" and shutil.which("rustc"), "Requires Unix and rustc")
    def test_real_launcher_resolves_adjacent_binary_and_passes_up(self):
        with tempfile.TemporaryDirectory(prefix="student-mcp test ") as directory:
            root = pathlib.Path(directory)
            launcher = root / "student-mcp-desktop"
            subprocess.run(["rustc", "--edition=2021", str(ROOT / "src/bin/student-mcp-desktop.rs"),
                            "-o", str(launcher)], capture_output=True, check=True)
            binary = root / "student-mcp"
            binary.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n', encoding="utf-8")
            binary.chmod(0o755)
            result = subprocess.run([str(launcher)], cwd=ROOT, capture_output=True, check=True)
            self.assertEqual(result.stdout, b"up\n")
            binary.unlink()
            result = subprocess.run([str(launcher)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"Could not open student-mcp", result.stderr)


if __name__ == "__main__":
    unittest.main()
