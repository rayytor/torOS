#!/usr/bin/env python3
"""Draw the picker's icons: the emoji groups and the emoji / clipboard switch.

  picker/icons.py    writes picker/data/icons/NAME-symbolic.svg

Each icon is drawn below on a 16x16 grid as lines 1.5 wide with round ends
(and a few filled shapes). GTK colours a symbolic icon by filling its shapes,
which would ruin lines, so every line is turned into the outline of itself
here. Run by hand on the PC after changing a drawing (needs the Python package
picosvg); the SVG files are part of the source tree, so the image build does
not run this.
"""
import os
from picosvg.svg import SVG

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data/icons")
LINE = 'fill="none" stroke="#222" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"'
FILL = 'fill="#222"'

FACE = f'<circle cx="8" cy="8" r="6.25" {LINE}/><circle cx="5.8" cy="6.6" r="1" {FILL}/><circle cx="10.2" cy="6.6" r="1" {FILL}/>'

ICONS = {
    # the switch
    "emoji": FACE + f'<path d="M4.9 9.3H11.1A3.1 3.1 0 0 1 4.9 9.3Z" {FILL}/>',
    "clipboard": f'<path d="M5.5 3.25H4.75A1.5 1.5 0 0 0 3.25 4.75V12.75A1.5 1.5 0 0 0 4.75 14.25H11.25A1.5 1.5 0 0 0 12.75 12.75V4.75A1.5 1.5 0 0 0 11.25 3.25H10.5" {LINE}/>'
                 f'<rect x="5.75" y="1.75" width="4.5" height="2.75" rx="1" {FILL}/>'
                 f'<path d="M5.75 8H10.25M5.75 11H8.75" {LINE}/>',
    # the groups
    "recent": f'<circle cx="8" cy="8" r="6.25" {LINE}/><path d="M8 4.5V8L10.4 9.6" {LINE}/>',
    "smileys": FACE + f'<path d="M5.3 9.7C6.6 11.6 9.4 11.6 10.7 9.7" {LINE}/>',
    "people": f'<circle cx="8" cy="4.75" r="2.75" {LINE}/><path d="M2.75 14C2.75 11.4 5 9.9 8 9.9S13.25 11.4 13.25 14" {LINE}/>',
    "animals": f'<ellipse cx="3.6" cy="6.9" rx="1.35" ry="1.7" {FILL}/><ellipse cx="6.3" cy="3.9" rx="1.35" ry="1.7" {FILL}/>'
               f'<ellipse cx="9.7" cy="3.9" rx="1.35" ry="1.7" {FILL}/><ellipse cx="12.4" cy="6.9" rx="1.35" ry="1.7" {FILL}/>'
               f'<path d="M8 7.7C5.9 7.7 4.2 10 4.2 11.8C4.2 13.2 5.3 13.9 6.4 13.6C7.1 13.4 7.4 13.2 8 13.2S8.9 13.4 9.6 13.6C10.7 13.9 11.8 13.2 11.8 11.8C11.8 10 10.1 7.7 8 7.7Z" {FILL}/>',
    "food": f'<path d="M2.75 7.25C2.75 4.6 5.1 2.75 8 2.75S13.25 4.6 13.25 7.25Z" {LINE}/><path d="M2.25 9.75H13.75" {LINE}/>'
            f'<path d="M2.75 12.25H13.25V12.5A1.25 1.25 0 0 1 12 13.75H4A1.25 1.25 0 0 1 2.75 12.5Z" {LINE}/>',
    "travel": f'<path d="M3.1 7.75L4.3 4.3A1.5 1.5 0 0 1 5.7 3.25H10.3A1.5 1.5 0 0 1 11.7 4.3L12.9 7.75" {LINE}/>'
              f'<rect x="1.75" y="7.75" width="12.5" height="4" rx="1.25" {LINE}/>'
              f'<path d="M3.75 11.75V13.25M12.25 11.75V13.25" {LINE}/>'
              f'<circle cx="4.6" cy="9.75" r="0.9" {FILL}/><circle cx="11.4" cy="9.75" r="0.9" {FILL}/>',
    "activities": f'<circle cx="8" cy="8" r="6.25" {LINE}/><path d="M8 1.75V14.25M3.6 3.6C5.5 5.8 5.5 10.2 3.6 12.4M12.4 3.6C10.5 5.8 10.5 10.2 12.4 12.4" {LINE}/>',
    "objects": f'<path d="M6 11.25V10.4C4.6 9.6 3.75 8.2 3.75 6.5A4.25 4.25 0 0 1 12.25 6.5C12.25 8.2 11.4 9.6 10 10.4V11.25Z" {LINE}/><path d="M6.5 13.75H9.5" {LINE}/>',
    "symbols": f'<path d="M6.4 2.75L5.1 13.25M10.9 2.75L9.6 13.25M2.9 5.75H13.4M2.6 10.25H13.1" {LINE}/>',
    "flags": f'<path d="M3.5 2.25V14" {LINE}/><path d="M3.5 3H12.75L10.6 6L12.75 9H3.5" {LINE}/>',
}


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, shapes in ICONS.items():
        drawing = f'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">{shapes}</svg>'
        outlines = SVG.fromstring(drawing).topicosvg(ndigits=2)
        paths = "".join(f'<path d="{s.as_path().d}"/>' for s in outlines.shapes())
        with open(os.path.join(OUT, f"{name}-symbolic.svg"), "w") as f:
            f.write('<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"'
                    f' fill="#222">{paths}</svg>\n')
    print(f"{OUT}: {len(ICONS)} icons")


if __name__ == "__main__":
    main()
