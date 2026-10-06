#!/usr/bin/env python3
"""Draw the torOS wallpapers: layered mountain ridges in the logo's colours,
one for the light desktop and one for the dark one.

  build/wallpaper.py    writes rootfs/usr/share/toros/wallpaper-{light,dark}.png

Run by hand on the PC when the picture should change (needs Pillow); the two
PNG files are part of the source tree, so the image build does not run this.
"""
import os, random
from PIL import Image, ImageDraw

W, H, SS = 1920, 1080, 2          # drawn at twice the size, then scaled down
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../rootfs/usr/share/toros")

SCHEMES = {
    "light": {
        "sky": ("#c3d0dc", "#eef1f2"),
        # far to near: (colour, height of the ridge line, roughness)
        "ridges": [("#d3dbe3", 0.50, 0.20), ("#b3bdc8", 0.60, 0.17), ("#8b97a5", 0.70, 0.14),
                   ("#5a6a7e", 0.80, 0.11)],
        "hills": [("#21ae68", 0.90), ("#1b8553", 0.96)],
    },
    "dark": {
        "sky": ("#0f1117", "#232936"),
        "ridges": [("#2b3342", 0.50, 0.20), ("#252c3a", 0.60, 0.17), ("#1f2531", 0.70, 0.14),
                   ("#191e28", 0.80, 0.11)],
        "hills": [("#14573a", 0.90), ("#0f432d", 0.96)],
    },
}

def rgb(h):
    return tuple(int(h[i:i + 2], 16) for i in (1, 3, 5))

def ridge(rng, base, rough, steps=9):
    """A jagged skyline by midpoint displacement: list of (x, y), 0..1."""
    pts = [(0.0, base + rng.uniform(-rough, rough) * 0.3), (1.0, base + rng.uniform(-rough, rough) * 0.3)]
    spread = rough
    for _ in range(steps):
        nxt = []
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            nxt += [(x0, y0), ((x0 + x1) / 2, (y0 + y1) / 2 + rng.uniform(-spread, spread))]
        pts = nxt + [pts[-1]]
        spread *= 0.52
    return pts

def hill(rng, base):
    """A smooth rolling line."""
    import math
    a, b, p, q = rng.uniform(0.02, 0.04), rng.uniform(0.01, 0.02), rng.uniform(0, 6.3), rng.uniform(0, 6.3)
    return [(x / 200, base + a * math.sin(x / 200 * 5 + p) + b * math.sin(x / 200 * 11 + q)) for x in range(201)]

def draw(name, scheme):
    w, h = W * SS, H * SS
    top, bottom = (rgb(c) for c in scheme["sky"])
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / (h - 1)
        d.line([(0, y), (w, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
    rng = random.Random(7)          # the same skyline in both pictures
    for colour, base, rough in scheme["ridges"]:
        line = ridge(rng, base, rough)
        d.polygon([(x * w, y * h) for x, y in line] + [(w, h), (0, h)], fill=rgb(colour))
    for colour, base in scheme["hills"]:
        line = hill(rng, base)
        d.polygon([(x * w, y * h) for x, y in line] + [(w, h), (0, h)], fill=rgb(colour))
    img = img.resize((W, H), Image.LANCZOS)
    path = os.path.normpath(os.path.join(OUT, f"wallpaper-{name}.png"))
    img.save(path, optimize=True)
    print(path, os.path.getsize(path) // 1024, "KB")

for name, scheme in SCHEMES.items():
    draw(name, scheme)
