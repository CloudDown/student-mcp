#!/usr/bin/env python3
"""Fetch a pinned, checksum-verified sidecar for packaging (no runtime download)."""
import argparse
import hashlib
import io
import pathlib
import tarfile
import urllib.request

VERSION = "2026.10.3"
# SHA-256 digests published on the official GitHub release.
ASSETS = {
    "linux-x64": ("cloudflared-linux-amd64", "053b80b0a23031a038fdbf3c073f59f82b8f1f152aa67971d7383ffafc0dbab0"),
    "linux-arm64": ("cloudflared-linux-arm64", "89297ebfb9e8c99c0dc013a1140601453cf3798d8306c5591651f4a3f5210be8"),
    "macos-x64": ("cloudflared-darwin-amd64.tgz", "81b7b85f2376815c4f59b1aa323887b62e16ce8b67555c2e1577dc23f2a82000"),
    "macos-arm64": ("cloudflared-darwin-arm64.tgz", "1165d14b6f6360a3b8c283d7d1100024d6ec7cd9bc9e32519e96150645f3ec55"),
    "windows-x64": ("cloudflared-windows-amd64.exe", "76b2805253d0d6ffacb092a7acfbee99e7af6917835c0c5410e23f3ef5052da2"),
}
# Upstream has no native Windows ARM64 binary. Windows 11 ARM runs the x64 sidecar.
ASSETS["windows-arm64"] = ASSETS["windows-x64"]


def verified_download(url, digest):
    with urllib.request.urlopen(url, timeout=120) as response:
        data = response.read(100 * 1024 * 1024 + 1)
    if len(data) > 100 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != digest:
        raise ValueError(f"Download failed SHA-256 verification: {url}")
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", required=True, choices=ASSETS)
    parser.add_argument("--output", required=True, type=pathlib.Path, help="Sidecar directory")
    args = parser.parse_args()
    asset, digest = ASSETS[args.label]
    data = verified_download(
        f"https://github.com/cloudflare/cloudflared/releases/download/{VERSION}/{asset}", digest
    )
    if asset.endswith(".tgz"):
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            matches = [m for m in archive.getmembers() if m.isfile() and pathlib.PurePosixPath(m.name).name == "cloudflared"]
            if len(matches) != 1:
                raise ValueError("Expected one cloudflared binary in upstream archive")
            data = archive.extractfile(matches[0]).read()
    license_data = verified_download(
        f"https://raw.githubusercontent.com/cloudflare/cloudflared/{VERSION}/LICENSE",
        "58d1e17ffe5109a7ae296caafcadfdbe6a7d176f0bc4ab01e12a689b0499d8bd",
    )
    args.output.mkdir(parents=True, exist_ok=True)
    name = "cloudflared.exe" if args.label.startswith("windows-") else "cloudflared"
    binary = args.output / name
    binary.write_bytes(data)
    binary.chmod(0o755)
    (args.output / "LICENSE-cloudflared.txt").write_bytes(license_data)
    print(f"Verified cloudflared {VERSION} for {args.label}: {binary}")


if __name__ == "__main__":
    main()
