#!/usr/bin/env bash
# Smoke b: rule arity 0/1/2 + sqlite drain at end of run.
#
# Top-level rule defs auto-fire once each. Body terminal cursors push
# RuleRows; at exit, sprefa-run dumps each rule to its own SQLite table
# named <sprf-stem>__<rule-name> with one TEXT column per capture name
# seen across rows + an `id INTEGER PRIMARY KEY`.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/rule_arity_smoke.sprf"
db="crates/server/fixtures/rule_arity_smoke.db"
root="crates/server"

rm -f "$db"
"$bin" "$fixture" --root "$root" >/dev/null

[ -f "$db" ] || { echo "FAIL: db not created at $db" >&2; exit 1; }

# Schema (v1 parity): id + paired (cap, cap_norm) per emitted column.
# Synthetic cursor fields lead: repo + rev (fs is None for these
# top-level rules without an fs() op upstream). User captures follow.
# arity 0 → id, repo+norm, rev+norm, R+norm        = 7 cols
# arity 1 → id, repo+norm, rev+norm, P+norm, R+norm = 9 cols
# arity 2 → id, repo+norm, rev+norm, A+norm, B+norm, R+norm = 11 cols
zero_cols=$(sqlite3 "$db" "PRAGMA table_info(rule_arity_smoke__zero)" | wc -l | tr -d ' ')
one_cols=$(sqlite3 "$db" "PRAGMA table_info(rule_arity_smoke__one)" | wc -l | tr -d ' ')
two_cols=$(sqlite3 "$db" "PRAGMA table_info(rule_arity_smoke__two)" | wc -l | tr -d ' ')
[ "$zero_cols" = "7"  ] || { echo "FAIL: zero arity expected 7 cols, got $zero_cols"  >&2; exit 1; }
[ "$one_cols"  = "9"  ] || { echo "FAIL: one arity expected 9 cols, got $one_cols"   >&2; exit 1; }
[ "$two_cols"  = "11" ] || { echo "FAIL: two arity expected 11 cols, got $two_cols" >&2; exit 1; }

# Norm indices: 3 (zero) + 4 (one) + 5 (two) = 12.
idx_count=$(sqlite3 "$db" "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'rule_arity_smoke__%_norm_idx'")
[ "$idx_count" = "12" ] || { echo "FAIL: expected 12 norm indices, got $idx_count" >&2; exit 1; }

# Synthetic cursor fields landed.
repo=$(sqlite3 "$db" "SELECT repo FROM rule_arity_smoke__zero")
rev=$(sqlite3 "$db" "SELECT rev FROM rule_arity_smoke__zero")
[ "$repo" = "server" ] || { echo "FAIL: synthetic repo expected 'server', got '$repo'" >&2; exit 1; }
[ "$rev"  = "wt"     ] || { echo "FAIL: synthetic rev expected 'wt', got '$rev'"       >&2; exit 1; }

# rev_norm is sprf_norm("wt") = "wt".
rev_norm=$(sqlite3 "$db" "SELECT rev_norm FROM rule_arity_smoke__zero")
[ "$rev_norm" = "wt" ] || { echo "FAIL: rev_norm expected 'wt', got '$rev_norm'" >&2; exit 1; }

# FTS5 trigram virtual table per rule.
for r in zero one two; do
    fts_present=$(sqlite3 "$db" "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='rule_arity_smoke__${r}_fts'")
    [ "$fts_present" = "1" ] || { echo "FAIL: rule $r missing FTS5 table" >&2; exit 1; }
done

# FTS5 trigram fuzzy query: 'serv' should match the row R=server.
fts_hit=$(sqlite3 "$db" "SELECT rowid FROM rule_arity_smoke__zero_fts WHERE rule_arity_smoke__zero_fts MATCH 'serv'")
[ "$fts_hit" = "1" ] || { echo "FAIL: FTS5 'serv' MATCH expected rowid=1, got '$fts_hit'" >&2; exit 1; }

# Each rule auto-fires once → 1 row per table.
for r in zero one two; do
    n=$(sqlite3 "$db" "SELECT COUNT(*) FROM rule_arity_smoke__$r")
    [ "$n" = "1" ] || { echo "FAIL: rule $r expected 1 row, got $n" >&2; exit 1; }
done

# User-bound capture R (via repo($R)) lands alongside synthetic repo.
got=$(sqlite3 "$db" "SELECT R FROM rule_arity_smoke__zero")
[ "$got" = "server" ] || { echo "FAIL: zero R column expected 'server', got '$got'" >&2; exit 1; }

# Param captures auto-fire-bound to empty string.
p=$(sqlite3 "$db" "SELECT P FROM rule_arity_smoke__one")
[ "$p" = "" ] || { echo "FAIL: one P column expected empty, got '$p'" >&2; exit 1; }
a=$(sqlite3 "$db" "SELECT A FROM rule_arity_smoke__two")
b=$(sqlite3 "$db" "SELECT B FROM rule_arity_smoke__two")
[ "$a" = "" ] && [ "$b" = "" ] || { echo "FAIL: two A/B expected empty, got A='$a' B='$b'" >&2; exit 1; }

echo "OK"
