#!/usr/bin/env python3
"""Package a release: student-mcp-<label>.zip + .sha256, from a cargo binary."""
import argparse
import hashlib
import pathlib
import sys
import zipfile


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--label", required=True)
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    args = parser.parse_args()

    binary = args.binary.resolve()
    if not binary.is_file():
        print(f"Binary not found: {binary}", file=sys.stderr)
        return 1

    staging = pathlib.Path("release")
    staging.mkdir(exist_ok=True)
    entry = "student-mcp.exe" if binary.suffix == ".exe" else "student-mcp"
    archive = staging / f"student-mcp-{args.label}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
        bundle.write(binary, arcname=entry)

    checksum = staging / f"{archive.name}.sha256"
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")

    print(f"{archive.name}: {archive.stat().st_size} bytes, sha256={digest[:16]}…")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
