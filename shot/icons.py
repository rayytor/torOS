#!/usr/bin/env python3
"""Draw the icons of toros-shot: the bar on the frozen screen, the recording
strip and the editor's tools.

  shot/icons.py    writes shot/data/icons/NAME-symbolic.svg

Drawn the way the picker's are (see picker/icons.py): on a 16x16 grid as lines
1.5 wide with round ends, each line then turned into its own outline, because
GTK colours a symbolic icon by filling its shapes. Run by hand on the PC after
changing a drawing (needs the Python package picosvg); the SVG files are part
of the source tree, so the image build does not run this.
"""
import os
from picosvg.svg import SVG

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data/icons")
LINE = 'fill="none" stroke="#222" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"'
FILL = 'fill="#222"'

ICONS = {
    # the bar: screenshot / recording, the whole screen, close
    "photo": f'<path d="M2.75 5.75A1.5 1.5 0 0 1 4.25 4.25H5.4L6.3 2.9H9.7L10.6 4.25H11.75A1.5 1.5 0 0 1 13.25 5.75V11.25A1.5 1.5 0 0 1 11.75 12.75H4.25A1.5 1.5 0 0 1 2.75 11.25Z" {LINE}/>'
             f'<circle cx="8" cy="8.4" r="2.1" {LINE}/>',
    "video": f'<rect x="1.75" y="4.25" width="8.5" height="7.5" rx="1.5" {LINE}/><path d="M10.25 7.2L14 5.2V10.8L10.25 8.8" {LINE}/>',
    "screen": f'<rect x="1.75" y="2.75" width="12.5" height="8.5" rx="1.5" {LINE}/><path d="M5.75 13.75H10.25M8 11.25V13.75" {LINE}/>',
    "close": f'<path d="M4 4L12 12M12 4L4 12" {LINE}/>',
    # the shape of a screenshot: a rectangle, or any shape drawn by hand
    "rect": f'<path d="M2.75 5.5V4.25A1.5 1.5 0 0 1 4.25 2.75H5.5M10.5 2.75H11.75A1.5 1.5 0 0 1 13.25 4.25V5.5M13.25 10.5V11.75A1.5 1.5 0 0 1 11.75 13.25H10.5M5.5 13.25H4.25A1.5 1.5 0 0 1 2.75 11.75V10.5" {LINE}/>'
            f'<path d="M7.4 2.75H8.6M7.4 13.25H8.6M2.75 7.4V8.6M13.25 7.4V8.6" {LINE}/>',
    "free": f'<path d="M8.2 2.75C11.6 2.75 13.6 4.6 13.25 7.2C13 9.3 11.1 9.1 10.3 10.7C9.5 12.4 8.3 13.4 6.2 13.2C3.9 13 2.6 10.9 2.8 8.2C3 5.2 5.2 2.75 8.2 2.75Z" {LINE}/>',
    # the recording strip
    "stop": f'<rect x="3.5" y="3.5" width="9" height="9" rx="2" {FILL}/>',
    "trash": f'<path d="M3.25 4.75H12.75M6.25 4.75V3.5A0.75 0.75 0 0 1 7 2.75H9A0.75 0.75 0 0 1 9.75 3.5V4.75" {LINE}/>'
             f'<path d="M4.5 4.75L5.1 12.35A1 1 0 0 0 6.1 13.25H9.9A1 1 0 0 0 10.9 12.35L11.5 4.75" {LINE}/><path d="M6.9 7.25V10.75M9.1 7.25V10.75" {LINE}/>',
    "sound": f'<path d="M2.75 6.5H4.75L7.75 3.75V12.25L4.75 9.5H2.75Z" {LINE}/><path d="M10.25 6.2C11.1 7.2 11.1 8.8 10.25 9.8M12.1 4.6C13.9 6.6 13.9 9.4 12.1 11.4" {LINE}/>',
    "muted": f'<path d="M2.75 6.5H4.75L7.75 3.75V12.25L4.75 9.5H2.75Z" {LINE}/><path d="M10.5 6.25L13.75 9.75M13.75 6.25L10.5 9.75" {LINE}/>',
    # the editor's tools
    "marker": f'<path d="M10.5 1.75L14.25 5.5L7.75 12L4 8.25Z" {LINE}/><path d="M4 8.25L2.75 11.25L4.75 13.25L7.75 12" {LINE}/>'
              f'<path d="M9.5 13.75H13.75" {LINE}/>',
    "pen": f'<path d="M2.75 13.25L3.5 10.25L10.75 3A1.6 1.6 0 0 1 13 5.25L5.75 12.5Z" {LINE}/><path d="M9.5 4.25L11.75 6.5" {LINE}/>',
    "eraser": f'<path d="M8.9 2.9L13.1 7.1A1.1 1.1 0 0 1 13.1 8.65L8.5 13.25H5.4L2.9 10.75A1.1 1.1 0 0 1 2.9 9.2L7.35 2.9A1.1 1.1 0 0 1 8.9 2.9Z" {LINE}/>'
              f'<path d="M5.3 6.6L10.2 11.5M8.5 13.25H13.25" {LINE}/>',
    "undo": f'<path d="M5.5 3.25L2.75 6L5.5 8.75" {LINE}/><path d="M2.75 6H9.5A3.75 3.75 0 0 1 9.5 13.5H6" {LINE}/>',
    "redo": f'<path d="M10.5 3.25L13.25 6L10.5 8.75" {LINE}/><path d="M13.25 6H6.5A3.75 3.75 0 0 0 6.5 13.5H10" {LINE}/>',
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
