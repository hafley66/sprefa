#!/usr/bin/env bash
# probe.sh NAME -- compile probes/NAME.dl6 through the real text door and
# report either OK or the refusal term. Output lands in out/NAME.ts.
LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$LAB/out"
echo "===== $1 ====="
if bash "$LAB/../../compile/scripts/compile_dl6.sh" "$LAB/probes/$1.dl6" "$LAB/out/$1.ts" 2>"$LAB/out/$1.err"; then
  echo "COMPILED  ($(wc -l < "$LAB/out/$1.ts") lines)"
else
  echo "REFUSED"
  head -20 "$LAB/out/$1.err"
fi
