#!/usr/bin/env python3

import re
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PANEL_DARK = "#1a1d23"
MARK_SVG = ROOT / "images/logo/zestbay-mark.svg"


def square_svg(pad: float, background: str = "") -> str:
    mark = MARK_SVG.read_text()
    vb = re.search(r'viewBox="([^"]+)"', mark).group(1)
    body = mark[mark.index(">", mark.index("<svg")) + 1:mark.rindex("</svg>")]
    p = 64 * pad
    bg = f'  <rect width="64" height="64" rx="{64 * 0.22:.2f}" fill="{background}"/>\n' if background else ""
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">\n'
        f"{bg}"
        f'  <svg x="{p:.2f}" y="{p:.2f}" width="{64 - 2 * p:.2f}" height="{64 - 2 * p:.2f}" '
        f'viewBox="{vb}" preserveAspectRatio="xMidYMid meet">{body}</svg>\n'
        "</svg>\n"
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
    data[0::4] = rgba[3::4]
    data[1::4] = rgba[0::4]
    data[2::4] = rgba[1::4]
    data[3::4] = rgba[2::4]
    out.write_bytes(bytes(data))


def main() -> None:
    logo = ROOT / "images/logo"
    launcher = logo / "zestbay-launcher.svg"
    launcher.write_text(square_svg(0.08, PANEL_DARK))
    stale = logo / "zestbay-mark-light.svg"
    if stale.exists():
        stale.unlink()

    with tempfile.TemporaryDirectory() as tmp:
        tray = Path(tmp) / "tray.svg"
        tray.write_text(square_svg(0.02))
        render(launcher, 256, ROOT / "images/zesticon.png")
        render(tray, 256, ROOT / "images/zesttray.png")
        for size in (22, 32, 64):
            png = Path(tmp) / f"tray{size}.png"
            render(tray, size, png)
            write_argb(png, ROOT / f"images/zesttray-{size}.argb")


if __name__ == "__main__":
    main()
