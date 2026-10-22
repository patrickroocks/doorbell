#!/usr/bin/env bash
# Renders doc/system-sketch.md (GitHub style) to project-doc/technical-docu-output/system-sketch.pdf.
set -euo pipefail
exec "$(dirname "${BASH_SOURCE[0]}")/helper/render-md-pdf.sh" system-sketch "Doorbell System Sketch"
