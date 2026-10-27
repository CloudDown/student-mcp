#!/usr/bin/env python3
"""Stage a raw release binary and its SHA-256 sidecar (no archive)."""
import argparse
import hashlib
import pathlib
import shutil


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--name", required=True)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("release"))
    args = parser.parse_args()

    if not args.name or args.name in (".", "..") or pathlib.PurePath(args.name).name != args.name:
        parser.error("--name must be a plain file name")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"Missing binary: {binary}")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    destination = output / args.name
    shutil.copy2(binary, destination)
    destination.chmod(0o755)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    destination.with_name(destination.name + ".sha256").write_text(
        f"{digest}  {destination.name}\n", encoding="utf-8"
    )
    print(f"{destination.name}: {destination.stat().st_size} bytes, sha256={digest[:16]}…")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
