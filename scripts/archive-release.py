"""Create an easy-to-download, portable ZIP for one GitHub release target."""

import hashlib
import os
import stat
import sys
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


def main() -> None:
    label = sys.argv[1] if len(sys.argv) > 1 else ""
    if label not in {
        "windows-x64", "windows-arm64", "macos-arm64", "macos-x64",
        "linux-x64", "linux-arm64",
    }:
        raise SystemExit("Usage: python scripts/archive-release.py <platform-arch>")

    root = Path(__file__).resolve().parent.parent
    binary = root / "build" / ("student-mcp.exe" if label.startswith("windows") else "student-mcp")
    if not binary.is_file():
        raise SystemExit(f"Missing build artifact: {binary}")
    installer = "Installer.cmd" if label.startswith("windows") else "Installer.command" if label.startswith("macos") else "Installer.sh"
    outdir = root / "release"
    outdir.mkdir(exist_ok=True)
    archive = outdir / f"student-mcp-{label}.zip"

    with ZipFile(archive, "w", ZIP_DEFLATED, compresslevel=9) as zip_file:
        for source, name in [
            (binary, binary.name),
            (root / "installers" / installer, installer),
            (root / "README.md", "README.md"),
        ]:
            info = ZipInfo(name)
            info.create_system = 3
            executable = not label.startswith("windows") and name != "README.md"
            info.external_attr = (stat.S_IFREG | (0o755 if executable else 0o644)) << 16
            info.compress_type = ZIP_DEFLATED
            zip_file.writestr(info, source.read_bytes())

    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (outdir / f"{archive.name}.sha256").write_text(f"{digest}  {archive.name}\n", encoding="utf8")
    print(f"Created {archive.name} ({os.path.getsize(archive):,} bytes)")


if __name__ == "__main__":
    main()
