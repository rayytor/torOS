#!/usr/bin/env python3
"""Turn the torOS logo into the picture the kernel shows at boot.
  build/splash.py     writes kernel/logo.ppm and kernel/logo.bmp (run by hand;
                      the files are kept in the project, as the wallpapers are)

build/kernel.sh puts logo.ppm in place of the kernel's penguin. The format is
the kernel's: plain-text PPM, at most 224 colours, no transparency, so the
logo is drawn on the splash background here. logo.bmp is the same picture,
pixel for pixel, for the boot loader's stub, which shows it before the kernel
runs (build/in-container.sh); both put it in the middle of the screen, so the
kernel's copy lands exactly on the stub's. That background is black: the
screen is black before the kernel has a display and again while the desktop
takes it over, so black is the only colour that does not flash, and the
logo's white peaks disappear on a light one.
"""
import os
from PIL import Image

os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
HEIGHT = 176            # about a quarter of the laptop's 768 lines
BACKGROUND = (0, 0, 0)

logo = Image.open("torOS.png").convert("RGBA")
logo = logo.crop(logo.getbbox())        # the drawing without its empty margin
logo = logo.resize((round(logo.width * HEIGHT / logo.height), HEIGHT), Image.LANCZOS)
picture = Image.new("RGB", logo.size, BACKGROUND)
picture.paste(logo, (0, 0), logo)
picture = picture.quantize(224, method=Image.MEDIANCUT, dither=Image.NONE).convert("RGB")
assert picture.getpixel((0, 0)) == BACKGROUND, "the corner must stay the background colour"

data = picture.tobytes()
pixels = [tuple(data[i:i + 3]) for i in range(0, len(data), 3)]
with open("kernel/logo.ppm", "w") as f:
    f.write(f"P3\n# torOS logo, written by build/splash.py\n{picture.width} {picture.height}\n255\n")
    for y in range(picture.height):
        row = pixels[y * picture.width:(y + 1) * picture.width]
        f.write(" ".join(f"{r} {g} {b}" for r, g, b in row) + "\n")
picture.save("kernel/logo.bmp")
print(f"kernel/logo.ppm, kernel/logo.bmp: {picture.width}x{picture.height}, {len(set(pixels))} colours")
