# Time-plane unification lab — header (planner-seeded contract)

User direction (2026-07-30, verbatim spine): "log rel is just rel with a time
column ... rels should have auto created_at_tick and updated_at_tick so to
speak, and we could historicalize them as well with a time column for easy
versioning." Question put to the lab: how important is the log/set distinction,
and what is the compiler blast radius of unifying it into something more
composable.

Standing constraints the lab does NOT relitigate:

- `locked(single_rel_type_system)`, `rel_default_policy = value_unkeyed`,
  `no_policy_suffix_words` (log is the only kind word today).
- Tick log = pure function of (program, schedule), byte-graded across doors.
  Wall-clock never enters identity; tick number is the time axis.
- Occurrences-cannot-un-happen (R7) is currently a THEOREM of the log kind;
  this lab is allowed to propose its restatement, not to silently drop it.
- The retention ruling: `retention_count_lowering = retracting_rule_over_log`.
- Struct-as-rows boundary-invisibility precedent (dictionaries are storage
  plane, frontier-TEMP class): the pattern for metadata columns that exist
  without entering the graded boundary.
- Lab protocol: run against the REAL compiler and runtime (both doors), in a
  worktree, die on landing. Opus only.

## The hypothesis, stated so it can lose

H1: `log` is expressible as `rel` plus a compiler-filled monotone time/seq
column excluded from row identity — i.e. `log keep(K)` becomes SUGAR the way
enum/match/spread/coalesce are, one expansion module, and the engine's log
branch dies.

H2: every rel can carry auto `created_at_tick` / `updated_at_tick` as a
METADATA PLANE (not identity), boundary-invisible unless read, at a priced
storage/write cost.

H3: historicization (as-of reads / versioning) = an opt-in shadow log derived
from H1+H2 machinery, priced in bytes on a measured corpus, never default.

## Questions the lab must grade (receipts, not prose)

- Q1 identity split: with a seq column EXCLUDED from identity, does a bare set
  rel keep zero-delta on identical re-arrival while a sugared log rel keeps
  duplicate stacking? Grade both doors, byte-diffed tick logs.
- Q2 the five cross-plane refusals (`log_on_level_headed_rel`,
  `keep_on_non_log_rel`, `log_without_retention`, finalize-over-log,
  `retract_from_log`): which dissolve, which restate, which must survive as
  refusals on the unified model? Each answer = a fixture.
- Q3 retention visibility: under unification, does keep(count(N)) prune emit
  an ordinary minus delta (fixing the retention-grading gap class), and what
  does that do to EVERY existing log fixture's expected tick log? Count the
  regrade (the json_ticklog regrade precedent: 244 artifacts, 12 changed).
- Q4 finalize-over-log (held stream card 4): does the natural spelling now
  fire on the prune's minus delta, and is that R7-compatible once R7 is
  restated as "the seq column is monotone and rows are never REPLACED"?
- Q5 blast radius, measured not estimated: the ~50 grep sites (engine 5,
  lower 15, emit_ts 14, parse/print/registry 11, program_check/clock ~5) —
  classify each as dies / moves-to-expansion / survives. Table with file:line.
- Q6 metadata plane cost: bytes and statements/tick for created_at/updated_at
  on the memory-soak program and one 100k-row corpus, vs today. updated_at on
  a keyed replace must not add a statement (it rides the existing write) —
  prove or refute.
- Q7 historicization: smallest honest shape (shadow log per rel? as-of read
  spelling?), priced in bytes at 100k rows / 10% churn. Compare against the
  existing keep(all) log-of-the-rel pattern the channel thread already named
  (keep-until min(consumed.ordinal), the Kafka low-watermark).
- Q8 the held stream card 1b: does `seq(name)` column-type sugar become THE
  unified mechanism (one expansion fills the ordinal, log = rel + seq +
  retention), or do 1b and unification stay separate constructs?
- Q9 grading contract: if the seq/tick columns are boundary-invisible, what
  exactly does a tick log line carry for a sugared log rel — unchanged bytes
  vs today, or a regrade? Byte-diff receipts required.

## Named slots (ambiguities the lab may hit; fill or hand back)

- slot_seq_scope: per-rel monotone counter vs global arrival counter vs
  (tick, within-tick index) pair. The two doors ALREADY disagree internally
  (engine.pl:356-358 global per tick vs lower.pl:2275 per-table rowid) and
  the stream lab left it ungraded because unobservable; unification may make
  it observable, at which point the pick is a cross-target contract.
- slot_updated_at_semantics: what counts as an update on an unkeyed set rel
  (re-arrival of an identical row is a zero-delta today — does it bump
  updated_at? bumping it breaks zero-delta; not bumping makes updated_at a
  lie). This slot is allowed to conclude updated_at exists ONLY on keyed rels.
- slot_history_read_spelling: as-of / version reads — body word, decl, or
  refused-for-now.
- slot_metadata_read_spelling: how a rule reads created_at_tick without the
  column entering the head's identity.

## Receipts required to land

- Both-doors fixture set, sweep both modes zero wrong, zero movement in prior
  buckets except regraded log fixtures (counted and listed).
- Count-test law: statements/tick flat receipts on the unified log vs today's
  log; EXPLAIN SEARCH-not-SCAN on any new read path.
- Memory-soak comparison run (the churn program is log+keyed already).
- The Q5 blast-radius table with every site classified.
- Verdict doc `plans/2026-07-30-time-plane-unification-verdict.md`; lab files
  die on landing per protocol.
