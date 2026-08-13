#!/usr/bin/env bash
# The live `-L lanes` server is read for its md5 here and never written.
set -uo pipefail
cd "$(dirname "$0")/.."

before=$(tmux -L lanes ls 2>&1 | md5 -q)
cargo test 2>&1 | tail -3
status=$?
after=$(tmux -L lanes ls 2>&1 | md5 -q)

procs=$(ps -eo comm,args | awk '$1 ~ /tmux$/ && /boop-test/' | wc -l | tr -d ' ')
socks=$(ls /private/tmp/tmux-"$(id -u)"/ 2>/dev/null | grep -c 'boop-test' || true)

echo "tmux-leak-rail: leaked_processes=$procs leaked_sockets=$socks lanes_md5_before=$before lanes_md5_after=$after"

if [ "$procs" -ne 0 ] || [ "$socks" -ne 0 ]; then
  echo "tmux-leak-rail: FAIL, the suite left tmux state behind" >&2
  exit 1
fi
if [ "$before" != "$after" ]; then
  echo "tmux-leak-rail: FAIL, the live lanes server changed during the suite" >&2
  exit 1
fi
exit $status
