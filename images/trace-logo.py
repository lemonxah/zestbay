#!/usr/bin/env python3

import colorsys
import re
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "images/logo/zestbay-master.png"
LIME = "#96dc2e"
ORANGE = "#f48d11"
SPLIT_Y = 482
SCALE = 3
SMOOTH = 1.6
MARGIN = 8


def coverage(im):
    w, h = im.size
    px = im.load()
    out = {k: Image.new("L", (w, h), 0) for k in ("lime", "orange")}
    lp = {k: v.load() for k, v in out.items()}
    for y in range(h):
        for x in range(w):
            r, g, b = px[x, y]
            hue, sat, val = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
            if sat < 0.3:
                continue
            hue *= 360
            key = "orange" if 12 <= hue <= 48 else "lime" if 58 <= hue <= 125 else None
            if key:
                cov = min(1.0, max(0.0, (val - 0.2) / (0.8 - 0.2)))
                lp[key][x, y] = int(cov * 255)
    return out


def split(mask, top):
    w, h = mask.size
    keep = Image.new("L", (w, h), 0)
    keep.paste(255, (0, 0, w, SPLIT_Y) if top else (0, SPLIT_Y, w, h))
    return Image.composite(mask, Image.new("L", (w, h), 0), keep)


def trace(mask, tmp):
    w, h = mask.size
    big = mask.resize((w * SCALE, h * SCALE), Image.LANCZOS).filter(ImageFilter.GaussianBlur(SMOOTH))
    bw = big.point(lambda v: 0 if v >= 128 else 255).convert("1")
    pbm = Path(tmp) / "layer.pbm"
    svg = Path(tmp) / "layer.svg"
    bw.save(pbm)
    subprocess.run(["potrace", "-s", "--flat", "-a", "1.15", "-O", "0.8", "-t", "24",
                    "-o", str(svg), str(pbm)], check=True)
    text = svg.read_text()
    return " ".join(re.findall(r'd="([^"]+)"', text, re.S)), h * SCALE


TOKEN = re.compile(r"[MmLlCcZz]|-?\d*\.?\d+(?:e-?\d+)?")
ARITY = {"M": 2, "L": 2, "C": 6, "Z": 0}


def to_pixels(d, bitmap_h):
    k = 0.1 / SCALE

    def pt(x, y):
        return x * k, (bitmap_h - y * 0.1) / SCALE

    toks = TOKEN.findall(d)
    out = []
    cx = cy = sx = sy = 0.0
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
            cmd = "l" if rel else "L"
        elif cmd.upper() == "L":
            x, y = (cx + vals[0], cy + vals[1]) if rel else vals
            cx, cy = x, y
            out.append("L%.2f %.2f" % pt(x, y))
        else:
            pts = [(cx + vals[j], cy + vals[j + 1]) if rel else (vals[j], vals[j + 1])
                   for j in (0, 2, 4)]
            cx, cy = pts[2]
            out.append("C" + " ".join("%.2f %.2f" % pt(*p) for p in pts))
    return "".join(out)


def bbox(d):
    nums = [float(v) for v in re.findall(r"-?\d+\.?\d*", d)]
    xs, ys = nums[0::2], nums[1::2]
    return min(xs), min(ys), max(xs), max(ys)


def union(*boxes):
    return (min(b[0] for b in boxes), min(b[1] for b in boxes),
            max(b[2] for b in boxes), max(b[3] for b in boxes))


def padded_box(b, margin):
    x0, y0, x1, y1 = b
    return (round(x0 - margin, 2), round(y0 - margin, 2),
            round(x1 - x0 + 2 * margin, 2), round(y1 - y0 + 2 * margin, 2))


def svg(box, parts):
    vb = " ".join("%g" % v for v in box)
    body = "\n".join(f'  <path fill="{fill}" d="{d}"/>' for fill, d in parts)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" '
            f'width="{box[2]:g}" height="{box[3]:g}">\n{body}\n</svg>\n')


def main():
    im = Image.open(SOURCE).convert("RGB")
    cov = coverage(im)
    paths = {}
    with tempfile.TemporaryDirectory() as tmp:
        for name, mask in (("lime", split(cov["lime"], True)),
                           ("orange", split(cov["orange"], True)),
                           ("wordLime", split(cov["lime"], False)),
                           ("wordOrange", split(cov["orange"], False))):
            raw, bh = trace(mask, tmp)
            paths[name] = to_pixels(raw, bh)

    mark_box = padded_box(union(bbox(paths["lime"]), bbox(paths["orange"])), MARGIN)
    full_box = padded_box(union(*(bbox(p) for p in paths.values())), MARGIN)
    mark_parts = [(LIME, paths["lime"]), (ORANGE, paths["orange"])]
    word_parts = [(LIME, paths["wordLime"]), (ORANGE, paths["wordOrange"])]

    logo = ROOT / "images/logo"
    (logo / "zestbay-mark.svg").write_text(svg(mark_box, mark_parts))
    (logo / "zestbay-logo.svg").write_text(svg(full_box, mark_parts + word_parts))

    glyphs = ROOT / "qml/Glyphs.qml"
    text = glyphs.read_text()
    block = (
        f"    readonly property var logoMarkBox: [{', '.join('%g' % v for v in mark_box)}]\n"
        f"    readonly property var logoFullBox: [{', '.join('%g' % v for v in full_box)}]\n"
        f'    readonly property string logoLimePath: "{paths["lime"]}"\n'
        f'    readonly property string logoOrangePath: "{paths["orange"]}"\n'
        f'    readonly property string logoWordLimePath: "{paths["wordLime"]}"\n'
        f'    readonly property string logoWordOrangePath: "{paths["wordOrange"]}"\n'
    )
    new, n = re.subn(
        r"    readonly property var logoMarkBox:.*?    readonly property string logoWordOrangePath: [^\n]*\n",
        lambda _: block, text, flags=re.S)
    if n != 1:
        raise SystemExit("qml/Glyphs.qml: logo properties not found")
    glyphs.write_text(new)
    sizes = {k: len(v) for k, v in paths.items()}
    print("mark box", mark_box, "full box", full_box, "path chars", sizes)


if __name__ == "__main__":
    main()
