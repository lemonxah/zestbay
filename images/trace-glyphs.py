#!/usr/bin/env python3
"""Trace the raster glyph masters in images/glyphs/ into filled icon paths.

Needs Pillow and potrace. Run from anywhere after changing a master:

    python3 images/trace-glyphs.py

The masters are black-on-white icons generated with Nano Banana 2
(gemini-3.1-flash-image, 1:1, 1K). Each is thresholded, cropped to its ink,
centred in a 16-unit square and traced, so it draws like the hand-written
stroke glyphs but as a filled outline (`fill: true`, see ZbIcon).

Outputs:
  images/glyphs/<name>.svg   the traced glyph (reference / preview)
  qml/Glyphs.qml             the TRACED block (`Glyphs.traced`)

To add one: generate a bold black line icon on white (prompts below), save
it as images/glyphs/<name>.png, add it to GLYPHS and run this script.

Prompts used (each followed by COMMON):
  grab    A single minimalist UI toolbar icon: an open hand — the 'pan /
          grab the canvas' tool cursor. Palm facing the viewer, four fingers
          upright and side by side with rounded tips (middle finger
          tallest), thumb angled out to the lower left, short wrist at the
          bottom. Drawn as pure black line art with ONE uniform, very bold,
          rounded stroke (stroke width about one eighth of the icon's
          width), rounded caps and joins, no fill, in the style of Lucide /
          Feather / Tabler interface icons.
  select  A single minimalist UI toolbar icon for a 'box select / marquee
          selection' tool: a square selection marquee drawn with a very bold
          dashed outline made of only a few thick pieces — four rounded
          L-shaped corner pieces, plus one dash in the middle of the top
          side and one dash in the middle of the left side — and a large
          solid black arrow mouse pointer (the classic cursor arrow with a
          short tail) covering the bottom-right quarter of the square, its
          tip pointing to the upper left at the square's centre. The
          marquee's bottom and right sides stop before they reach the
          pointer, leaving a clear white gap around it. Pure black on white,
          every marquee piece has the same very bold stroke (stroke width
          about one eighth of the icon's width) with rounded caps, in the
          style of Lucide / Feather / Tabler interface icons.
  COMMON  Pure white background. Extremely simple and geometric so it stays
          legible at 16x16 pixels. Perfectly centred, the icon fills about
          80% of the square canvas with an even margin. Only black and
          white: no grey, no shading, no gradient, no shadow, no texture, no
          text, no frame, no border, no extra elements, exactly one icon.
"""

import re
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "images/glyphs"
VB = 16          # viewBox of every traced glyph
PAD = 1.0        # empty margin inside the viewBox, in viewBox units
TRACE_PX = 320   # the ink's longer side when traced: smooth, yet short paths
SMOOTH = 1.2     # blur (traced pixels) before thresholding

# name: (master file, thicken). `thicken` grows the ink by that many master
# pixels on every side, for masters whose strokes are too thin at 16 px.
GLYPHS = {
    "grab": ("grab.png", 10),
    "select": ("select.png", 0),
}

TOKEN = re.compile(r"[MmLlCcZz]|-?\d*\.?\d+(?:e-?\d+)?")
ARITY = {"M": 2, "L": 2, "C": 6}


def ink_mask(path, thicken=0):
    """The master's ink as a white-on-black mask, cropped to the ink."""
    grey = Image.open(path).convert("L")
    ink = grey.point(lambda v: 255 if v < 128 else 0)
    if thicken > 0:
        # A blurred mask cut low grows the shape evenly with round corners.
        ink = ink.filter(ImageFilter.GaussianBlur(thicken * 1.2)).point(lambda v: 255 if v >= 40 else 0)
    box = ink.getbbox()
    if box is None:
        raise SystemExit(f"{path}: no ink found")
    return ink.crop(box)


def trace(mask, tmp):
    """potrace a mask scaled to TRACE_PX; returns potrace's path data, the
    traced bitmap's height, its border and the scale used."""
    w, h = mask.size
    scale = TRACE_PX / max(w, h)
    small = mask.resize((round(w * scale), round(h * scale)), Image.LANCZOS)
    # A border keeps the blur from clipping ink that touches the crop edge.
    b = 6
    framed = Image.new("L", (small.width + 2 * b, small.height + 2 * b), 0)
    framed.paste(small, (b, b))
    framed = framed.filter(ImageFilter.GaussianBlur(SMOOTH))
    bw = framed.point(lambda v: 0 if v >= 128 else 255).convert("1")  # potrace traces black
    pbm = Path(tmp) / "glyph.pbm"
    svg = Path(tmp) / "glyph.svg"
    bw.save(pbm)
    subprocess.run(["potrace", "-s", "--flat", "-a", "1.2", "-O", "1.0", "-t", "8",
                    "-o", str(svg), str(pbm)], check=True)
    d = " ".join(re.findall(r'd="([^"]+)"', svg.read_text(), re.S))
    return d, bw.height, b, scale


def to_viewbox(d, bitmap_h, border, size):
    """Rewrite potrace path data (relative, y up, 1/10 px) as absolute
    commands in the VB square (y down), the ink centred in it. `size` is the
    traced ink's (w, h) in traced pixels."""
    w, h = size
    k = (VB - 2 * PAD) / max(w, h)
    ox = (VB - w * k) / 2
    oy = (VB - h * k) / 2

    def pt(x, y):
        px = x * 0.1 - border
        py = bitmap_h - y * 0.1 - border
        return px * k + ox, py * k + oy

    toks = TOKEN.findall(d)
    out = []
    cx = cy = sx = sy = 0.0  # current point / subpath start, potrace units
    i = 0
    cmd = None
    while i < len(toks):
        if toks[i] in "MmLlCcZz":
            cmd = toks[i]
            i += 1
            if cmd in "Zz":
                out.append("Z")
                cx, cy = sx, sy
                continue
        n = ARITY[cmd.upper()]
        vals = [float(v) for v in toks[i:i + n]]
        i += n
        rel = cmd.islower()
        if cmd.upper() == "M":
            x, y = (cx + vals[0], cy + vals[1]) if rel else vals
            cx, cy, sx, sy = x, y, x, y
            out.append("M%.2f %.2f" % pt(x, y))
            cmd = "l" if rel else "L"  # implicit lineto after moveto
        elif cmd.upper() == "L":
            x, y = (cx + vals[0], cy + vals[1]) if rel else vals
            cx, cy = x, y
            out.append("L%.2f %.2f" % pt(x, y))
        else:  # C
            pts = [(cx + vals[j], cy + vals[j + 1]) if rel else (vals[j], vals[j + 1])
                   for j in (0, 2, 4)]
            cx, cy = pts[2]
            out.append("C" + " ".join("%.2f %.2f" % pt(*p) for p in pts))
    return "".join(out)


def trace_glyph(path, thicken=0):
    """Filled path data for one master, in a VB x VB viewBox."""
    mask = ink_mask(path, thicken)
    with tempfile.TemporaryDirectory() as tmp:
        raw, bitmap_h, border, scale = trace(mask, tmp)
    return to_viewbox(raw, bitmap_h, border, (mask.width * scale, mask.height * scale))


def svg(d, colour="#000"):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {VB} {VB}" '
            f'width="{VB}" height="{VB}">\n'
            f'  <path fill="{colour}" fill-rule="evenodd" d="{d}"/>\n</svg>\n')


def main():
    paths = {}
    for name, (file, thicken) in GLYPHS.items():
        paths[name] = trace_glyph(SOURCE / file, thicken)
        (SOURCE / f"{name}.svg").write_text(svg(paths[name]))

    # Path data for ZbIcon, between the markers in qml/Glyphs.qml.
    glyphs = ROOT / "qml/Glyphs.qml"
    text = glyphs.read_text()
    rows = ",\n".join(f'        "{name}": {{ vb: {VB}, fill: true, d: "{d}" }}'
                      for name, d in paths.items())
    block = (
        "    // BEGIN TRACED (generated by images/trace-glyphs.py; do not edit)\n"
        "    readonly property var traced: ({\n"
        f"{rows}\n"
        "    })\n"
        "    // END TRACED\n"
    )
    new, n = re.subn(r"    // BEGIN TRACED.*?    // END TRACED\n", lambda _: block, text, flags=re.S)
    if n != 1:
        raise SystemExit("qml/Glyphs.qml: TRACED markers not found")
    glyphs.write_text(new)
    print("path chars", {k: len(v) for k, v in paths.items()})


if __name__ == "__main__":
    main()
