#!/usr/bin/env bash
set -euo pipefail

d=$(mktemp -d)
trap 'rm -rf "$d"' EXIT
mkdir -p "$d/scripts" "$d/.dl" "$d/src"
cp scripts/filesize-rail.sh "$d/scripts/"
printf 'src/a.rs\n' >"$d/scripts/filesize-allow.txt"
printf 'big_file_ok("src/a.rs", "test")\n' >"$d/.dl/file-size.dl"
awk 'BEGIN { for (i = 1; i <= 501; i++) print "line" }' >"$d/src/a.rs"
git -C "$d" init -q
git -C "$d" add .

# Red: the allowlist has a path that the dl facts do not; the rail warns but
# continues enforcing the file-size budget.
grep -v 'big_file_ok' "$d/.dl/file-size.dl" >"$d/.dl/file-size.tmp" || true
mv -f "$d/.dl/file-size.tmp" "$d/.dl/file-size.dl"
(cd "$d" && ./scripts/filesize-rail.sh) >"$d/red.out" 2>"$d/red.err"
grep -q 'has no big_file_ok fact' "$d/red.err"

# Green: restore the matching fact; the same 501-line file is grandfathered.
printf 'big_file_ok("src/a.rs", "test")\n' >"$d/.dl/file-size.dl"
(cd "$d" && ./scripts/filesize-rail.sh) >"$d/green.out"
grep -q 'grandfathered' "$d/green.out"
