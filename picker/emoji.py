#!/usr/bin/env python3
"""Write the picker's emoji table from Unicode's list and CLDR's search words.

  picker/emoji.py FONT    writes picker/data/emoji.txt

FONT is the Apple Color Emoji file the image is built with (see "Apple Color
Emoji" in build/in-container.sh); an emoji the font cannot draw as one picture
is left out. Run by hand on the PC when the font or the Unicode version
changes (needs the Python package uharfbuzz and the network); the table is
part of the source tree, so the image build does not run this.

One line per emoji, fields separated by tabs:
  emoji, the same emoji with the light skin tone ("" if it has no skin
  tones), its English name, words to find it by (lower case, English and
  Turkish). A line starting with "# " begins a group.
"""
import os, re, sys, urllib.request
import xml.etree.ElementTree as ET
import uharfbuzz as hb

UNICODE = "17.0.0"
CLDR = "release-48"
SOURCES = [f"https://unicode.org/Public/{UNICODE}/emoji/emoji-test.txt"] + [
    f"https://raw.githubusercontent.com/unicode-org/cldr/{CLDR}/common/{d}/{lang}.xml"
    for lang in ("en", "tr") for d in ("annotations", "annotationsDerived")]
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data/emoji.txt")
TONES = set(range(0x1F3FB, 0x1F400))
VS16 = "️"


def fetch(url):
    with urllib.request.urlopen(url, timeout=60) as r:
        return r.read().decode("utf-8")


def bare(s):
    """An emoji without skin tones and variation selectors (CLDR's spelling)."""
    return "".join(c for c in s if ord(c) not in TONES and c != VS16)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    face = hb.Face(hb.Blob.from_file_path(sys.argv[1]))
    font = hb.Font(face)

    def drawable(s):
        buf = hb.Buffer()
        buf.add_str(s)
        buf.guess_segment_properties()
        hb.shape(font, buf)
        return len(buf.glyph_infos) == 1 and buf.glyph_infos[0].codepoint != 0

    test, *cldr = [fetch(u) for u in SOURCES]

    words = {}
    for xml in cldr:
        for a in ET.fromstring(xml).iter("annotation"):
            if not {ord(c) for c in a.get("cp")} & TONES:
                words.setdefault(bare(a.get("cp")), []).extend(w.strip().lower() for w in a.text.split("|"))

    groups, toned, group = [], {}, None
    for line in test.splitlines():
        if line.startswith("# group: "):
            group = (line[9:], [])
            groups.append(group)
            continue
        m = re.match(r"([0-9A-F ]+?) *; fully-qualified *# \S+ E[\d.]+ (.+)", line)
        if not m:
            continue
        s = "".join(chr(int(c, 16)) for c in m.group(1).split())
        tones = {ord(c) for c in s} & TONES
        if not tones:
            group[1].append((s, m.group(2)))
        elif tones == {0x1F3FB}:
            toned[bare(s)] = s

    lines, skipped, count, tones = [], [], 0, 0
    for name, emojis in groups:
        if name == "Component":
            continue
        lines.append(f"# {name}")
        for s, title in emojis:
            if not drawable(s):
                skipped.append(title)
                continue
            tone = toned.get(bare(s), "")
            if tone and not drawable(tone):
                tone = ""
            # a word that is already part of the text would be found anyway
            find = title.lower()
            for w in words.get(bare(s), []):
                if w not in find:
                    find += " " + w
            lines.append("\t".join((s, tone, title, find)))
            count, tones = count + 1, tones + bool(tone)

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{OUT}: {count} emoji, {tones} with skin tones")
    if skipped:
        print(f"not in the font ({len(skipped)}): " + ", ".join(skipped))


if __name__ == "__main__":
    main()
