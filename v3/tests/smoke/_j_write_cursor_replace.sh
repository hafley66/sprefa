#!/usr/bin/env bash
# Smoke j: end-to-end fs > comment(${SCOPE?}) > render > write_cursor.
#
# Proves the marker-zone codegen loop:
#   1. fs lists the target file
#   2. comment("@begin", ${SCOPE?}, "@end") narrows byte_range to the
#      whole region AND binds the inner range (exclusive of marker
#      lines) to a capture named SCOPE
#   3. render[plain] rebases cursor.content to a templated literal
#   4. write_cursor(${SCOPE}, :replace) splices the rendered bytes
#      into the original file at SCOPE.byte_range via WriteRangeEffect
#      / WriteRangeBatcher::disk()
#
# Asserts target file contents on disk after drain — markers preserved,
# old body gone, new body in place.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"

bin="$v3_dir/target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_wc_smoke.XXXX)"
trap 'rm -rf "$work"' EXIT

target="$work/target.txt"
cat >"$target" <<'EOF'
header line
# @sprf-begin
old body line one
old body line two
# @sprf-end
footer line
EOF

sprf="$work/replace.sprf"
cat >"$sprf" <<'EOF'
fs(glob(target.txt)) > comment("@sprf-begin", ${SCOPE?}, "@sprf-end") > render[plain](REPLACED-BY-SMOKE) > write_cursor(${SCOPE}, :replace)
EOF

(cd "$work" && "$bin" "$sprf" --root .) >/dev/null

after="$(cat "$target")"

echo "----- target.txt after drain -----"
echo "$after"
echo "----------------------------------"

grep -qF 'header line'          <<<"$after" || { echo "FAIL: header line lost" >&2; exit 1; }
grep -qF '# @sprf-begin'        <<<"$after" || { echo "FAIL: open marker lost" >&2; exit 1; }
grep -qF '# @sprf-end'          <<<"$after" || { echo "FAIL: close marker lost" >&2; exit 1; }
grep -qF 'footer line'          <<<"$after" || { echo "FAIL: footer line lost" >&2; exit 1; }
grep -qF 'REPLACED-BY-SMOKE'    <<<"$after" || { echo "FAIL: replacement bytes not present" >&2; exit 1; }
if grep -qF 'old body' <<<"$after"; then
    echo "FAIL: old body still present — splice did not take" >&2
    exit 1
fi
# Marker order preserved: begin < REPLACED < end < footer.
expected_order='# @sprf-begin
REPLACED-BY-SMOKE# @sprf-end
footer line'
if ! grep -qF "$expected_order" <<<"$after"; then
    echo "FAIL: spliced order does not match expected:" >&2
    echo "$expected_order" >&2
    exit 1
fi

echo "OK write_cursor :replace round-trip"
