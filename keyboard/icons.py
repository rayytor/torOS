#!/usr/bin/env python3
"""Draw the keyboard's icons: Shift in its three states, delete, return, and
the arrow on the tab the keyboard leaves at the edge of the screen.

  keyboard/icons.py    writes keyboard/data/icons/NAME-symbolic.svg

Drawn the way the picker's are (see picker/icons.py), but on a 28x28 grid, one
unit for each pixel of the key they stand on, as lines 1.8 wide with round
ends. Each line is then turned into its own outline, because GTK colours a
symbolic icon by filling its shapes. Run by hand on the PC after changing a
drawing (needs the Python package picosvg); the SVG files are part of the
source tree, so the image build does not run this.
"""
import os
from picosvg.svg import SVG

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data/icons")
SIZE = 28
ROUND = 'stroke-linecap="round" stroke-linejoin="round"'
LINE = f'fill="none" stroke="#222" stroke-width="1.8" {ROUND}'
# a filled shape with the same rounded outside as the line of that shape
SOLID = f'fill="#222" stroke="#222" stroke-width="1.8" {ROUND}'
BOLD = f'fill="none" stroke="#222" stroke-width="2.6" {ROUND}'

ARROW = "M14 4.5L24.5 14.75H19V22.5H9V14.75H3.5Z"

ICONS = {
    # Shift: off, on for one letter, on for all of them
    "shift": f'<path d="{ARROW}" {LINE}/>',
    "shift-on": f'<path d="{ARROW}" {SOLID}/>',
    "caps": f'<path d="M14 3L24.5 13.25H19V17.75H9V13.25H3.5Z" {SOLID}/><path d="M9 23.5H19" {SOLID}/>',
    "backspace": f'<path d="M10.6 6.5H22.25A3 3 0 0 1 25.25 9.5V18.5A3 3 0 0 1 22.25 21.5H10.6A2.6 2.6 0 0 1 8.7 20.7L2.75 14L8.7 7.3A2.6 2.6 0 0 1 10.6 6.5Z" {LINE}/>'
                 f'<path d="M13.25 10.75L19.75 17.25M19.75 10.75L13.25 17.25" {LINE}/>',
    "return": f'<path d="M16 8H21A1.75 1.75 0 0 1 22.75 9.75V14.75A1.75 1.75 0 0 1 21 16.5H5.75" {LINE}/>'
              f'<path d="M10.5 11.75L5.75 16.5L10.5 21.25" {LINE}/>',
    # the tab: which way the keyboard comes out
    "pull-left": f'<path d="M17 6.5L10 14L17 21.5" {BOLD}/>',
    "pull-right": f'<path d="M11 6.5L18 14L11 21.5" {BOLD}/>',
}


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, shapes in ICONS.items():
        drawing = f'<svg xmlns="http://www.w3.org/2000/svg" width="{SIZE}" height="{SIZE}" viewBox="0 0 {SIZE} {SIZE}">{shapes}</svg>'
        outlines = SVG.fromstring(drawing).topicosvg(ndigits=2)
        paths = "".join(f'<path d="{s.as_path().d}"/>' for s in outlines.shapes())
        with open(os.path.join(OUT, f"{name}-symbolic.svg"), "w") as f:
            f.write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{SIZE}" height="{SIZE}" viewBox="0 0 {SIZE} {SIZE}"'
                    f' fill="#222">{paths}</svg>\n')
    print(f"{OUT}: {len(ICONS)} icons")


if __name__ == "__main__":
    main()
