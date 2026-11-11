"""Write each symbol in the manual's cut-down Nerd Font as an SVG path, for the screenshots.

    python symbol-paths.py FONT OUT.rs

scripts/symbols-font.sh runs it with fontTools. A picture shown on a page with <img> can't use
the page's fonts, and a font carried inside the picture can be drawn before it has loaded, so
the screenshots draw each symbol as its outline instead.
"""

import sys

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont

font_path, out_path = sys.argv[1], sys.argv[2]
font = TTFont(font_path)
glyphs = font.getGlyphSet()
cmap = font.getBestCmap()
advances = {glyphs[name].width for name in cmap.values()}
assert len(advances) == 1, "a Mono font's symbols are all as wide"

lines = [
    "//! The Nerd Font symbols the program draws, as SVG paths, for the screenshots. Made by",
    "//! `scripts/symbols-font.sh` from the manual's cut-down Symbols Nerd Font Mono: don't edit it.",
    "//! The outlines are the icon sets', under the licences in `docs/manual/src/symbols/NOTICE`.",
    "",
    "/// How wide every symbol is, in the font's units, which the paths are drawn in, with y going",
    "/// up from the baseline.",
    f"pub const ADVANCE: f32 = {advances.pop()}.0;",
    "/// The font's ascent and descent, in its units, whose middle is the symbols' middle.",
    f"pub const ASCENT: f32 = {font['hhea'].ascent}.0;",
    f"pub const DESCENT: f32 = {font['hhea'].descent}.0;",
    "",
    "/// Each symbol, by its character, with its outline.",
    "pub const PATHS: &[(char, &str)] = &[",
]
for code in sorted(cmap):
    pen = SVGPathPen(glyphs)
    glyphs[cmap[code]].draw(pen)
    lines.append(f"    ('\\u{{{code:x}}}', \"{pen.getCommands()}\"),")
lines.append("];")

with open(out_path, "w", encoding="utf-8") as out:
    out.write("\n".join(lines) + "\n")
