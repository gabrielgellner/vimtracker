#!/bin/sh
# Make the manual's Nerd Font: Symbols Nerd Font Mono from Nerd Fonts' release, cut down to the
# symbols the program draws, which are those in crates/vt-tui/src/glyphs.rs. The manual's pages
# use it, as a browser has no Nerd Font of its own, and the screenshots draw its symbols as
# paths, which scripts/symbol-paths.py writes from it.
#
#   scripts/symbols-font.sh   (or just symbols-font) after adding a symbol to glyphs.rs
#
# It writes the font, the symbols in it and Nerd Fonts' licence to docs/manual/src/symbols, and
# the paths to crates/vt-tui/src/screenshot/symbols.rs. It needs curl, unzip and uv, which runs
# fontTools without installing it.
set -eu

version=v3.5.1
root=$(cd "$(dirname "$0")/.." && pwd)
out=$root/docs/manual/src/symbols
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -sSfL -o "$work/symbols.zip" \
    "https://github.com/ryanoasis/nerd-fonts/releases/download/$version/NerdFontsSymbolsOnly.zip"
unzip -q -o "$work/symbols.zip" SymbolsNerdFontMono-Regular.ttf LICENSE -d "$work"

# Every symbol written as \u{...} in glyphs.rs that's in a private use area, where Nerd Fonts puts
# its symbols, so the no-break space and the like are left to the page's own fonts.
mkdir -p "$out"
grep -o '\\u{[0-9a-fA-F]*}' "$root/crates/vt-tui/src/glyphs.rs" |
    sed 's/\\u{\(.*\)}/\1/' |
    while read -r hex; do
        if [ "$((0x$hex))" -ge "$((0xE000))" ]; then
            printf 'U+%04X\n' "$((0x$hex))"
        fi
    done | sort -u >"$out/codepoints.txt"

uvx --quiet --from 'fonttools[woff]==4.60.1' pyftsubset "$work/SymbolsNerdFontMono-Regular.ttf" \
    --unicodes-file="$out/codepoints.txt" \
    --layout-features='' --no-hinting --desubroutinize --drop-tables+=PfEd \
    --flavor=woff2 --output-file="$out/symbols-nerd-font-mono.woff2"
cp "$work/LICENSE" "$out/LICENSE"
uv run --quiet --no-project --with 'fonttools[woff]==4.60.1' python -I \
    "$root/scripts/symbol-paths.py" "$out/symbols-nerd-font-mono.woff2" \
    "$root/crates/vt-tui/src/screenshot/symbols.rs"

echo "$out/symbols-nerd-font-mono.woff2: $(wc -l <"$out/codepoints.txt" | tr -d ' ') symbols," \
    "$(wc -c <"$out/symbols-nerd-font-mono.woff2" | tr -d ' ') bytes"
