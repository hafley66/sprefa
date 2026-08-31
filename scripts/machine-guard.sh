#!/usr/bin/env bash
# Watchdog: kills the heaviest cargo/rustc/extract/test process when the
# machine crosses a load or memory-pressure line. Logs to $GUARD_LOG.
# Usage: bash scripts/machine-guard.sh &   (poll every 5 s)
set -u
NC=$(sysctl -n hw.ncpu)
LOAD_MAX=${GUARD_LOAD_MAX:-$(( NC * 3 / 2 ))}
GUARD_LOG=${GUARD_LOG:-$HOME/.agent/machine-guard.log}
PAT='cargo test|cargo build|rustc |target/release/deps|target/release/extract|target/debug/deps'
while sleep "${GUARD_POLL:-5}"; do
  load=$(sysctl -n vm.loadavg | awk '{print int($2)}')
  free_pct=$(memory_pressure 2>/dev/null | awk '/percentage/ {gsub("%","",$NF); print $NF}')
  free_pct=${free_pct:-100}
  if [ "$load" -gt "$LOAD_MAX" ] || [ "$free_pct" -lt "${GUARD_FREE_MIN:-8}" ]; then
    victim=$(ps -Ao pid,rss,command | grep -E "$PAT" | grep -v grep | grep -v machine-guard | sort -k2 -nr | head -1)
    [ -z "$victim" ] && continue
    pid=$(echo "$victim" | awk '{print $1}')
    echo "$(date '+%F %T') load=$load free=${free_pct}% kill $victim" >> "$GUARD_LOG"
    kill -TERM "$pid" 2>/dev/null; sleep 3; kill -KILL "$pid" 2>/dev/null
  fi
done
