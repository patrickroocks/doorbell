#!/usr/bin/env bash
# Renders doc/<name>.md (GitHub style) to project-doc/technical-docu-output/<name>.pdf.
# Usage: helper/render-md-pdf.sh <name> <page title>
# Requires pandoc and Chrome/Chromium.
set -euo pipefail

[ $# -eq 2 ] || { echo "Usage: $0 <name> <page title>" >&2; exit 1; }
NAME="$1"
TITLE="$2"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOC_DIR="$(dirname "$(dirname "$SCRIPT_DIR")")"
ROOT_DIR="$(dirname "$DOC_DIR")"
OUT_DIR="$ROOT_DIR/project-doc/technical-docu-output"
IN_MD="$DOC_DIR/$NAME.md"
OUT_PDF="$OUT_DIR/$NAME.pdf"

[ -f "$IN_MD" ] || { echo "Not found: $IN_MD" >&2; exit 1; }

CHROME=""
for c in google-chrome chromium chromium-browser; do
    if command -v "$c" >/dev/null; then CHROME="$c"; break; fi
done
[ -n "$CHROME" ] || { echo "Chrome/Chromium not found" >&2; exit 1; }
command -v pandoc >/dev/null || { echo "pandoc not found" >&2; exit 1; }

# The HTML must live in doc/ so relative image links (*.drawio.svg) resolve. The CSS goes there
# too, a snap Chromium (Ubuntu) has a private /tmp and would not find it.
TMP_HTML="$(mktemp "$DOC_DIR/.$NAME.XXXXXX.html")"
TMP_CSS="$(mktemp "$DOC_DIR/.$NAME.XXXXXX.css")"
trap 'rm -f "$TMP_HTML" "$TMP_CSS"' EXIT

cat > "$TMP_CSS" <<'EOF'
.markdown-body { font-size: 11px; }
h1, h2, h3 { break-after: avoid; }
pre, table, figure, img { break-inside: avoid; }
EOF

# A huge --columns keeps pandoc from deriving fixed column widths from the source's "| --- |" line
# (which long link URLs blow up); the browser then sizes columns from the rendered content.
pandoc "$IN_MD" -s --columns=100000 \
    --metadata pagetitle="$TITLE" \
    -c https://cdn.jsdelivr.net/npm/github-markdown-css@5.5.1/github-markdown.min.css \
    -c "file://$TMP_CSS" \
    -V include-before='<div class="markdown-body" style="max-width:980px;margin:auto;padding:24px">' \
    -V include-after='</div>' \
    -o "$TMP_HTML"

mkdir -p "$OUT_DIR"
"$CHROME" --headless=new --disable-gpu --no-pdf-header-footer \
    --print-to-pdf="$OUT_PDF" "file://$TMP_HTML" 2>/dev/null

echo "Written: $OUT_PDF"
