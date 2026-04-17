#!/usr/bin/env bash
# Smoke driver: runs every _N_*.sh in lexical order. Exits non-zero if any
# script fails. Each sub-script is isolated (resets its own state dir and
# starts/stops its own server).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

source ./_common.sh

fail=0
total_t0="$(now_ms)"
for script in _[0-9]*_*.sh; do
    [ "$script" = "_9_all.sh" ] && continue
    echo "==> $script"
    s_t0="$(now_ms)"
    if bash "./$script"; then
        s_t1="$(now_ms)"
        printf '    OK  (%d ms)\n' "$((s_t1 - s_t0))"
    else
        s_t1="$(now_ms)"
        printf '    FAIL (%d ms)\n' "$((s_t1 - s_t0))"
        fail=1
    fi
done
total_t1="$(now_ms)"
printf '==> total: %d ms\n' "$((total_t1 - total_t0))"
exit "$fail"
