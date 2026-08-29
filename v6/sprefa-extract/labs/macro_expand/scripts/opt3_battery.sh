#!/bin/bash
# Option 3 battery: rustc -Zunpretty=expanded, one run per RA workspace crate.
# Records wall ms, success, expanded bytes vs source bytes (whole src dir).
RA=/Users/chrishafley/projects/rust-analyzer
LOG="$(pwd)/opt3.battery.log"
mkdir -p "$(pwd)/out_opt3"
rm -f "$LOG"
cd "$RA" || exit 1

DONE_CRATES=$(grep -c . "$LOG" 2>/dev/null || true)
for p in $(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(' '.join(x['name'] for x in d['packages']))" | tr ' ' '\n' | tail -n +$((DONE_CRATES+1))); do
  t0=$(python3 -c 'import time; print(time.time())')
  if timeout 600 cargo rustc -p "$p" --offline -q -- -Zunpretty=expanded > "/tmp/exp_$p.rs" 2>"/tmp/exp_$p.err"; then
    ok=OK
  else
    ok=FAIL
  fi
  t1=$(python3 -c 'import time; print(time.time())')
  ms=$(python3 -c "print(int(($t1-$t0)*1000))")
  eb=$(wc -c < "/tmp/exp_$p.rs" | tr -d ' ')
  sb=$(find "$RA/crates/$p" -name '*.rs' -not -path '*/target/*' -exec cat {} + 2>/dev/null | wc -c | tr -d ' ')
  echo -e "$p\t$ok\tms=$ms\texp_bytes=$eb\tsrc_bytes=$sb" >> "$LOG"
done
echo DONE >> "$LOG"
