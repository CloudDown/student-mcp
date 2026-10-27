#!/usr/bin/env python3
"""Build one universal desktop installer per OS: student-mcp-setup.exe / student-mcp.dmg."""
import argparse
import hashlib
import os
import pathlib
import plistlib
import shutil
import subprocess
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def checksum(artifact):
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    artifact.with_name(artifact.name + ".sha256").write_text(
        f"{digest}  {artifact.name}\n", encoding="utf-8"
    )


def lipo(output, *inputs):
    """Combine x64 and arm64 slices; the result runs on both."""
    subprocess.run(
        ["lipo", "-create", *(str(source) for source in inputs), "-output", str(output)],
        check=True,
    )
    return output


def macos_bundle(directory, binary, launcher, version):
    app = directory / "student-mcp.app"
    contents = app / "Contents"
    executables = contents / "MacOS"
    executables.mkdir(parents=True)
    for source, name in ((binary, "student-mcp"), (launcher, "student-mcp-desktop")):
        destination = executables / name
        shutil.copy2(source, destination)
        destination.chmod(0o755)
    with (contents / "Info.plist").open("wb") as output:
        plistlib.dump({
            "CFBundleName": "student-mcp",
            "CFBundleDisplayName": "student-mcp",
            "CFBundleIdentifier": "com.clouddown.student-mcp",
            "CFBundleExecutable": "student-mcp-desktop",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": version,
            "CFBundleVersion": version,
            "LSMinimumSystemVersion": "11.0",
            "NSHighResolutionCapable": True,
        }, output)
    return app


def package_macos(args, version, output):
    artifact = output / "student-mcp.dmg"
    with tempfile.TemporaryDirectory(prefix="student-mcp-desktop-") as temporary:
        directory = pathlib.Path(temporary)
        merged = directory / "universal"
        merged.mkdir()
        binary = lipo(merged / "student-mcp", args.x64_binary, args.arm64_binary)
        launcher = lipo(
            merged / "student-mcp-desktop", args.x64_launcher, args.arm64_launcher
        )
        payload = macos_bundle(directory, binary, launcher, version)
        image_root = directory / "image"
        image_root.mkdir()
        installer = image_root / "Install student-mcp.app"
        subprocess.run([
            "osacompile", "-o", str(installer),
            str(ROOT / "packaging/install-macos.applescript"),
        ], check=True)
        resources = installer / "Contents/Resources"
        shutil.copytree(payload, resources / payload.name)
        shutil.copy2(ROOT / "packaging/install-macos.sh", resources)
        # osacompile supplies a generic applet ID; give the installer its own ID.
        plist_path = installer / "Contents/Info.plist"
        with plist_path.open("rb") as source:
            info = plistlib.load(source)
        info.update(CFBundleIdentifier="com.clouddown.student-mcp.installer",
                    CFBundleShortVersionString=version, CFBundleVersion=version)
        with plist_path.open("wb") as destination:
            plistlib.dump(info, destination)
        # Apple Silicon requires valid code signatures, even for unsigned releases.
        # These ad-hoc signatures are NOT Developer ID signing or notarization.
        for executable in sorted((resources / payload.name / "Contents/MacOS").iterdir()):
            subprocess.run(["codesign", "--force", "--sign", "-", str(executable)], check=True)
        for bundle in (resources / payload.name, installer):
            subprocess.run(["codesign", "--force", "--sign", "-", str(bundle)], check=True)
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(installer)], check=True)
        (image_root / "INSTALL.txt").write_text(
            "Double-click Install student-mcp, then choose Install and open.\n"
            "The app is copied to /Applications/student-mcp.app and opened.\n"
            "Quit an existing copy from its menu-bar icon before upgrading.\n"
            "This release is not Developer ID signed or notarized.\n", encoding="utf-8"
        )
        subprocess.run([
            "hdiutil", "create", "-volname", "student-mcp", "-srcfolder", str(image_root),
            "-format", "UDZO", "-ov", str(artifact),
        ], check=True)
    return artifact


def package_windows(args, version, output):
    compiler = args.iscc or str(
        pathlib.Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
        / "Inno Setup 6/ISCC.exe"
    )
    subprocess.run([
        compiler,
        f"/DAppVersion={version}", f"/DX64Binary={args.x64_binary}",
        f"/DX64Launcher={args.x64_launcher}", f"/DArm64Binary={args.arm64_binary}",
        f"/DArm64Launcher={args.arm64_launcher}", f"/DOutputDir={output}",
        str(ROOT / "packaging/windows.iss"),
    ], check=True)
    return output / "student-mcp-setup.exe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=("windows", "macos"))
    for architecture in ("x64", "arm64"):
        parser.add_argument(f"--{architecture}-binary", required=True, type=pathlib.Path)
        parser.add_argument(f"--{architecture}-launcher", required=True, type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("release"))
    parser.add_argument("--iscc", help="Path to the Inno Setup 6 compiler (Windows only)")
    args = parser.parse_args()
    for architecture in ("x64", "arm64"):
        for kind in ("binary", "launcher"):
            path = getattr(args, f"{architecture}_{kind}").resolve()
            if not path.is_file():
                parser.error(f"Missing {architecture} {kind}: {path}")
            setattr(args, f"{architecture}_{kind}", path)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with (ROOT / "Cargo.toml").open("rb") as source:
        version = tomllib.load(source)["package"]["version"]
    package = package_windows if args.platform == "windows" else package_macos
    checksum(package(args, version, output))


if __name__ == "__main__":
    main()
