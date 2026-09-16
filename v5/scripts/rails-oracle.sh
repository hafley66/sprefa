#!/usr/bin/env bash
# Oracle harness for the exe-swap-storm syntax-ban rails.
#
# Proves each rail catches the historical defect it was built from:
#   rail 1 (unordered-select) : commit 80617b6b^ (defect 6, nondeterministic file-set queries)
#   rail 2 (dishonest-flag)   : commit 4d0d24bf^ (scip/catalog hardcoded Ok(true))
#                              commit f48749e0^ (refresh_call_rels hardcoded Ok(true))
#   rail 3 (lossy-dedup)      : HEAD (dataflow.rs first-wins dedup gates)
#
# Run: ./scripts/rails-oracle.sh
# Requires: dl on PATH, git worktree support, ~1-2 min per old worktree.
#
# The rails scope their own scans to src/**/*.rs, so no extra corpus-limiting
# config is needed; each old worktree pays only for its source scan.

set -uo pipefail
cd "$(dirname "$0")/.."

REPO_ROOT="$(pwd)"
DL="${DL:-dl}"
DL_FLAGS="${DL_FLAGS:-}"

# Absolute paths to the rails under test.
RAIL1="$REPO_ROOT/.dl/unordered-select.dl"
RAIL2="$REPO_ROOT/.dl/dishonest-flag.dl"
RAIL3="$REPO_ROOT/.dl/lossy-dedup.dl"

# Temporary worktree roots.
WT1="$(mktemp -d)"
WT2="$(mktemp -d)"
WT3="$(mktemp -d)"

cleanup() {
    local rc=$?
    # Remove worktrees first so Git releases the directories.
    git worktree remove --force "$WT1" 2>/dev/null || true
    git worktree remove --force "$WT2" 2>/dev/null || true
    git worktree remove --force "$WT3" 2>/dev/null || true
    rm -rf "$WT1" "$WT2" "$WT3"
    exit "$rc"
}
trap cleanup EXIT

run_rail() {
    local rail="$1"
    local worktree="$2"
    # The rail's own scan("WORK", "src/**/*.rs", ...) uses the cwd as root.
    (
        cd "$worktree" || exit 1
        DL_NO_DAEMON=1 timeout 900 "$DL" $DL_FLAGS "$rail" --check 2>&1
    )
}

# Extract the SQL snippet rendered inside backticks from a --check warning line.
sql_from_diag() {
    sed -n 's/.*(`\([^`]*\)`).*/\1/p'
}

failures=0
declare -A RESULT

# ---------------------------------------------------------------------------
# Rail 1: unordered SELECT should catch the pre-fix file-set queries.
# ---------------------------------------------------------------------------
echo "[oracle] rail 1: checkout 80617b6b^"
git worktree add --detach "$WT1" "80617b6b^" >/dev/null 2>&1
rail1_old_out="$(run_rail "$RAIL1" "$WT1")"

rail1_old_files=(
    "src/engine/extract/mod.rs"
    "src/engine/extract/node.rs"
    "src/engine/extract/doc.rs"
    "src/engine/extract/text.rs"
    "src/rels/filelines.rs"
)
rail1_old_hits=0
for f in "${rail1_old_files[@]}"; do
    if echo "$rail1_old_out" | grep -q "$f"; then
        rail1_old_hits=$((rail1_old_hits + 1))
    fi
done

if [ "$rail1_old_hits" -eq "${#rail1_old_files[@]}" ]; then
    RESULT[rail1_old]="PASS"
else
    RESULT[rail1_old]="FAIL (found $rail1_old_hits/${#rail1_old_files[@]} files)"
    failures=$((failures + 1))
fi

# Capture the exact SQL fragments that were flagged in the historical files.
# The defect was file-set queries reading from `_file`; restrict to those so
# unrelated SELECTs in the same files do not fail the HEAD-clean assertion.
declare -a rail1_old_sqls=()
for f in "${rail1_old_files[@]}"; do
    while IFS= read -r sql; do
        [ -z "$sql" ] && continue
        # Historical file-set queries all read directly from `_file`.
        echo "$sql" | grep -qF "FROM _file" || continue
        rail1_old_sqls+=("$f:$sql")
    done <<< "$(echo "$rail1_old_out" | grep -E 'warning\[unordered-select\]' | grep "$f" | sql_from_diag)"
done

echo "[oracle] rail 1: run on HEAD"
rail1_head_out="$(run_rail "$RAIL1" "$REPO_ROOT")"

# Guard against a vacuous pass: the old-rev capture must have found the
# file-set SELECTs, otherwise the HEAD-clean loop below asserts nothing.
rail1_head_clean=1
if [ "${#rail1_old_sqls[@]}" -eq 0 ]; then
    echo "[oracle] rail 1 captured no FROM _file SQL fragments from the old rev"
    rail1_head_clean=0
fi
for entry in "${rail1_old_sqls[@]}"; do
    file="${entry%%:*}"
    sql="${entry#*:}"
    if echo "$rail1_head_out" | grep -F "$file" | grep -qF "$sql"; then
        echo "[oracle] rail 1 HEAD still contains old site: $file: $sql"
        rail1_head_clean=0
    fi
done

if [ "$rail1_head_clean" -eq 1 ]; then
    RESULT[rail1_head]="PASS"
else
    RESULT[rail1_head]="FAIL"
    failures=$((failures + 1))
fi

# ---------------------------------------------------------------------------
# Rail 2: dishonest Ok(true) flags.
# ---------------------------------------------------------------------------
echo "[oracle] rail 2: checkout 4d0d24bf^"
git worktree add --detach "$WT2" "4d0d24bf^" >/dev/null 2>&1
rail2_old_out="$(run_rail "$RAIL2" "$WT2")"

rail2_old_files=(
    "src/rels/scip.rs"
    "src/rels/catalog.rs"
)
rail2_old_hits=0
for f in "${rail2_old_files[@]}"; do
    if echo "$rail2_old_out" | grep -q "$f"; then
        rail2_old_hits=$((rail2_old_hits + 1))
    fi
done

if [ "$rail2_old_hits" -eq "${#rail2_old_files[@]}" ]; then
    RESULT[rail2_old_4d0d24bf]="PASS"
else
    RESULT[rail2_old_4d0d24bf]="FAIL (found $rail2_old_hits/${#rail2_old_files[@]} files)"
    failures=$((failures + 1))
fi

echo "[oracle] rail 2: checkout f48749e0^"
git worktree add --detach "$WT3" "f48749e0^" >/dev/null 2>&1
rail2_call_out="$(run_rail "$RAIL2" "$WT3")"

if echo "$rail2_call_out" | grep -q 'src/engine/extract/call.rs'; then
    RESULT[rail2_old_f48749e0]="PASS"
else
    RESULT[rail2_old_f48749e0]="FAIL"
    failures=$((failures + 1))
fi

# ---------------------------------------------------------------------------
# Rail 3: lossy dedup must include dataflow.rs on HEAD.
# ---------------------------------------------------------------------------
echo "[oracle] rail 3: run on HEAD"
rail3_head_out="$(run_rail "$RAIL3" "$REPO_ROOT")"

if echo "$rail3_head_out" | grep -q 'src/engine/extract/dataflow.rs'; then
    RESULT[rail3_head]="PASS"
else
    RESULT[rail3_head]="FAIL"
    failures=$((failures + 1))
fi

# ---------------------------------------------------------------------------
# Report.
# ---------------------------------------------------------------------------
echo ""
echo "rails-oracle results:"
printf "  %-30s %s\n" "rail 1 old (80617b6b^)" "${RESULT[rail1_old]}"
printf "  %-30s %s\n" "rail 1 HEAD clean" "${RESULT[rail1_head]}"
printf "  %-30s %s\n" "rail 2 old (4d0d24bf^)" "${RESULT[rail2_old_4d0d24bf]}"
printf "  %-30s %s\n" "rail 2 old (f48749e0^)" "${RESULT[rail2_old_f48749e0]}"
printf "  %-30s %s\n" "rail 3 HEAD (dataflow.rs)" "${RESULT[rail3_head]}"
echo ""

if [ "$failures" -eq 0 ]; then
    echo "OVERALL: PASS"
    exit 0
else
    echo "OVERALL: FAIL ($failures assertion(s) failed)"
    exit 1
fi
