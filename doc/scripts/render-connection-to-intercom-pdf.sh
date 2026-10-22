#!/usr/bin/env bash
# Renders doc/connection-to-intercom.md (GitHub style) to project-doc/technical-docu-output/connection-to-intercom.pdf.
set -euo pipefail
exec "$(dirname "${BASH_SOURCE[0]}")/helper/render-md-pdf.sh" connection-to-intercom "Doorbell Connection to the House Intercom"
