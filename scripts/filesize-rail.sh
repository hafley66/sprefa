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

# Keep the shell enforcement prong and the dl advisory facts synchronized. A
# stale copy makes a grandfathered file look intentional in one rail and
# missing in the other, so drift is an error rather than a quiet warning.
allow_paths=$(mktemp)
fact_paths=$(mktemp)
trap 'rm -f "$allow_paths" "$fact_paths"' EXIT
awk '!/^#/ && NF {print $1}' "$allow" | sort -u >"$allow_paths"
sed -n 's/^[[:space:]]*big_file_ok("\([^"]*\)".*/\1/p' .dl/file-size.dl | sort -u >"$fact_paths"
drift=0
while read -r path; do
    [ -z "${path:-}" ] && continue
    echo "[filesize] ERROR: $allow lists $path but .dl/file-size.dl has no big_file_ok fact" >&2
    drift=1
done < <(comm -23 "$allow_paths" "$fact_paths")
while read -r path; do
    [ -z "${path:-}" ] && continue
    echo "[filesize] ERROR: .dl/file-size.dl lists $path but $allow has no allowlist row" >&2
    drift=1
done < <(comm -13 "$allow_paths" "$fact_paths")
if [ "$drift" -ne 0 ]; then
    echo "[filesize] WARNING: allowlist and dl big_file_ok path sets drift" >&2
fi

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
