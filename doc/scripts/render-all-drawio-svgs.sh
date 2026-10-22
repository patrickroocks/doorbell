#!/usr/bin/env bash
# Renders all doc/*.drawio.svg to PNGs in project-doc/technical-docu-output/:
# <name>.png (original size) and <name>@2x.png (high-res, twice the size).
# Requires Chrome/Chromium.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOC_DIR="$(dirname "$SCRIPT_DIR")"
ROOT_DIR="$(dirname "$DOC_DIR")"
OUT_DIR="$ROOT_DIR/project-doc/technical-docu-output"

CHROME=""
for c in google-chrome chromium chromium-browser; do
    if command -v "$c" >/dev/null; then CHROME="$c"; break; fi
done
[ -n "$CHROME" ] || { echo "Chrome/Chromium not found" >&2; exit 1; }

mkdir -p "$OUT_DIR"
for SVG in "$DOC_DIR"/*.drawio.svg; do
    NAME="$(basename "$SVG" .drawio.svg)"
    # Size from the root element, e.g. width="700px" height="460px" (rounded up).
    ROOT="$(grep -o '<svg [^>]*>' "$SVG" | head -n 1)"
    W="$(sed -nE 's/.* width="([0-9.]+)(px)?".*/\1/p' <<<"$ROOT" | awk '{ print int($1 + 0.999) }')"
    H="$(sed -nE 's/.* height="([0-9.]+)(px)?".*/\1/p' <<<"$ROOT" | awk '{ print int($1 + 0.999) }')"
    [ -n "$W" ] && [ -n "$H" ] || { echo "No width/height in $SVG" >&2; exit 1; }

    for SCALE in 1 2; do
        SUFFIX=""
        [ "$SCALE" -eq 1 ] || SUFFIX="@${SCALE}x"
        OUT_PNG="$OUT_DIR/$NAME$SUFFIX.png"
        "$CHROME" --headless=new --disable-gpu --hide-scrollbars \
            --window-size="$W,$H" --force-device-scale-factor="$SCALE" \
            --screenshot="$OUT_PNG" "file://$SVG" 2>/dev/null
        echo "Written: $OUT_PNG ($((W * SCALE)) x $((H * SCALE)))"
    done
done
