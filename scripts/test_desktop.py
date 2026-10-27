"""Offline universal installer contracts; native tools are exercised by release CI."""
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
with (ROOT / "Cargo.toml").open("rb") as source:
    VERSION = desktop.tomllib.load(source)["package"]["version"]


class DesktopTests(unittest.TestCase):
    def fixtures(self, root):
        paths = {}
        for architecture in ("x64", "arm64"):
            for kind in ("binary", "launcher"):
                path = root / f"{architecture}-{kind}"
                path.write_bytes(f"{architecture}-{kind}-fixture".encode())
                paths[f"{architecture}_{kind}"] = path
        output = root / "release"
        output.mkdir()
        return SimpleNamespace(**paths, iscc="ISCC"), output

    def test_macos_dmg_is_one_file_with_universal_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            args, output = self.fixtures(root)
            calls = []

            def native_tool(command, **kwargs):
                self.assertTrue(kwargs["check"])
                calls.append(command)
                if command[0] == "lipo":
                    destination = pathlib.Path(command[command.index("-output") + 1])
                    inputs = [pathlib.Path(value) for value in command[2:command.index("-output")]]
                    destination.write_bytes(b"+".join(path.read_bytes() for path in inputs))
                elif command[0] == "osacompile":
                    contents = pathlib.Path(command[2]) / "Contents"
                    (contents / "Resources").mkdir(parents=True)
                    with (contents / "Info.plist").open("wb") as destination:
                        plistlib.dump({}, destination)
                elif command[0] == "hdiutil":
                    image_root = pathlib.Path(command[command.index("-srcfolder") + 1])
                    installer = image_root / "Install student-mcp.app"
                    resources = installer / "Contents/Resources"
                    payload = resources / "student-mcp.app/Contents/MacOS"
                    self.assertEqual(
                        (payload / "student-mcp").read_bytes(),
                        b"x64-binary-fixture+arm64-binary-fixture",
                    )
                    self.assertEqual(
                        (payload / "student-mcp-desktop").read_bytes(),
                        b"x64-launcher-fixture+arm64-launcher-fixture",
                    )
                    self.assertTrue((resources / "install-macos.sh").is_file())
                    with (installer / "Contents/Info.plist").open("rb") as source:
                        self.assertEqual(
                            plistlib.load(source)["CFBundleIdentifier"],
                            "com.clouddown.student-mcp.installer",
                        )
                    pathlib.Path(command[-1]).write_bytes(b"dmg-fixture")

            with mock.patch.object(desktop.subprocess, "run", side_effect=native_tool):
                artifact = desktop.package_macos(args, VERSION, output)
            self.assertEqual(artifact.name, "student-mcp.dmg")
            self.assertEqual(
                [command[:2] for command in calls if command[0] == "lipo"],
                [["lipo", "-create"], ["lipo", "-create"]],
            )
            signatures = [command for command in calls if command[:2] == ["codesign", "--force"]]
            self.assertEqual(len(signatures), 4)  # two binaries, inner app, installer app
            self.assertEqual(signatures[-1][-1].split("/")[-1], "Install student-mcp.app")
            self.assertTrue(any(command[:2] == ["codesign", "--verify"] for command in calls))
            desktop.checksum(artifact)
            self.assertEqual(
                artifact.with_name(artifact.name + ".sha256").read_text(),
                f"{desktop.hashlib.sha256(b'dmg-fixture').hexdigest()}  {artifact.name}\n",
            )
            self.assertEqual(sorted(path.name for path in output.iterdir()),
                             ["student-mcp.dmg", "student-mcp.dmg.sha256"])

    def test_windows_installer_gets_both_architectures_and_no_label_choice(self):
        with tempfile.TemporaryDirectory() as directory:
            args, output = self.fixtures(pathlib.Path(directory))
            with mock.patch.object(desktop.subprocess, "run") as run:
                artifact = desktop.package_windows(args, VERSION, output)
            command = run.call_args.args[0]
            self.assertEqual(command[0], "ISCC")
            self.assertFalse(any(
                definition.startswith(("/DArchitecture", "/DLabel"))
                for definition in command
            ))
            for definition in (
                f"/DAppVersion={VERSION}",
                f"/DX64Binary={args.x64_binary}",
                f"/DX64Launcher={args.x64_launcher}",
                f"/DArm64Binary={args.arm64_binary}",
                f"/DArm64Launcher={args.arm64_launcher}",
                f"/DOutputDir={output}",
            ):
                self.assertIn(definition, command)
            self.assertTrue(run.call_args.kwargs["check"])
            self.assertEqual(artifact.name, "student-mcp-setup.exe")

    def test_failed_native_packaging_does_not_emit_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            args, output = self.fixtures(root)
            with mock.patch.object(
                desktop.subprocess, "run",
                side_effect=subprocess.CalledProcessError(1, "lipo"),
            ):
                with self.assertRaises(subprocess.CalledProcessError):
                    desktop.package_macos(args, VERSION, output)
            self.assertEqual(list(output.iterdir()), [])

    def test_missing_payload_is_refused_before_packaging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            args, output = self.fixtures(root)
            result = subprocess.run([
                os.sys.executable, str(ROOT / "scripts/package-desktop.py"), "windows",
                "--x64-binary", str(args.x64_binary), "--x64-launcher", str(args.x64_launcher),
                "--arm64-binary", str(args.arm64_binary),
                "--arm64-launcher", str(root / "missing"), "--output", str(output),
            ], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"Missing arm64 launcher", result.stderr)
            self.assertEqual(list(output.iterdir()), [])

    @unittest.skipUnless(os.name == "posix" and shutil.which("rustc"), "Requires Unix and rustc")
    def test_real_launcher_resolves_adjacent_binary_and_passes_up(self):
        with tempfile.TemporaryDirectory(prefix="student-mcp test ") as directory:
            root = pathlib.Path(directory)
            launcher = root / "student-mcp-desktop"
            subprocess.run([
                "rustc", "--edition=2021", str(ROOT / "src/bin/student-mcp-desktop.rs"),
                "-o", str(launcher),
            ], capture_output=True, check=True)
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
