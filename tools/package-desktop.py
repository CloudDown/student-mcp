#!/usr/bin/env python3
"""Build one universal desktop installer per OS: student-mcp-setup.exe / student-mcp.dmg."""
import argparse
import hashlib
import json
import os
import pathlib
import plistlib
import shutil
import subprocess
import struct
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICON = ROOT / "src/resources/assets/app-icon.png"


def png_icon(output, kind):
    """Wrap the bundled 256px PNG in a native Windows or macOS icon container."""
    image = ICON.read_bytes()
    if kind == "ico":
        header = struct.pack("<HHHBBBBHHII", 0, 1, 1, 0, 0, 0, 0, 1, 32, len(image), 22)
        output.write_bytes(header + image)
    else:
        output.write_bytes(b"icns" + struct.pack(">I", len(image) + 16) + b"ic08" +
                           struct.pack(">I", len(image) + 8) + image)


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


def windows_sign(path):
    thumbprint = os.environ.get("WINDOWS_SIGNING_THUMBPRINT")
    if not thumbprint:
        return
    tool = os.environ.get("SIGNTOOL", "signtool")
    subprocess.run([tool, "sign", "/sha1", thumbprint, "/s", "My", "/fd", "SHA256",
                    "/tr", "http://timestamp.digicert.com", "/td", "SHA256", str(path)], check=True)
    subprocess.run([tool, "verify", "/pa", str(path)], check=True)


def macos_sign(path):
    identity = os.environ.get("MACOS_SIGNING_IDENTITY", "-")
    options = ["--options", "runtime", "--timestamp"] if identity != "-" else []
    subprocess.run(["codesign", "--force", "--sign", identity, *options, str(path)], check=True)


def notarize(path):
    profile = os.environ.get("MACOS_NOTARY_PROFILE")
    if not profile:
        return
    if not os.environ.get("MACOS_SIGNING_IDENTITY"):
        raise ValueError("Notarization requires MACOS_SIGNING_IDENTITY")
    with tempfile.TemporaryDirectory(prefix="student-notary-") as temporary:
        submission = path
        if path.suffix == ".app":
            submission = pathlib.Path(temporary) / "submission.zip"
            subprocess.run(["ditto", "-c", "-k", "--keepParent", str(path), str(submission)], check=True)
        result = subprocess.run(["xcrun", "notarytool", "submit", str(submission),
                                 "--keychain-profile", profile, "--wait", "--output-format", "json"],
                                check=True, capture_output=True, text=True)
        if json.loads(result.stdout).get("status") != "Accepted":
            raise RuntimeError("Apple notarization was not accepted; no artifact will be published")
    subprocess.run(["xcrun", "stapler", "staple", str(path)], check=True)
    subprocess.run(["xcrun", "stapler", "validate", str(path)], check=True)


def macos_bundle(directory, binary, launcher, version):
    app = directory / "student-mcp.app"
    contents = app / "Contents"
    executables = contents / "MacOS"
    executables.mkdir(parents=True)
    resources = contents / "Resources"
    resources.mkdir()
    png_icon(resources / "student-mcp.icns", "icns")
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
            "CFBundleIconFile": "student-mcp.icns",
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
            str(ROOT / "tools/packaging/install-macos.applescript"),
        ], check=True)
        resources = installer / "Contents/Resources"
        shutil.copytree(payload, resources / payload.name)
        shutil.copy2(ROOT / "tools/packaging/install-macos.sh", resources)
        # osacompile supplies a generic applet ID; give the installer its own ID.
        plist_path = installer / "Contents/Info.plist"
        with plist_path.open("rb") as source:
            info = plistlib.load(source)
        info.update(CFBundleIdentifier="com.clouddown.student-mcp.installer",
                    CFBundleShortVersionString=version, CFBundleVersion=version)
        with plist_path.open("wb") as destination:
            plistlib.dump(info, destination)
        # Sign inside out; absence of credentials retains the ad-hoc build path.
        for executable in sorted((resources / payload.name / "Contents/MacOS").iterdir()):
            macos_sign(executable)
        for bundle in (resources / payload.name, installer):
            macos_sign(bundle)
            notarize(bundle)
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(installer)], check=True)
        (image_root / "INSTALL.txt").write_text(
            "Double-click Install student-mcp, then choose Install and open.\n"
            "The app is copied to /Applications/student-mcp.app and opened.\n"
            "Quit an existing copy from its menu-bar icon before upgrading.\n"
            + ("Developer ID signed and notarized.\n" if os.environ.get("MACOS_NOTARY_PROFILE")
               else "Not notarized by Apple.\n"), encoding="utf-8"
        )
        subprocess.run([
            "hdiutil", "create", "-volname", "student-mcp", "-srcfolder", str(image_root),
            "-format", "UDZO", "-ov", str(artifact),
        ], check=True)
        if os.environ.get("MACOS_SIGNING_IDENTITY"):
            macos_sign(artifact)
        notarize(artifact)
    return artifact


def package_windows(args, version, output):
    for architecture in ("x64", "arm64"):
        for kind in ("binary", "launcher"):
            windows_sign(getattr(args, f"{architecture}_{kind}"))
    compiler = args.iscc or str(
        pathlib.Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
        / "Inno Setup 6/ISCC.exe"
    )
    with tempfile.TemporaryDirectory(prefix="student-mcp-icon-") as temporary:
        icon = pathlib.Path(temporary) / "student-mcp.ico"
        png_icon(icon, "ico")
        subprocess.run([
            compiler,
            f"/DAppVersion={version}", f"/DX64Binary={args.x64_binary}",
            f"/DX64Launcher={args.x64_launcher}", f"/DArm64Binary={args.arm64_binary}",
            f"/DArm64Launcher={args.arm64_launcher}", f"/DOutputDir={output}",
            f"/DAppIcon={icon}", str(ROOT / "tools/packaging/windows.iss"),
        ], check=True)
    artifact = output / "student-mcp-setup.exe"
    windows_sign(artifact)
    return artifact


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
