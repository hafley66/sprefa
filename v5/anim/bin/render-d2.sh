#!/usr/bin/env bash
# Render every graphs/*.d2 to public/<name>.svg.
# Multi-board d2 files can instead be animated:  d2 --animate-interval=800 in.d2 out.svg
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p public
for f in graphs/*.d2; do
  name="$(basename "$f" .d2)"
  d2 "$f" "public/$name.svg"
  echo "rendered public/$name.svg"
done
