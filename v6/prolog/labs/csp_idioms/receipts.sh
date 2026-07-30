#!/usr/bin/env bash
#
# receipts.sh : CSP idioms lab, the runnable half.
#
#   bash v6/prolog/labs/csp_idioms/receipts.sh
#
# Exit 0 means every receipt held. Nonzero means at least one did not.
#
# Four legs:
#
#   1. TWO-DOOR grading. Each idiom's .dl6 runs through the reference engine
#      (compile/scripts/dl6_oracle.pl) AND through the served tsv2 engine
#      (bop serve on an ephemeral port, driven by curl only); the two tick logs
#      are compared per rel.
#   2. DETERMINISM. Idioms with same-tick multi-arrivals are run under two
#      arrival orderings and diffed, so the dependence is stated not hidden.
#   3. SILENT-WRONG receipts. Three CSP properties that the language accepts and
#      then violates, each pinned by asserting the WRONG output that is actually
#      produced. These are the lab's findings; they are not fixed here.
#   4. ERROR-SURFACE receipts. The reserved-word trap, pinned both ways.
#
# Hermetic: SPREFA_CONFIG points at a path that does not exist, DL_NO_DAEMON=1,
# every server is :memory: on an ephemeral port. Nothing reads or writes
# ~/.local/state and no daemon is spoken to.
#
# THE ONE NORMALIZATION, and its reason. Tick NUMBERS are not compared; each
# REL's own sequence of signed rows is. The served engine and the schedule-fed
# oracle place drain ticks differently -- the runtime-bridge arc measured and
# recorded exactly this ("a carrying tick with an empty queue drains: 3 ticks
# fed vs 4 served, same deltas"). This is the same normalization, and the same
# justification, as v6/prolog/labs/rel_as_stream/receipts.sh; receipt (S1) is
# the proof it still discriminates.

set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
ORACLE_DIR="$REPO/v6/prolog/compile/scripts"

export SPREFA_CONFIG=/nonexistent/csp-idioms.toml
export DL_NO_DAEMON=1

PASS=0
FAIL=0

ok()  { printf 'PASS %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf 'FAIL %s\n' "$1"; FAIL=$((FAIL + 1)); }

command -v swipl   >/dev/null || { echo "swipl is required";   exit 2; }
command -v curl    >/dev/null || { echo "curl is required";    exit 2; }
command -v python3 >/dev/null || { echo "python3 is required"; exit 2; }

# ── the per-rel delta sequence, the thing actually compared ──────────────────

read -r -d '' SEQUENCE_PY <<'PY' || true
import json, sys
order, sequences = [], {}
for raw in sys.stdin:
    raw = raw.strip()
    if raw.startswith("data: "):
        raw = raw[6:]
    if not raw:
        continue
    tick = json.loads(raw)
    for rel in sorted(tick.get("deltas", {})):
        delta = tick["deltas"][rel]
        if rel not in sequences:
            order.append(rel)
            sequences[rel] = []
        for row in delta.get("del", []):
            sequences[rel].append("%s - %s" % (rel, json.dumps(row, separators=(",", ":"))))
        for row in delta.get("add", []):
            sequences[rel].append("%s + %s" % (rel, json.dumps(row, separators=(",", ":"))))
for rel in sorted(order):
    for line in sequences[rel]:
        print(line)
PY

sequence() { python3 -c "$SEQUENCE_PY"; }

oracle_log() {
  local program="$1" schedule="$2"
  ( cd "$ORACLE_DIR" && swipl -q -l dl6_oracle.pl \
      -g "oracle('$program','$schedule')" -g halt 2>/dev/null )
}

served_log() {
  local program="$1" schedule="$2"
  local scratch port pid capture batch count index
  scratch="$(mktemp -d)"
  ( cd "$TSV2" && node --experimental-transform-types cli/bop.ts \
      serve --port 0 --db ':memory:' >"$scratch/serve.log" 2>&1 ) &
  pid=$!
  port=""
  for _ in $(seq 1 200); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  if [ -z "$port" ]; then
    echo "SERVER NEVER LISTENED" >&2
    kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1
  fi
  local base="http://127.0.0.1:$port" loaded
  loaded="$(curl -s -X POST --data-binary @"$program" "$base/program")"
  case "$loaded" in
    *'"loaded":true'*) : ;;
    *) echo "LOAD FAILED: $loaded" >&2; kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1 ;;
  esac
  ( curl -sN "$base/ticks" >"$scratch/ticks" ) &
  capture=$!
  sleep 0.5
  count="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))))' "$schedule")"
  for (( index = 0; index < count; index++ )); do
    batch="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))[int(sys.argv[2])]))' "$schedule" "$index")"
    curl -s -X POST -d "{\"batch\":$batch}" "$base/arrivals" >/dev/null
    sleep 0.35
  done
  sleep 0.8
  kill "$capture" 2>/dev/null
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  cat "$scratch/ticks"
  rm -rf "$scratch"
}

two_door() {
  local name="$1" program="$HERE/$2" schedule="$HERE/$3"
  local scratch
  scratch="$(mktemp -d)"
  oracle_log "$program" "$schedule" | sequence >"$scratch/oracle"
  served_log "$program" "$schedule" | sequence >"$scratch/served"
  if [ ! -s "$scratch/oracle" ]; then
    bad "$name (oracle produced nothing)"
  elif diff -u "$scratch/oracle" "$scratch/served" >"$scratch/diff"; then
    ok "$name ($(wc -l <"$scratch/oracle" | tr -d ' ') deltas, both doors)"
  else
    bad "$name"
    sed -n 1,30p "$scratch/diff"
  fi
  rm -rf "$scratch"
}

# grep the oracle log for one exact delta line
has_delta() {
  local program="$HERE/$1" schedule="$HERE/$2" want="$3"
  oracle_log "$program" "$schedule" | sequence | grep -qxF "$want"
}

echo "── leg 1: two-door grading, all nine idioms ─────────────────────────────"

two_door "(1) buffered channel: one take per drain, keep(count(3)) evicts" \
         buffered.dl6 buffered.schedule.json
two_door "(2) worker pool over one queue" \
         workerpool.dl6 workerpool.schedule.json
two_door "(3) pipeline, three stages" \
         pipeline.dl6 pipeline.schedule.json
two_door "(4a) fan-in, two producers one ordered channel" \
         fanin.dl6 fanin.schedule.json
two_door "(4b) fan-out, N readers each at its own pace" \
         fanout.dl6 fanout.schedule.json
two_door "(5) select, priority arm, loser stays queued" \
         select.dl6 select.schedule.json
two_door "(6) timeout against a world-fed clock" \
         timeout.dl6 timeout.schedule.json
two_door "(7) done channel stops every taker" \
         done.dl6 done.schedule.json
two_door "(9) semaphore, K=2" \
         semaphore.dl6 semaphore.schedule.json
echo "    (8) rendezvous is NOT graded here -- it diverges; see receipt (W4)."

echo
echo "── leg 2: determinism under two arrival orderings ───────────────────────"

# The question the header asks: does any idiom's answer depend on WITHIN-TICK
# arrival order? Each pair below is the same schedule with one tick's arrivals
# written in the opposite order. Identical graded output means the answer is a
# function of term order, not of arrival order -- which is the stronger and more
# reproducible contract, and is what both idioms turn out to have.
determinism() {
  local name="$1" program="$HERE/$2" a="$HERE/$3" b="$HERE/$4"
  local scratch
  scratch="$(mktemp -d)"
  oracle_log "$program" "$a" | sequence >"$scratch/a"
  oracle_log "$program" "$b" | sequence >"$scratch/b"
  if [ ! -s "$scratch/a" ]; then
    bad "$name (produced nothing)"
  elif diff -u "$scratch/a" "$scratch/b" >"$scratch/diff"; then
    ok "$name (arrival order within a tick does not change the answer)"
  else
    bad "$name -- ARRIVAL-ORDER DEPENDENT"
    sed -n 1,20p "$scratch/diff"
  fi
  rm -rf "$scratch"
}

determinism "(D1) fan-in ordinal assignment" \
            fanin.dl6 fanin.schedule.json fanin.perturbed.schedule.json
determinism "(D2) worker pool assignment" \
            workerpool.dl6 workerpool.schedule.json workerpool.perturbed.schedule.json

echo
echo "── leg 3: silent-wrong receipts (findings, not fixes) ────────────────────"

# (W1) WORKER POOL EXACTLY-ONCE IS VIOLATED. Two workers polling in the SAME
#      tick both read the same ready_min and both receive item 1. `taken` is
#      keyed so it collapses to one row and the queue advances once, but
#      `assigned` -- the rel that represents the work actually handed out --
#      carries the item TWICE. Go's runtime guarantees the opposite. Pinned by
#      asserting the wrong rows, so that fixing the engine turns this receipt
#      red on purpose.
if has_delta workerpool.dl6 workerpool.schedule.json 'assigned + [1,"w1","a"]' \
&& has_delta workerpool.dl6 workerpool.schedule.json 'assigned + [1,"w2","a"]'; then
  ok '(W1) worker pool double-delivers item 1 to both same-tick pollers (WRONG, pinned)'
else
  bad '(W1) the double-delivery receipt no longer reproduces -- re-grade the finding'
fi

# (W2) SEMAPHORE CAP IS BREACHED WITHIN A TICK. Three acquires arriving together
#      with nothing held all pass the guard, because each reads the same frozen
#      pre-edge state. held_count settles at 3 against a declared cap of 2.
if has_delta semaphore.dl6 semaphore.schedule.json 'held_count + [3]'; then
  ok '(W2) semaphore admits 3 leases against K=2 in one tick (WRONG, pinned)'
else
  bad '(W2) the cap-breach receipt no longer reproduces -- re-grade the finding'
fi

# (W3) THE NAIVE SEMAPHORE IS A DEAD PROGRAM. count() over an empty set yields
#      NO ROW rather than 0, so the very first acquire fails its guard and the
#      set never becomes non-empty. It compiles clean on both doors and grants
#      nothing, with no diagnostic anywhere. Receipt: `granted` never appears.
if oracle_log "$HERE/semaphore_naive.dl6" "$HERE/semaphore_naive.schedule.json" \
     | sequence | grep -q '^granted '; then
  bad '(W3) the naive semaphore now grants something -- re-grade the finding'
else
  ok '(W3) naive semaphore grants ZERO leases, silently, on a clean compile (WRONG, pinned)'
fi

# (W4) TWO-DOOR DIVERGENCE ON A DERIVED-TRIGGER PROGRAM. rendezvous fires off a
#      DERIVED trigger (`beat`), so the served engine needs an extra carry tick
#      per meeting: 15 served ticks against the oracle's 8. That alone would be
#      absorbed by the per-rel normalization. What is NOT absorbed is that an
#      AGGREGATE over the carried rel takes a transient value during the extra
#      tick and publishes it: waiting_receivers emits three +[1]/-[1] pairs on
#      the served side against one on the oracle. The oracle never observes the
#      intermediate state; the served engine does.
#
#      This is a tick-log divergence between the two graded targets on a program
#      both of them ACCEPT -- the cross-target log contract does not hold here.
#      Same class as review-A4 (oracle/emitter divergence with zero fixture
#      coverage). Pinned as the wrong-but-current behaviour: this receipt goes
#      red when the divergence is fixed, which is the intent.
w4_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  oracle_log "$HERE/rendezvous.dl6" "$HERE/rendezvous.schedule.json" | sequence >"$scratch/oracle"
  served_log "$HERE/rendezvous.dl6" "$HERE/rendezvous.schedule.json" | sequence >"$scratch/served"
  local o s
  o="$(grep -c '^waiting_receivers ' "$scratch/oracle" || true)"
  s="$(grep -c '^waiting_receivers ' "$scratch/served" || true)"
  if [ -s "$scratch/oracle" ] && [ -s "$scratch/served" ] && [ "$o" = 2 ] && [ "$s" = 6 ]; then
    ok "(W4) rendezvous diverges: waiting_receivers 2 deltas oracle / 6 served (WRONG, pinned)"
  else
    bad "(W4) divergence receipt no longer reproduces (oracle=$o served=$s) -- re-grade"
  fi
  rm -rf "$scratch"
}
w4_receipt

echo
echo "── leg 4: error surface ─────────────────────────────────────────────────"

check_out() {
  ( cd "$TSV2" && npm run --silent bop -- check "$1" 2>&1 )
}

# (E2a) A reserved rx word used as an ordinary rel name is the natural CSP
#       naming mistake, and it produces a raw dl_parse_error carrying a LIST OF
#       CHARACTER CODES -- no word named, no line, and the dump runs from the
#       offending statement to end of file.
if check_out "$HERE/reserved_word_bare.dl6" | grep -q 'dl_parse_error(statement,\[[0-9]'; then
  ok '(E2a) rel named `subscribe` => unreadable char-code parse dump (pinned)'
else
  bad '(E2a) the char-code dump no longer reproduces -- re-grade the finding'
fi

# (E2b) The SAME word with an argument that happens to parse as a rel atom gets
#       the proper named refusal. So the diagnostic quality depends on the shape
#       of the argument, not on the mistake.
if check_out "$HERE/reserved_word_relatom.dl6" | grep -q 'unsupported_construct(lifecycle_arm(subscribe))'; then
  ok '(E2b) same word, rel-atom argument => named refusal (the good path exists)'
else
  bad '(E2b) the named refusal no longer reproduces -- re-grade the finding'
fi

echo
echo "── leg 5: sabotage, proving leg 1 discriminates ─────────────────────────"

# (S1) Leg 1 discards tick numbers, so it must be shown that it still catches a
#      content change. One character in the buffered channel's numbering rule
#      (At + 1 becomes At + 2) must make the graded sequence differ.
sabotage_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  sed 's/NextAt := At + 1/NextAt := At + 2/' "$HERE/buffered.dl6" >"$scratch/sabotaged.dl6"
  oracle_log "$HERE/buffered.dl6" "$HERE/buffered.schedule.json" | sequence >"$scratch/good"
  oracle_log "$scratch/sabotaged.dl6" "$HERE/buffered.schedule.json" | sequence >"$scratch/bad"
  if [ -s "$scratch/bad" ] && ! diff -q "$scratch/good" "$scratch/bad" >/dev/null; then
    ok '(S1) sabotage: one changed increment makes the graded sequence differ'
  else
    bad '(S1) sabotage did not register -- the comparison is not discriminating'
  fi
  rm -rf "$scratch"
}
sabotage_receipt

echo
printf '%s PASS %s FAIL\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
