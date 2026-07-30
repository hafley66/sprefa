#!/usr/bin/env bash
# 3_v5_collect.sh — RECEIPT 3 of the effect chain-and-batch lab.
#
# What v5's `collect(x[, N])` actually did, run rather than read. Three programs
# over the SAME five body solutions and the same hermetic `sh` decl:
#
#   A  no collect          -> one request per solution
#   B  collect(name, 2)    -> ceil(5/2) = 3 requests, values comma-joined
#   C  collect(name)       -> 1 request, the whole set in one hole
#
# The counter is the spawned shell appending a byte to a ledger, so every number
# is a process count.
#
# HERMETIC: SPREFA_CONFIG names a file that does not exist, DL_NO_DAEMON=1,
# DL_STATE_DIR points into the scratch tree, and --db names a scratch file. The
# isolation is ASSERTED (state/invocations.db must exist under the scratch dir),
# not asserted about. Nothing touches ~/.local/state and no daemon is started.
#
# `--settle` is the one-shot effect runtime (src/lib.rs run_settle): it drives
# ticks plus off-tick drains to a fixpoint, so a `@async` program converges in
# process instead of leaving pending_effect rows 'queued'. That is what makes
# this receipt runnable without the daemon.
set -uo pipefail

DL_BIN="${DL_V5_BIN:-$HOME/.cargo/bin/dl}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/v5-collect.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/state"
cd "$WORK"
git init -q

[ -x "$DL_BIN" ] || { printf 'FAIL  no v5 dl binary at %s\n' "$DL_BIN"; exit 1; }
printf 'v5 binary: %s\n' "$DL_BIN"

# One `sh` decl, five slugs, one `@async` rule. Only the effect-arg spelling
# differs between the three programs.
write_program() { # $1 = file, $2 = effect arg text
  cat >"$1" <<EOF
rel slug(name: text).
slug("alpha").
slug("bravo").
slug("charlie").
slug("delta").
slug("echo").

rel batch_resp(body: text).
sh gather(items) -> (body: text) =
  \`printf 's' >> "\$LAB_SPAWNS"; printf '%s\n' "{items}"\`.

batch_resp(body) <- @async slug(name), gather($2) -> (body).

? batch_resp(body).
EOF
}

run_case() { # $1 = label, $2 = effect arg text
  local label="$1" arg="$2"
  local program="$WORK/$label.dl" ledger="$WORK/$label.spawns" db="$WORK/$label.sqlite"
  write_program "$program" "$arg"
  : >"$ledger"
  LAB_SPAWNS="$ledger" SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 \
    DL_STATE_DIR="$WORK/state" \
    "$DL_BIN" "$program" --db "$db" --settle >"$WORK/$label.out" 2>"$WORK/$label.err"
  local status=$?
  if [ $status -ne 0 ]; then
    printf 'FAIL  %s exited %s: %s\n' "$label" "$status" "$(tail -5 "$WORK/$label.err")"
    exit 1
  fi
  local spawns rows requests
  spawns="$(wc -c <"$ledger" | tr -d ' ')"
  rows="$(grep -c . "$WORK/$label.out" 2>/dev/null || echo 0)"
  requests="$(sqlite3 -noheader "$db" \
    "select group_concat(args_json, ' | ') from pending_effect where kind='gather';" 2>/dev/null)"
  printf '  %-22s solutions=5  spawns=%s  batch_flag=%s\n' \
    "$label" "$spawns" \
    "$(sqlite3 -noheader "$db" "select distinct batch from pending_effect where kind='gather';" | tr '\n' ',')"
  printf '  %-22s requests: %s\n' "" "$requests"
  printf '  %-22s answers:\n' ""
  sed -n '/^? batch_resp/,/rows)/p' "$WORK/$label.out" | sed 's/^/      /'
  printf '\n'
}

printf '\nRECEIPT 3: v5 `collect` over five body solutions\n'
run_case no_collect       'name'
run_case collect_chunk_2  'collect(name, 2)'
run_case collect_whole    'collect(name)'

# ── the isolation assertion ─────────────────────────────────────────────────
if [ -f "$WORK/state/invocations.db" ]; then
  printf 'PASS  isolation: v5 wrote its state under DL_STATE_DIR=%s\n' "$WORK/state"
else
  printf 'FAIL  v5 did not write state under DL_STATE_DIR; the run was not isolated\n'
  exit 1
fi

# ── collect's non-collected-arg rule, as a loud failure ─────────────────────
# `collect` batches ONE request, so an arg that VARIES across body solutions is
# a named error rather than a silent split. This is the compatibility rule v6's
# grouping states in TypeScript, stated in v5 as a message.
cat >"$WORK/vary.dl" <<'EOF'
rel slug(name: text, owner: text).
slug("alpha", "one").
slug("bravo", "two").

rel batch_resp(body: text).
sh gather(owner, items) -> (body: text) =
  `printf '%s %s\n' "{owner}" "{items}"`.

batch_resp(body) <- @async slug(name, owner), gather(owner, collect(name)) -> (body).

? batch_resp(body).
EOF
SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 DL_STATE_DIR="$WORK/state" \
  "$DL_BIN" "$WORK/vary.dl" --db "$WORK/vary.sqlite" --settle >"$WORK/vary.out" 2>"$WORK/vary.err"
vary_status=$?
printf 'RECEIPT 3b: a non-collected arg that varies across solutions\n'
printf '  exit=%s\n' "$vary_status"
grep -o 'collect effect.*' "$WORK/vary.err" | head -1 | sed 's/^/  /'
printf '  answer rows = %s\n' \
  "$(sqlite3 -noheader "$WORK/vary.sqlite" 'select count(*) from rel_batch_resp_txt;' 2>/dev/null)"

# ── collect is absent from v5's own self-describing op catalog ──────────────
cat >"$WORK/cat.dl" <<'EOF'
? op_catalog(op, kind, syntax, doc).
EOF
SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 DL_STATE_DIR="$WORK/state" \
  "$DL_BIN" "$WORK/cat.dl" --db "$WORK/cat.sqlite" >"$WORK/cat.out" 2>/dev/null
printf '\nRECEIPT 3c: v5 op_catalog rows mentioning collect or async\n'
printf '  op_catalog rows total = %s\n' \
  "$(sqlite3 -noheader "$WORK/cat.sqlite" 'select count(*) from rel_op_catalog_txt;' 2>/dev/null)"
printf '  rows whose op is collect or async = %s\n' \
  "$(sqlite3 -noheader "$WORK/cat.sqlite" \
     "select count(*) from rel_op_catalog_txt where op in ('collect','async','effect','sh');" 2>/dev/null)"
