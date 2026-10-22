#!/usr/bin/env bash
# Renders doc/bill-of-materials.md (GitHub style) to project-doc/technical-docu-output/bill-of-materials.pdf.
set -euo pipefail
exec "$(dirname "${BASH_SOURCE[0]}")/helper/render-md-pdf.sh" bill-of-materials "Doorbell Bill of Materials"
