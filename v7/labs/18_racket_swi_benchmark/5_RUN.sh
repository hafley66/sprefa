#!/usr/bin/env bash
set -euo pipefail

lab_dir="$(cd "$(dirname "$0")" && pwd)"
artifact_dir="${RACKET_SWI_BENCH_OUT:?set RACKET_SWI_BENCH_OUT to an existing /private/tmp directory}"

case "$artifact_dir" in
  /private/tmp/*) ;;
  *) echo "RACKET_SWI_BENCH_OUT must be under /private/tmp" >&2; exit 2 ;;
esac

test -d "$artifact_dir"

{
  racket --version
  raco pkg show -d datalog racklog
  swipl --version
  du -sk /opt/homebrew/Cellar/minimal-racket/9.3
  du -sk "$HOME/Library/Racket/9.3"
} > "$artifact_dir/0_versions-and-install-size.txt"

for repetition in 1 2 3 4 5; do
  racket "$lab_dir/2_RACKET_DATALOG.rkt" assert 100000
  swipl -q -f "$lab_dir/4_SWI.pl" -- assert 100000

  racket "$lab_dir/2_RACKET_DATALOG.rkt" cycle 100
  swipl -q -f "$lab_dir/4_SWI.pl" -- cycle 100

  racket "$lab_dir/2_RACKET_DATALOG.rkt" all-pairs 50
  swipl -q -f "$lab_dir/4_SWI.pl" -- all-pairs 50

  racket "$lab_dir/3_RACKLOG.rkt" assert 10000
  racket "$lab_dir/3_RACKLOG.rkt" chain 100
done > "$artifact_dir/1_samples.txt"

{
  racket "$lab_dir/2_RACKET_DATALOG.rkt" cycle 300
  swipl -q -f "$lab_dir/4_SWI.pl" -- cycle 300
  swipl -q -f "$lab_dir/4_SWI.pl" -- cycle 10000
  racket "$lab_dir/2_RACKET_DATALOG.rkt" all-pairs 100
  swipl -q -f "$lab_dir/4_SWI.pl" -- all-pairs 100
} > "$artifact_dir/1a_scale-probes.txt"

racket "$lab_dir/3_RACKLOG.rkt" cycle 10 \
  > "$artifact_dir/2_racklog-cycle.txt"

raco exe -o "$artifact_dir/racket-datalog-benchmark" \
  "$lab_dir/2_RACKET_DATALOG.rkt"
raco distribute "$artifact_dir/racket-datalog-distribution" \
  "$artifact_dir/racket-datalog-benchmark"

swipl -q -g "qsave_program('$artifact_dir/swi-benchmark-state',[goal=main,stand_alone=true]),halt" \
  -t halt -s "$lab_dir/4_SWI.pl"

{
  wc -c "$artifact_dir/racket-datalog-benchmark"
  du -sk "$artifact_dir/racket-datalog-distribution"
  wc -c "$artifact_dir/swi-benchmark-state"
  du -sk /opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl
  file "$artifact_dir/racket-datalog-benchmark" "$artifact_dir/swi-benchmark-state"
  otool -L "$artifact_dir/racket-datalog-benchmark"
  otool -L "$artifact_dir/swi-benchmark-state"
} > "$artifact_dir/3_packaging.txt"
