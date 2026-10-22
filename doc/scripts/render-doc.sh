#!/usr/bin/env bash
# Renders all documents in doc/ to PDFs and all diagrams (*.drawio.svg) to PNGs in
# project-doc/technical-docu-output/.
set -euo pipefail
SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
"$SCRIPT_DIR/render-bill-of-materials-pdf.sh"
"$SCRIPT_DIR/render-system-sketch-pdf.sh"
"$SCRIPT_DIR/render-connection-to-intercom-pdf.sh"
"$SCRIPT_DIR/render-all-drawio-svgs.sh"
