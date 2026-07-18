#!/usr/bin/env bash
# dl-trace: one-shot diagnostic capture of a running dl daemon.
# Answers, without guessing: what is the daemon burning CPU on (stack sample),
# is the machine swapping, what does dl itself say it is doing (why/status/
# invocations), and which root's trail is moving.
#
# Usage:
#   scripts/dl-trace.sh [seconds]     capture (default 10s sample window)
#   DL_TRACE_DIR=/path override the output base (default /tmp/dl-trace)
#
# Output: a timestamped dir with ps/top/sample/vm_stat/swap/why/status/
# invocations/log tails, plus a hottest-frames digest printed to stdout.
set -uo pipefail
DUR=${1:-10}
TS=$(date +%Y%m%d-%H%M%S)
OUT=${DL_TRACE_DIR:-/tmp/dl-trace}/$TS
mkdir -p "$OUT"

PID=$(pgrep -f 'dl daemon serve' | head -1 || true)
if [ -z "${PID:-}" ]; then
  echo "no 'dl daemon serve' process running"
  exit 1
fi
echo "daemon pid $PID, sampling ${DUR}s -> $OUT"

# process + thread state
ps -o pid,ppid,pcpu,pmem,rss,vsz,nice,etime,command -p "$PID" > "$OUT/ps.txt"
top -pid "$PID" -l 2 -stats pid,cpu,th,time,mem,pgrp > "$OUT/top.txt" 2>&1 || true

# CPU stacks: the line-level answer. sample(1) needs no special perms for
# same-user processes.
sample "$PID" "$DUR" -file "$OUT/sample.txt" >/dev/null 2>&1 \
  || echo "sample failed (SIP or perms?)" > "$OUT/sample.txt"

# memory + swap pressure ("check swaps")
vm_stat > "$OUT/vm_stat.txt"
sysctl vm.swapusage > "$OUT/swap.txt" 2>/dev/null || true
footprint "$PID" > "$OUT/footprint.txt" 2>&1 || true

# dl's own story (read-only surfaces; alarm-bounded — a busy daemon can hang
# socket verbs, and the capture must never hang with it)
bounded() { perl -e 'alarm 8; exec @ARGV' -- "$@"; }
bounded dl daemon why  > "$OUT/why.txt" 2>&1 || true
bounded dl daemon status > "$OUT/status.txt" 2>&1 || true
bounded dl daemon invocations > "$OUT/invocations.txt" 2>&1 || true
tail -60 "$HOME/.local/state/sprefa/daemon.log" > "$OUT/daemon.log.tail" 2>&1 || true
LOGDIR="$HOME/.local/state/sprefa/log"
[ -d "$LOGDIR" ] && tail -60 "$LOGDIR"/*.log > "$OUT/dl.log.tail" 2>&1 || true

# which root's perf trail is moving (mtime + tail each)
for r in "$HOME/.local/state/sprefa/.dl/perf.jsonl" "$HOME"/projects/*/.dl/perf.jsonl "$HOME"/projects/*/*/.dl/perf.jsonl; do
  [ -f "$r" ] || continue
  key=$(echo "$r" | tr '/' '_')
  { stat -f '%Sm %N' "$r"; tail -20 "$r"; } > "$OUT/trail$key" 2>/dev/null || true
done

# hottest-frames digest: leaf frames weighted by sample counts
grep -E '^\s+[0-9]{2,}' "$OUT/sample.txt" \
  | sed 's/^[[:space:]]*//' | sort -rn | head -30 > "$OUT/hot.txt" || true

echo "== swap =="
cat "$OUT/swap.txt" 2>/dev/null
echo "== cpu =="
tail -1 "$OUT/ps.txt"
echo "== hottest frames (count frame) =="
head -20 "$OUT/hot.txt"
echo "== capture dir: $OUT =="
