#!/usr/bin/env python3
"""Generate every icon asset from the ZestBay mark (logo "B · Patch").

The mark is defined once below on a 64x64 grid (design/DESIGN-BIBLE.md, Logo).
Needs `rsvg-convert` (librsvg) and Pillow.  Run from anywhere:

    python3 images/make-icons.py

Outputs:
  images/logo/zestbay-mark.svg        mark alone, accent lime (tray, dark UI)
  images/logo/zestbay-mark-light.svg  mark alone, text.primary (light surfaces)
  images/logo/zestbay-launcher.svg    mark on a bg.panel rounded square (app icon)
  images/zesticon.png                 launcher, 256 px (native packages, README)
  images/zesttray.png                 tray mark, 256 px (native packages)
  images/zesttray-{22,32,64}.argb     tray mark as raw ARGB32 for src/tray.rs
  pkg/flatpak/icons/NxN/...           Flatpak app + tray icons, named by app id
"""

import subprocess
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
APP_ID = "io.github.lemonxah.ZestBay"

ACCENT_DARK = "#c9e265"     # accent (dark theme)
TEXT_PRIMARY_LIGHT = "#1a1d23"  # text.primary (light theme)
PANEL_DARK = "#1a1d23"      # bg.panel (dark theme)

# Logo B · Patch: two pins joined by the canvas S-curve (64x64 grid).
MARK = """\
  <path d="M19 19C47 19 17 45 45 45" fill="none" stroke="{c}" stroke-width="5.5" stroke-linecap="round"/>
  <circle cx="19" cy="19" r="6.5" fill="{c}"/>
  <circle cx="45" cy="45" r="6.5" fill="{c}"/>"""


def mark_svg(colour: str) -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">\n'
        f"{MARK.format(c=colour)}\n</svg>\n"
    )


def launcher_svg() -> str:
    # Rounded square, radius 22 % of the size; the mark keeps its own margin.
    r = 64 * 0.22
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">\n'
        f'  <rect width="64" height="64" rx="{r:.2f}" fill="{PANEL_DARK}"/>\n'
        f"{MARK.format(c=ACCENT_DARK)}\n</svg>\n"
    )


def render(svg: Path, size: int, out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), "-o", str(out), str(svg)],
        check=True,
    )


def write_argb(png: Path, out: Path) -> None:
    rgba = Image.open(png).convert("RGBA").tobytes()
    data = bytearray(len(rgba))
    data[0::4] = rgba[3::4]  # A
    data[1::4] = rgba[0::4]  # R
    data[2::4] = rgba[1::4]  # G
    data[3::4] = rgba[2::4]  # B
    out.write_bytes(bytes(data))


def main() -> None:
    logo = ROOT / "images/logo"
    logo.mkdir(parents=True, exist_ok=True)
    mark = logo / "zestbay-mark.svg"
    launcher = logo / "zestbay-launcher.svg"
    mark.write_text(mark_svg(ACCENT_DARK))
    (logo / "zestbay-mark-light.svg").write_text(mark_svg(TEXT_PRIMARY_LIGHT))
    launcher.write_text(launcher_svg())

    render(launcher, 256, ROOT / "images/zesticon.png")
    render(mark, 256, ROOT / "images/zesttray.png")
    with tempfile.TemporaryDirectory() as tmp:
        for size in (22, 32, 64):
            png = Path(tmp) / f"tray{size}.png"
            render(mark, size, png)
            write_argb(png, ROOT / f"images/zesttray-{size}.argb")

    icons = ROOT / "pkg/flatpak/icons"
    for size in (16, 22, 32, 48, 128, 256, 512):
        render(launcher, size, icons / f"{size}x{size}/{APP_ID}.png")
    for size in (16, 22, 32, 48, 256):
        render(mark, size, icons / f"{size}x{size}/{APP_ID}-tray.png")


if __name__ == "__main__":
    main()
