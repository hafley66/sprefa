# Interning contract red-team (flash pass 1 + coordinator verification pass 2, 2026-08-08)

Coordinator verdicts: F1 CONFIRMED (literal present in emitted corpus, construct absent from contract enumeration); F2 CONFIRMED (independent fresh-db probe: OR IGNORE+RETURNING reports rowsAffected=0 on @libsql 0.17.4, inserted rows only in .rows); F3/F4/F5/F6 CONFIRMED at design level (doc-internal logic verified against quoted sections; CTAS constraint loss is standard SQLite behavior; the batch split in F6 follows from ISqlRunner.batch mechanics); F7 CONFIRMED (parse_dl.pl's only name refusal is tagged_brace_reserved). All 7 go back to the planner. Process note: the flash lane died once mid-run (resumed via its opencode session) and did not commit its report despite the brief; artifacts collected by the coordinator.

# REPORT-BREAK.md — interning contract, adversarial review, pass 1

Head: `e8bb9911` (verified). Target docs:
`plans/2026-08-08-interning-contract.md` (+ amendments §15 gun, §16 telemetry, §17
lanes) and `plans/2026-08-08-interning-contract.visual.human.unga.md`.

All SQL probes in this report were run against both builds this repo uses: the
CLI `sqlite3` (3.43.2) and `@libsql/client` 0.17.4 (bundled SQLite 3.45.1), on
throwaway dbs under `scratch/`. Source reads are at base `f650f2b7` content (the
emitted corpus in `v6/prolog/compile/out/*.ts` is the pre-task-#4 baseline the
contract itself pins).

---

## Findings, most severe first

### 1. Text-literal equality (`lit`/`eq_lit`) is absent from §5.2's enumeration and outside §5.3's five call sites — silent empty/wrong, and the migration as specified cannot pass its own gate

- **Doc attacked:** §1 ("every text comparison the language admits is identity,
  which survives interning"), §5.2 (the 10-row enumeration), §5.3 (the five
  `text_operand_sql/4` call sites plus the "one predicate carries it" claim).
- **The shape:** a body pattern that compares a text column to a text constant
  compiles to `lit(ColumnExpr, Arg)` → `where_text` (`lower.pl:338`) →
  `"col" = 'literal'`, and on the fixpoint path to `eq_lit(IrLeft, Literal)`
  (`lower.pl:3170-3180`). `text_operand_sql/4`'s enumerated callers
  (`compile_regexp_goal/3`, `text_scalar_rendering/3`, the concat arm, two ORDER
  BY renderings, §5.3) do not include it. §5.3's rule classifies `==` as
  "identity-demanding → reads the id," but a literal operand is not in the id
  space. Nothing decodes it, nothing interns the literal.
- **Corpus-patient-zero:** `v6/prolog/compile/dl_view/backslash_in_string_literal_survives_both_doors.dl6`
  is `hit(Value) <- raw(Value), Value == 'digit \\d here'.` Emitted at base:
  `b0."text_value" = 'digit \\d here'` (in `?...where_text`, and in addSql /
  insertSql / frontRow, corpus file
  `out/backslash_in_string_literal_survives_both_doors.ts:189-250`). Other
  corpus constants: `"kind" = 'rust'`, `"name" = 'acme'`, `"partition" = 'ping'`,
  `"text_value" = ...`.
- **Why it breaks:** after interning, `"text_value"`/`"kind"` become INTEGER ids
  while the literal stays TEXT. SQLite applies the column's INTEGER affinity to
  the literal: a non-numeric literal (`'digit \d here'`) stays text and `id =
  text` is false → the filter silently matches nothing; a numeric-looking
  literal (`= '42'`) coerces to the integer `42` → silently matches whichever
  row happens to carry word-id 42. Both are silent wrong answers.
- **Consequence beyond the wrong answer:** this module is in the 211-module
  corpus, so gate G5 / §10.2 receipt 1 (`wrong=0`) cannot go green while §5.3's
  fix set is limited to the five named sites. The contract claims a complete
  enumeration ("The full enumeration follows") and the fix is "one predicate
  carries it" with five callers; the six-by-six fix is reached only by a sixth
  site (literal interning or a decode) the plan never names.
- **Severity:** silent-wrong-answer (and, as specified, deterministic gate
  failure of G5).

### 2. §16.2's stated source for `interned` (`rowsAffected`) reads 0 under `INSERT OR IGNORE ... RETURNING` — internally contradicts §16.3 and zeroes the telemetry

- **Doc attacked:** §16.2 (table row "interned ... which is the INSERT OR
  IGNORE's rowsAffected", receipt `types.ts:65-67`), §16.3 (statement shown with
  RETURNING; "so `interned` is the row count and the new-bytes sum is a fold
  over that same result").
- **Probe transcript** (`scratch/probe_ret.mjs`, `pa.mjs`, @libsql 3.45.1):

  ```
  plain INSERT OR IGNORE (3 distinct new): rowsAffected = 3
  plain INSERT OR IGNORE (1 new):           rowsAffected = 1
  plain INSERT OR IGNORE (all-dup):         rowsAffected = 0
  same INSERT OR IGNORE + RETURNING (1 new):rowsAffected = 0, rows = [{"content":"x"}]
  ```

  With `RETURNING` present, the driver reports `rowsAffected` as 0 even for rows
  actually inserted; the inserted rows are available only through the result
  set (`.rows`). The runtime path is @libsql (`sqlite.ts`/`ISqlRunner` via
  `@libsql/client`, confirmed install 0.17.4).
- **Why it breaks:** §16.2 tells the implementer to read `interned` from the
  statement's `rowsAffected` and asserts that equals the RETURNING row count.
  They do not: `rowsAffected` is 0, so `interned` becomes 0 every tick. The
  `rows` running total (`previous rows + interned`) stops growing, `dict_hit_pct`
  and `dict_converged` (both divide on `interned`, §16.5) compute wrong, and the
  §16 feature reports a dictionary that never learns. §16.3's alternative source
  (fold over the RETURNING result) is correct — this is an internal self-
  contradiction between the two sections on a shared number.
- **Severity:** silent-wrong-answer (telemetry only, but telemetry is the whole
  deliverable of §16/§17 lanes I-F, I-G).

### 3. `direct(col)` joined against an interned column is documented as a do-not but never refused; §5.3's own rule produces the silent-empty join

- **Doc attacked:** §9.2 (refusal list: only `direct_column_unknown`,
  `direct_column_not_text`), §9.3 (row 2 is a prose do-not enforced only by "the
  comment budget"; "this is a constraint the code cannot show"), §5.3 rule,
  §3.3 one-global argument (which covers two dictionaries, not direct-vs-id).
- **The shape:** rel `a(p: text) direct(p)` joined to `b(p: text)` (interned)
  on the same key. The join equality lowers to `a."p" = b."p"` (lower.pl:3171
  eq / §3.3 `reach(P) <- a(P), b(P)`). `a.p` is TEXT, `b.p` is an INTEGER id
  because §5.3's "identity-demanding → reads the id" hands `b.p` over raw.
  `'foo.ts' = 7` is silently empty.
- **Why it breaks:** there is no mechanical refusal; the only guard is a human
  review comment demanded by §9.3. The program shape that slips past every
  checker: one `direct(col)` column appearing in any join equality with another
  rel's interned text column. §10.3's "cross-rel text join" fixture asserts a
  NONEMPTY result but only for two interned rels in the one global dict; it
  never exercises the direct-vs-interned half.
- **Severity:** silent-wrong-answer.

### 4. The gun's §15.4 byte-identity gate targets base `f650f2b7`, but §7 flips recursive-head DDL independently of interning — the two features contaminate the gate

- **Doc attacked:** §15.4 ("byte-identical to base `f650f2b7`", "runs on every
  commit"), §10.2 receipt 3 (four allowed changed-line classes), against §7
  ("Interning and head-shape are two independent switches"; recursive heads with
  a non-null `fixpointIr` → rowid+unique, lane I-E).
- **Why it breaks:** at base, recursive heads emit `WITHOUT ROWID` — confirmed
  in the pinned corpus: `out/flagship_flow_reach_over_resolved_edges.ts:139`
  emits `CREATE TABLE "flow_reach" (...) WITHOUT ROWID` with a TEXT PK, and
  `flow_edge`/`resolved_call_edge`/`__support_next_*`/`__expand_*` same. §7's
  head flip (rowid+unique) changes those exact DDL lines for recursive heads,
  and it is explicitly independent of the intern mode. So once lane I-E lands,
  compiling with `intern(direct)` no longer reproduces base bytes at those
  heads, and §15.4's "byte-identical to f650f2b7" gate false-fails. Independently,
  §10.2 receipt 3's allowed four classes omit "a WITHOUT ROWID head became
  rowid+unique", so the classifier would flag a legitimate §7 line as a finding.
  The two scheduled changes (intern off / head flip) cross the gate's reference
  point on an axis the gate does not admit.
- **Severity:** ops (gate becomes non-passable / stale reference; audit signal
  corrupted).

### 5. §15.6 Route B ("one statement per relation") under-describes CTAS: affinity and PK/uniqueness are lost and the dump cannot reconnect to the reverted module

- **Doc attacked:** §15.6 Route B, "the one-statement dump": `CREATE TABLE
  "rel_x__plain" AS SELECT * FROM "__txt_rel_x"`, presented as "an un-intern is
  one statement per relation" and as "the escape hatch ... one statement per
  relation because the decoder was never optional."
- **Why it breaks:** `CREATE TABLE ... AS SELECT` produces a plain rowid table
  with no PRIMARY KEY, no UNIQUE, and no WITHOUT ROWID. The reverted
  `intern(direct)` module emits a typed, keyed, `WITHOUT ROWID` table of the
  same name; the CTAS dump is a different name (`rel_x__plain`), so it is not
  the shape the reverted program reads. Remounting requires drop-the-mount-old,
  create-the-typed-rel, INSERT-SELECT from the dump, then drop the dump — four
  statements and a drop/collision dance (the CREATEd table also collides with a
  still-mounted rel_x if not dropped). Route B is a data escape, not a
  reconnection, and the doc does not say CTAS drops the PK/UNIQUE/WITHOUT ROWID
  or that the "one statement" claim is only a dump. Affinity loss for the decoded
  columns is mild (they decode to TEXT), but the shape loss is real.
- **Severity:** ops (escape hatch under-specified; misleads into a half-migrated
  database).

### 6. Crash between intern and swap: §16.2's JS fold forces `__str` intern and `__str_stats` insert into separate transactions, so the running totals silently diverge under SIGKILL

- **Doc attacked:** §6.4 (intern-before-swap invariant), §16.2 (the "JS sum over
  the NEW words only" over the RETURNING result), §16.3 (running-total spelling
  reading "previous row ... ORDER BY rowid DESC LIMIT 1"), §16.7 / unga §15
  ("durable answer survives a kill", "the durable answer survives a kill").
- **The mechanism:** `ISqlRunner.batch` is one atomic transaction
  (`types.ts:65-67`, `1_incremental.ts` `seam.runner.batch`). But the stats
  INSERT's `content_bytes` delta is a JS fold over the RETURNING rows of the
  intern INSERT; that result is only available after the intern statement has
  run, so the stats INSERT cannot share the intern statement's batch. The intern
  commit and the stats commit are therefore separate transactions. If SIGKILL
  lands after the intern INSERT commits (`__str` grew by N new words, ids set)
  but before the stats row commits, the next tick's "previous row" read is the
  stale pre-N value, and `__str_stats.rows`/`content_bytes` stay permanently
  short by N with no reconciliation (the delta chain has no recovery step).
- **Why it breaks the doc:** this is the exact scenario I-G-R is told to answer
  ("is the running total recoverable after a kill, or does it silently restart
  at zero") and the contract's answer is to "read the emitted SQL and the
  EXPLAIN" — the design never establishes atomicity between intern and stats. In
  the same corner, §16.4's "a missing row for tick N means the door did not run
  at tick N" mis-reads: the door did run and added words; the row is missing
  because of the kill, so the absence is the opposite of the stated meaning.
- **Severity:** silent-wrong-answer (telemetry) + ops (crash-recovery claim
  unmet).

### 7. `__txt_*` / `__str_stats` reserved names are asserted, not enforced; a matching user rel collides with the emitted view or catalog row

- **Doc attacked:** §4.1 (`text_view_name` in the "`__` reserved namespace"),
  §16.1 (a second `catalog_ddl_contract/2` row for `__str_stats`).
- **The shape:** the lexer allows leading-underscore identifiers
  (`parse_dl.pl:414-423`); no refusal of `__`-prefixed rel names was found (the
  only name refusal in the parser is `tagged_brace_reserved`, `parse_dl.pl:1656`).
  A user rel literally named `__txt_<tablename>` collides with the emitted TEMP
  view of the rel of that table (a table and a view cannot share a schema name);
  a user rel named `__str_stats` collides with the catalog contract row that §16.1
  injects when the program "mentions" it — the mention that is supposed to
  materialize the catalog columns would instead conflict with the user's own
  column list.
- **Why it breaks:** the contract treats the namespace as reserved but gives no
  mechanical gate or fixture for it; the "same `__` reserved namespace as `__ref_`,
  `__new_`..." is a statement, not a checked invariant, and §10.3's new-fixture
  list adds no reserved-name test. A reader connection that never ran the
  module's DDL sees no TEMP view at all (by design, `TEMP`), so the decode is
  available only on a booted module's connection.
- **Severity:** ops (failure mode is a DDL error or catalog/column conflict, not
  a silent answer; low but un-enumerated).

---

## Did not break (held attacks, receipts)

- **Attack 3 — `INSERT OR IGNORE ... RETURNING` row set.** Both @libsql 3.45.1
  and CLI 3.43.2 return exactly the rows actually inserted, deduplicated even
  when a same-statement input row is new-then-ignored (`["z","z"]` → one row),
  empty when all-ignored, and empty for the empty input
  (`scratch/probe_ret.mjs`, `probe_ret2.mjs`, `c1.sql`). The RETURNING column
  count is the correct `interned` source (the `rowsAffected`-vs-rows.length
  mismatch is tracked as finding 2).
- **Attack 5 — `keep(count(4096))` running totals.** `rows`/`content_bytes`
  read the newest cumulative row via `ORDER BY rowid DESC LIMIT 1`; the newest
  row is never trimmed by keep-retention, a quiet tick just writes no row and the
  next covered tick reads the last cumulative value, so the chain never reads a
  stale previous value. Holds.
- **Attack 9 — NULL and empty-string keys.** Stored text columns are
  `TEXT NOT NULL` (`lower.pl:976`) and `__str.content TEXT NOT NULL UNIQUE`;
  NULL is refused at the door (`text_intern_null`, §6.3). `''` interns once
  (UNIQUE ignores the duplicate), length 0 grows `content_bytes` by 0, and
  lookup + view round-trip it correctly (`scratch/probe_empty.mjs`). Holds.
- **Attack 10 — dbstat availability.** The `dbstat` vtab is present and
  functional on both builds: CLI 3.43.2 returns rows, and @libsql 3.45.1 returns
  `[{name: t, pgsize: 4096}]` (`scratch/probe_dbstat.mjs`). The serve boundary's
  true-bytes read (§16.6, `serveStats.ts:82-102`) has both builds available.
  Holds.
- **Attack 2 (ordering-comparison part) — the refusal is total.** `<`/`=<`/`>`/`>=`
  carry TypeRule `both_number` (`registry.pl:240-243`) and `check_comparison_types`
  throws `comparison_operand_not_number` when either side is text
  (`lower.pl:866-873`); `min`/`max` filter through
  `compile_aggregate_number_operand/5` (`lower.pl:3811-3816`) which throws
  `aggregate_operand_not_number` on text. No reachable text `<`/`>`/min/max
  exists. The one reachable text comparison that the refusal misses is the
  literal-equality in finding 1, which is identity-shaped, not ordering-shaped.
- **Attack 4's atomicity premise.** If the intern/lookup were one batch and the
  rewrite never touches storage, a kill between statements 1 and 2 leaves only
  orphaned `__str` rows (append-only, already accounted in §13 row 4) and the
  rel tables correctly un-touched. The break is specifically the intern-vs-stats
  split in finding 6, not the intern-vs-lookup split.
