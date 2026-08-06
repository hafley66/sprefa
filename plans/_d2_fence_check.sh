#!/usr/bin/env bash
# A fence that does not compile is a broken diagram in the reader's app.
set -uo pipefail
fail=0
for md in "$@"; do
  n=0
  while IFS= read -r -d '' block; do
    n=$((n + 1))
    tmp=$(mktemp -t fenceXXXXXX).d2
    printf '%s' "$block" >"$tmp"
    if ! out=$(d2 "$tmp" "${tmp%.d2}.svg" 2>&1); then
      echo "FAIL $md fence #$n"
      echo "$out" | sed 's/^/    /'
      fail=1
    fi
    rm -f "$tmp" "${tmp%.d2}.svg"
  done < <(awk '
    /^```d2$/ { inb = 1; buf = ""; next }
    /^```$/   { if (inb) { printf "%s%c", buf, 0; inb = 0 } next }
    inb       { buf = buf $0 "\n" }
  ' "$md")
  echo "$md: $n d2 fences"
done
exit $fail
