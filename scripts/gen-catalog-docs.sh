#!/usr/bin/env bash
# Regenerate docs/book/src/property-catalog.md by tallying the templates
# under crates/vergil-properties/templates/. Phase 3 implementation: prints
# the category counts; the docs page itself is hand-written for now.
# Phase 4 will replace this with a full per-template auto-gen pass.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
TEMPLATES="${ROOT}/crates/vergil-properties/templates"

echo "Template count: $(find "$TEMPLATES" -name manifest.yaml | wc -l | tr -d ' ')"
echo
echo "By category:"
ls "$TEMPLATES" | sed 's/-.*//' | sort | uniq -c | sort -rn
echo
echo "Templates listed in docs/book/src/property-catalog.md should match the count above."
echo "If not, update the docs page accordingly (Phase 4 will auto-gen this)."
