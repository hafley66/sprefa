#!/usr/bin/env bash
# THE FILE-SIZE LAW (Chris, 2026-07-11): no source file over 500 lines.
# Target 300; 300-500 needs a written reason; >500 means STOP and bring
# 3 split ideas or 1 justification (a scripts/filesize-allow.txt row with a
# plan pointer). The allowlist may only SHRINK — CI diffs it.
#
# This script is the always-on prong (verify.sh + hook + CI). A dl-native
# twin rides the planned `file_lines` builtin (file.content is a hash, so
# pure-dl line counts need engine support — ledgered).
#
# Exit 0 = clean or grandfathered-only. Exit 2 = a NEW file crossed 500.
set -uo pipefail
cd "$(dirname "$0")/.."
allow=scripts/filesize-allow.txt
soft=300
hard=500

offenders=$(git ls-files 'src/*.rs' 'src/**/*.rs' | xargs wc -l 2>/dev/null \
    | awk -v h="$hard" '$2 != "total" && $1 > h {print $1" "$2}' | sort -k2)
softies=$(git ls-files 'src/*.rs' 'src/**/*.rs' | xargs wc -l 2>/dev/null \
    | awk -v s="$soft" -v h="$hard" '$2 != "total" && $1 > s && $1 <= h {print $2}' | wc -l | tr -d ' ')

new=0
grandfathered=0
while read -r count path; do
    [ -z "${path:-}" ] && continue
    if grep -qxF "$path" "$allow" 2>/dev/null; then
        grandfathered=$((grandfathered + 1))
    else
        echo "[filesize] ERROR: $path is $count lines (hard budget $hard) — STOP: propose 3 splits or 1 justification (then allowlist with a plan pointer)" >&2
        new=1
    fi
done <<< "$offenders"

# The one-line running debt counter Chris asked for — always printed.
echo "[filesize] you still have $grandfathered unacceptable files (>${hard} lines, grandfathered) and $softies files in the 300-500 needs-a-reason band"

[ "$new" -ne 0 ] && exit 2
exit 0
