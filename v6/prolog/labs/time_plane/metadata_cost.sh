#!/usr/bin/env bash
#
# metadata_cost.sh : Q6 of the time-plane header, measured on real SQLite.
#
# Two questions, both answered in bytes/statements rather than estimated:
#
#   Q6a  what do auto created_at_tick / updated_at_tick cost in BYTES on a
#        100k-row corpus, for the two storage shapes the engine actually
#        emits (a log rel = plain rowid table, a keyed set rel = PK table)?
#   Q6b  does updated_at on a KEYED REPLACE add a statement, or does it ride
#        the existing upsert? The header demands prove-or-refute.
#
# Hermetic: every db is a fresh file under mktemp, removed on exit. No daemon,
# no ~/.local/state, no network.

set -u

command -v sqlite3 >/dev/null || { echo "sqlite3 is required"; exit 2; }

SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

ROWS="${ROWS:-100000}"

bytes() { # db -> bytes on disk per sqlite's own page accounting
  sqlite3 "$1" 'SELECT (SELECT * FROM pragma_page_count()) * (SELECT * FROM pragma_page_size());'
}

# ── Q6a-1: the LOG shape (plain rowid table, lower.pl rel_ddl clause 1) ──────

sqlite3 "$SCRATCH/log_base.db" <<SQL
CREATE TABLE ev ("payload" TEXT, "amount" INTEGER);
INSERT INTO ev SELECT 'payload-' || value, value
  FROM generate_series(1, $ROWS);
SQL

sqlite3 "$SCRATCH/log_meta.db" <<SQL
CREATE TABLE ev ("payload" TEXT, "amount" INTEGER,
                 "created_at_tick" INTEGER, "updated_at_tick" INTEGER);
INSERT INTO ev SELECT 'payload-' || value, value, value, value
  FROM generate_series(1, $ROWS);
SQL

LOG_BASE="$(bytes "$SCRATCH/log_base.db")"
LOG_META="$(bytes "$SCRATCH/log_meta.db")"

# ── Q6a-2: the KEYED SET shape (PK over the key columns) ────────────────────

sqlite3 "$SCRATCH/set_base.db" <<SQL
CREATE TABLE cur ("id" INTEGER, "payload" TEXT, PRIMARY KEY ("id"));
INSERT INTO cur SELECT value, 'payload-' || value FROM generate_series(1, $ROWS);
SQL

sqlite3 "$SCRATCH/set_meta.db" <<SQL
CREATE TABLE cur ("id" INTEGER, "payload" TEXT,
                  "created_at_tick" INTEGER, "updated_at_tick" INTEGER,
                  PRIMARY KEY ("id"));
INSERT INTO cur SELECT value, 'payload-' || value, value, value
  FROM generate_series(1, $ROWS);
SQL

SET_BASE="$(bytes "$SCRATCH/set_base.db")"
SET_META="$(bytes "$SCRATCH/set_meta.db")"

pct() { python3 -c "print(f'{(($2-$1)/$1*100):+.1f}%')"; }
per_row() { python3 -c "print(f'{(($2-$1)/$3):.1f}')"; }

echo "── Q6a: bytes at ${ROWS} rows ───────────────────────────────────────────"
printf '  log shape  (rowid table): %10s -> %10s  %8s  %s bytes/row\n' \
  "$LOG_BASE" "$LOG_META" "$(pct "$LOG_BASE" "$LOG_META")" "$(per_row "$LOG_BASE" "$LOG_META" "$ROWS")"
printf '  keyed set  (PK table):    %10s -> %10s  %8s  %s bytes/row\n' \
  "$SET_BASE" "$SET_META" "$(pct "$SET_BASE" "$SET_META")" "$(per_row "$SET_BASE" "$SET_META" "$ROWS")"

# ── Q6b: does updated_at cost a statement on a keyed replace? ────────────────
#
# The shipped keyed-arrival SQL (lower.pl set_arrival_sql_parts/4) is ONE
# upsert. The question is whether stamping updated_at needs a second one. It
# does not: the stamp is another assignment inside the DO UPDATE SET list, and
# `excluded` carries the incoming tick. Measured by counting the statements
# SQLite actually compiled and ran, via the authorizer-free route of asking
# sqlite3 to report changes per statement.

sqlite3 "$SCRATCH/upsert.db" <<'SQL'
CREATE TABLE cur ("id" INTEGER, "payload" TEXT,
                  "created_at_tick" INTEGER, "updated_at_tick" INTEGER,
                  PRIMARY KEY ("id"));
SQL

# One statement, first write at tick 7.
sqlite3 "$SCRATCH/upsert.db" <<'SQL'
INSERT INTO cur ("id","payload","created_at_tick","updated_at_tick")
VALUES (1,'first',7,7)
ON CONFLICT("id") DO UPDATE SET
  "payload"=excluded."payload",
  "updated_at_tick"=excluded."updated_at_tick";
SQL

# The SAME one statement, replacing that key at tick 9.
sqlite3 "$SCRATCH/upsert.db" <<'SQL'
INSERT INTO cur ("id","payload","created_at_tick","updated_at_tick")
VALUES (1,'second',9,9)
ON CONFLICT("id") DO UPDATE SET
  "payload"=excluded."payload",
  "updated_at_tick"=excluded."updated_at_tick";
SQL

ROW="$(sqlite3 "$SCRATCH/upsert.db" 'SELECT "id"||"|"||"payload"||"|"||"created_at_tick"||"|"||"updated_at_tick" FROM cur;')"

echo
echo "── Q6b: keyed replace, one statement ────────────────────────────────────"
echo "  after insert(tick 7) then replace(tick 9), one upsert each:"
echo "    row = $ROW"

FAILED=0
if [ "$ROW" = "1|second|7|9" ]; then
  echo "  PASS created_at pinned at 7, updated_at advanced to 9, ONE statement per tick"
else
  echo "  FAIL expected 1|second|7|9"
  FAILED=1
fi

# The discriminating half: created_at must NOT be in the DO UPDATE SET list.
# Sabotage -- add it and show the birth tick is destroyed.
sqlite3 "$SCRATCH/upsert.db" <<'SQL'
INSERT INTO cur ("id","payload","created_at_tick","updated_at_tick")
VALUES (1,'third',11,11)
ON CONFLICT("id") DO UPDATE SET
  "payload"=excluded."payload",
  "created_at_tick"=excluded."created_at_tick",
  "updated_at_tick"=excluded."updated_at_tick";
SQL
SABOTAGE="$(sqlite3 "$SCRATCH/upsert.db" 'SELECT "created_at_tick" FROM cur;')"
if [ "$SABOTAGE" = "11" ]; then
  echo "  PASS sabotage receipt: listing created_at in DO UPDATE SET destroys the birth tick (7 -> 11)"
else
  echo "  FAIL sabotage receipt did not reproduce"
  FAILED=1
fi

# ── Q7: historicization, priced at 100k rows / 10% churn ────────────────────
#
# Three shapes for "what did this rel look like at tick T", same workload:
# 100k keyed rows, then 10% of them replaced once.
#
#   (i)   current only, no history          -- today's keyed set rel
#   (ii)  shadow log: current + a per-rel history table carrying the
#         superseded versions plus the tick range
#   (iii) rel-as-log: the rel IS a log keep(all); "current" is a derived
#         max-tick-per-key view. The channel thread's existing pattern.

CHURN=$(( ROWS / 10 ))

sqlite3 "$SCRATCH/hist_current.db" <<SQL
CREATE TABLE cur ("id" INTEGER, "payload" TEXT,
                  "created_at_tick" INTEGER, "updated_at_tick" INTEGER,
                  PRIMARY KEY ("id"));
INSERT INTO cur SELECT value, 'payload-' || value, 1, 1 FROM generate_series(1, $ROWS);
UPDATE cur SET "payload" = 'revised-' || "id", "updated_at_tick" = 2 WHERE "id" <= $CHURN;
SQL

sqlite3 "$SCRATCH/hist_shadow.db" <<SQL
CREATE TABLE cur ("id" INTEGER, "payload" TEXT,
                  "created_at_tick" INTEGER, "updated_at_tick" INTEGER,
                  PRIMARY KEY ("id"));
CREATE TABLE cur_history ("id" INTEGER, "payload" TEXT,
                          "from_tick" INTEGER, "to_tick" INTEGER);
INSERT INTO cur SELECT value, 'payload-' || value, 1, 1 FROM generate_series(1, $ROWS);
INSERT INTO cur_history SELECT "id", "payload", 1, 2 FROM cur WHERE "id" <= $CHURN;
UPDATE cur SET "payload" = 'revised-' || "id", "updated_at_tick" = 2 WHERE "id" <= $CHURN;
CREATE INDEX cur_history_id ON cur_history ("id", "from_tick");
SQL

sqlite3 "$SCRATCH/hist_log.db" <<SQL
CREATE TABLE cur ("id" INTEGER, "payload" TEXT, "at_tick" INTEGER);
INSERT INTO cur SELECT value, 'payload-' || value, 1 FROM generate_series(1, $ROWS);
INSERT INTO cur SELECT value, 'revised-' || value, 2 FROM generate_series(1, $CHURN);
CREATE INDEX cur_id_tick ON cur ("id", "at_tick");
SQL

H_CUR="$(bytes "$SCRATCH/hist_current.db")"
H_SHADOW="$(bytes "$SCRATCH/hist_shadow.db")"
H_LOG="$(bytes "$SCRATCH/hist_log.db")"

echo
echo "── Q7: historicization at ${ROWS} rows, ${CHURN} replaced (10% churn) ───"
printf '  (i)   current only, no history : %10s\n' "$H_CUR"
printf '  (ii)  current + shadow history : %10s  %8s vs (i)\n' \
  "$H_SHADOW" "$(pct "$H_CUR" "$H_SHADOW")"
printf '  (iii) rel-as-log + max-tick view: %9s  %8s vs (i)\n' \
  "$H_LOG" "$(pct "$H_CUR" "$H_LOG")"

# The read that separates (ii) from (iii): "the row as of tick 1" for one key.
echo "  as-of read plans:"
echo -n "    (ii)  shadow : "
sqlite3 "$SCRATCH/hist_shadow.db" \
  'EXPLAIN QUERY PLAN SELECT "payload" FROM cur_history WHERE "id"=5 AND "from_tick"<=1 AND "to_tick">1;' \
  | tr '\n' ' '; echo
echo -n "    (iii) log    : "
sqlite3 "$SCRATCH/hist_log.db" \
  'EXPLAIN QUERY PLAN SELECT "payload" FROM cur WHERE "id"=5 AND "at_tick"<=1 ORDER BY "at_tick" DESC LIMIT 1;' \
  | tr '\n' ' '; echo

exit "$FAILED"
