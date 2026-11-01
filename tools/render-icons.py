#!/usr/bin/env python3
"""Rebuild the bundled app icons. Requires librsvg (rsvg-convert) and ImageMagick 7."""
import pathlib
import shutil
import subprocess

ASSETS = pathlib.Path(__file__).resolve().parent.parent / "src/resources/assets"


def render(source, size, destination):
    # Supersample the vector, then downsample once for smooth small-size edges.
    png = subprocess.run(
        ["rsvg-convert", "--width", str(size * 4), "--height", str(size * 4),
         str(ASSETS / source)],
        check=True, capture_output=True,
    ).stdout
    subprocess.run(
        ["magick", "png:-", "-filter", "Lanczos", "-resize", f"{size}x{size}",
         "-strip", "-define", "png:compression-level=9", str(destination)],
        input=png, check=True,
    )


def rgba(png, destination):
    pixels = subprocess.run(
        ["magick", str(png), "-depth", "8", "rgba:-"],
        check=True, capture_output=True,
    ).stdout
    if len(pixels) != 64 * 64 * 4:
        raise ValueError(f"Expected a 64×64 RGBA tray icon: {png}")
    destination.write_bytes(pixels)


def main():
    for executable in ("rsvg-convert", "magick"):
        if shutil.which(executable) is None:
            raise SystemExit(f"Missing {executable}; install librsvg and ImageMagick 7.")
    for size, filename in ((256, "app-icon.png"), (64, "app-icon-64.png")):
        render("app-icon.svg", size, ASSETS / filename)
    rgba(ASSETS / "app-icon-64.png", ASSETS / "app-icon-64.rgba")
    render("app-icon-template.svg", 64, ASSETS / "app-icon-template-64.png")
    rgba(ASSETS / "app-icon-template-64.png", ASSETS / "app-icon-template-64.rgba")


if __name__ == "__main__":
    main()
