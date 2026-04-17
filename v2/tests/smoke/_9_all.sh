#!/usr/bin/env bash
# Smoke driver: runs every _N_*.sh in lexical order. Exits non-zero if any
# script fails. Each sub-script is isolated (resets its own state dir and
# starts/stops its own server).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

fail=0
for script in _[0-9]*_*.sh; do
    [ "$script" = "_9_all.sh" ] && continue
    echo "==> $script"
    if bash "./$script"; then
        echo "    OK"
    else
        echo "    FAIL"
        fail=1
    fi
done
exit "$fail"
