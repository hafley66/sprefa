#!/usr/bin/env bash
# Turn the harness's machine-readable matrix (perf.json) into a self-contained,
# openable charts page (perf-charts.html). No CDN, no server, no hand-editing.
#
#   1. cargo run --release --example perf_report   # measures -> perf.json (+ md)
#   2. tools/gen-perf-charts.sh                     # perf.json  -> perf-charts.html
#   3. open perf-charts.html
#
# Regenerating charts after a page tweak costs nothing (no re-measure). Args
# override paths:  gen-perf-charts.sh [perf.json] [perf-charts.html]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
store="$(cd "$here/.." && pwd)"
json="${1:-$store/perf.json}"
out="${2:-$store/perf-charts.html}"
tpl="$here/perf-charts.template.html"

[ -f "$tpl" ]  || { echo "missing template: $tpl" >&2; exit 1; }
[ -f "$json" ] || { echo "no data: $json (run: cargo run --release --example perf_report)" >&2; exit 1; }

# Inline the JSON where the single __PERF_JSON__ token sits. awk splices the file
# contents in verbatim, so no sed escaping games regardless of JSON content.
awk -v jf="$json" '
  {
    i = index($0, "__PERF_JSON__")
    if (i) {
      printf "%s", substr($0, 1, i-1)
      while ((getline line < jf) > 0) print line
      close(jf)
      print substr($0, i + length("__PERF_JSON__"))
    } else print
  }
' "$tpl" > "$out.tmp"
mv "$out.tmp" "$out"

cells="$(grep -c '"engine"' "$json" 2>/dev/null || echo '?')"
echo "wrote $out  ($cells cells from $(basename "$json"))"
