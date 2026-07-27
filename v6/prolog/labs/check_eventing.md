# check_eventing: the LSP-diagnostics / agent-turn-hook loop under level vs edge

Lab: `v6/prolog/labs/check_eventing.pl` (435 lines, 17 checks, all PASS).
Run: `swipl -q -l v6/prolog/labs/check_eventing.pl -g go -g halt` (exit 0).

## Verdict

The level/edge distinction carries the whole loop, and it is the only new idea
needed. Everything v5 does with `diag(path, line, severity, code, msg)` plus
`diag_stage(code, "agent-turn")` plus a hand-rolled ratchet table falls out of
three constructs already in LANG.md: a level rule for the diagnostic, an edge
rule for the history, a keyed rel for the ratchet. The hook question v5 cannot
answer at all ("what did this turn break, as opposed to what is broken") is a
two-line join of the level view against the edge history.

Two things are missing rather than ambiguous:

1. **A phantom tick read that is not a rel.** An edge rule needs the arrival
   tick in its head. If the tick is read from a clock REL, the clock row's own
   arrival re-fires the rule every tick, because LANG.md says an edge rule
   fires on ANY body atom's arrival. Graded receipt below: 5 history rows the
   right way, 13 the wrong way, same program, same run.
2. **Edge on departure.** `<+` fires on arrivals only, so
   `diag_closed_at(path, line, code, tick)` cannot be written. Time-to-fix and
   "was this lint red at commit time" are not recordable, only recomputable,
   and the recomputation retracts when the lint re-opens.

Everything else in this lab is spec ambiguity, numbered at the bottom.

## What the reference interpreter implements

`state(Base, Level, Edge)`, one tick per input batch.

| step | rule |
|---|---|
| apply inputs | `add`/`del`/`replace(Pattern, Rows)`; a keyed rel drops the row sharing its key prefix, which is the `-old/+new` |
| settle | level rules recomputed to a least fixpoint from the ground set; edge rules fire on this tick's arrivals; loop until the edge set stops growing, so an edge row is visible to level rules in the SAME tick |
| diff | `Plus` and `Minus` are computed against the TICK BOUNDARY, not against intermediate states |

The last row is the whole retraction story. Level rows are re-derived and
diffed; edge rows are unioned and never appear in `Minus` (graded:
`edge_never_retracts` quantifies over every tick's `Minus`).

Aggregates (`count(...)` in a head position) and negation (`\+`) are evaluated
against the settled round, which is why the fixpoint recomputes from scratch
each round instead of accumulating.

## (a) File rows: replaced wholesale per file, not keyed by (path, line_no)

Decision: `replace(file_line(Path, _, _), NewRows)`. Justification: the
extractor re-reads a file and cannot know which old row each new row
corresponds to. Keying on `(path, line_no)` leaves stale rows behind whenever a
file shrinks, because a key replace only fires for keys that reappear. Wholesale
replace plus set-diff is the only shape where a deleted trailing line produces a
`-file_line` at all.

The cost, and it is real: line numbers are part of row identity, so an insert at
the top of a file retracts and re-adds every row below it. Diagnostics on
untouched code then retract and re-appear, and the edge history gains a bogus
episode for code nobody edited. See ambiguity 2.

## Graded receipts

Full trace from the lab's 7-tick scenario over one file. `+` is an arrival,
`-` is a retraction. Rows shown are the ones the grader asserts.

**T1** two banned calls, ratchet grandfathered at 2:
```
+file_line(a.rs,3,...)  +file_line(a.rs,5,...)  +ratchet(eprintln_ban,2)
+diagnostic(a.rs,3,eprintln_ban,warning)  +diagnostic(a.rs,5,eprintln_ban,warning)
+lint_count(eprintln_ban,2)
+diag_history(a.rs,3,eprintln_ban,1)  +diag_history(a.rs,5,eprintln_ban,1)
```
no `violation`: 2 > 2 is false. (`level_diag_appears`, `ratchet_holds_at_baseline`)

**T2** a third banned call arrives:
```
-lint_count(eprintln_ban,2)  +lint_count(eprintln_ban,3)
+diagnostic(a.rs,7,eprintln_ban,warning)  +violation(eprintln_ban,3,2)
+diag_history(a.rs,7,eprintln_ban,2)
```
(`ratchet_fires_on_growth`, `count_row_replaces_by_key`)

**T3** lines 3 and 7 fixed, line 5 untouched. THE HEADLINE DELTA:
```
-diagnostic(a.rs,3,eprintln_ban,warning)  -diagnostic(a.rs,7,eprintln_ban,warning)
-violation(eprintln_ban,3,2)  -lint_count(eprintln_ban,3)  +lint_count(eprintln_ban,1)
```
`diagnostic(a.rs,5,...)` appears in NEITHER list, and neither does
`file_line(a.rs,5,...)`, across a wholesale file replace. `diag_history` is
untouched: the two rows from T1 and the row from T2 all survive the fix.
(`level_diag_retracts_on_fix`, `wholesale_replace_no_flicker`,
`ratchet_violation_retracts`, `edge_history_survives_fix`)

**T4** the ratchet tightens, keyed replace, no code change:
```
-ratchet(eprintln_ban,2)  +ratchet(eprintln_ban,1)
```
one row survives in the rel, no violation on the way past (count is 1).
(`ratchet_tightens_by_key`)

**T5** line 3 regresses, an unwrap lands:
```
+diagnostic(a.rs,3,eprintln_ban,warning)  +diagnostic(a.rs,9,unwrap_ban,warning)
+violation(eprintln_ban,2,1)          <- fires against the TIGHTENED ratchet
+unratcheted_lint(unwrap_ban,1)       <- v5's "no baseline row" diag rule
+diag_history(a.rs,3,eprintln_ban,5)  <- SECOND episode; the tick-1 row stays
```
(`tightened_ratchet_catches_regrowth`, `unratcheted_lint_fires`,
`edge_history_reopens`)

**T6** the agent-turn hook writes its window row and gets its answer in the same
tick:
```
+hook_window(turn_7,4)
+turn_diag(turn_7,a.rs,3,eprintln_ban,5)  +turn_diag(turn_7,a.rs,9,unwrap_ban,5)
```
`diagnostic(a.rs,5,...)` has been open since tick 1 and is correctly absent.
(`hook_window_join`, `hook_window_excludes_old`)

**T7** everything fixed:
```
-diagnostic x3  -violation(eprintln_ban,2,1)  -unratcheted_lint(unwrap_ban,1)
-turn_diag(turn_7,a.rs,3,eprintln_ban,5)  -turn_diag(turn_7,a.rs,9,unwrap_ban,5)
```
`diag_history` still holds all 5 rows, including the two that were fixed inside
the turn. (`hook_window_excludes_fixed`)

**The clock hazard, measured.** Same program, two edge rules over the same
`diagnostic` view, differing only in how the tick is read:
```
diag_history(path,line,code,opened_at) <+ diagnostic(path,line,code,_), now(opened_at);
diag_seen(path,line,code,at)           <+ diagnostic(path,line,code,_), tick_rel(at);
```
after 7 ticks: `diag_history` = 5 rows (one per appearance),
`diag_seen` = 13 rows (one per live diagnostic per tick, the sum of
2+3+1+1+3+3+0). (`clock_rel_join_storms`)

## (3) The hook, and what the CLI actually subscribes to

The hook is not a new mechanism. It is two asks and one keyed demand row.

```
rel hook_window(turn: Key(TurnId), since: Tick);

turn_diag(turn, path, line, code, opened_at) <-
    hook_window(turn, since),
    diagnostic(path, line, code, _),
    diag_history(path, line, code, opened_at),
    opened_at > since;
```

| consumer | ask | mode (cardinality, lifetime) | why |
|---|---|---|---|
| agent-turn hook, turn start | write `hook_window(turn_id, now)` | n/a, one keyed row | key replace means a turn never leaks its window into the next turn |
| agent-turn hook, turn end | snapshot `? turn_diag(turn_id, path, line, code, opened_at)` | (multi, finite) | turn bound, location free; a snapshot is a SELECT so it always completes, and the hook exits with a status from the row count instead of blocking the agent |
| LSP client | tail `? diagnostic(path, line, code, severity)` scoped to open documents | (multi, until(disconnect)) | the connection is the scope; `switch_map` on the open-document set dominates it, so closing a document range-DELETEs its demand rows |
| commit gate | snapshot `? violation(code, count, allowed)` | (multi, finite) | exit 2 when non-empty |
| dashboard | tail `? diag_history(...)` | (multi, never) on its own | history has no completing upstream; the CLI can warn before blocking, per plans/2026-07-27-mode-dominance.md |

The `-row` on the LSP tail IS the publishDiagnostics clear. v5's LSP had to
re-run the program and diff two result sets to find out what stopped being
true; here the retraction is the wire message.

Retention: `diagnostic`, `violation`, `turn_diag` are bounded by the corpus.
`diag_history` and `diag_seen` grow without bound and are the rels that need
ARCH.pl's `retention_bound` fold. That split falls exactly on the level/edge
line, which is a second reason the distinction is worth having: it tells the
storage tier which tables need a prune policy.

## (4) Debounce, not graded

Editor keystrokes arrive faster than re-lint should run. The debounce is the
KEY, not an operator:

```
rel keystroke(path: Path, at: Tick);
rel lint_due(path: Key(Path), due_at: Tick);

lint_due(path, due_at) <- keystroke(path, at), due_at is at + 200;
lint_ready(path)       <- lint_due(path, due_at), clock(due_at);
```

Each keystroke derives a new `lint_due` row for the same key, which REPLACES
the pending one, which restarts the timer. That is trailing-edge debounce with
no scheduler and no cancellation: the superseded due row is simply gone, and
the clock join never sees it. This is ARCH.pl's
`technique(delay, due_row_plus_clock_join)` with a key on top, and it survives a
crash because the due row is on disk.

Leading-edge throttle is the same rule with `\+ lint_due(path, _)` guarding the
derivation. Coalescing across files is free: one due row per path, and the
clock join fires them in one tick.

Caveat, and it is ambiguity 11: `due_at is at + 200` is an arithmetic body
guard producing a head column, which LANG.md's surface does not have. Either
bodies get computed columns or the head gets expressions; the lab dodged this by
not grading debounce.

## Ambiguities found in LANG.md

1. **Wholesale replace and flicker.** When a file's rows are replaced
   wholesale, does a level diagnostic that holds both before and after flicker
   `-x/+x`, or hold steady? LANG.md says "Tick = one sqlite transaction: deltas
   in, rules join, writes, commit", which implies boundary diffing, but never
   says it. It must say it. If the extractor's DELETE and INSERT are two
   sub-steps with a rule pass between them, every diagnostic in an edited file
   blinks in the editor and gains a bogus edge-history episode on every
   keystroke. SHOULD: rows landing in one tick are one delta set, and level
   deltas are `post \ pre` / `pre \ post` at the tick boundary only. The lab
   implements this and grades it (`wholesale_replace_no_flicker`).
2. **Row identity versus line numbers.** Rule 1 does not save you when the rows
   genuinely differ. Inserting a line at the top of a file changes `line_no` on
   every row below it, so the level view legitimately retracts and re-derives
   diagnostics for untouched code, and the edge history records a new episode
   per shifted line. Options the spec does not choose between: key the
   diagnostic on `(path, code, text_hash)` and carry `line` as a plain column;
   or have extraction mint a stable per-node id. The lab uses fixed line numbers
   and therefore does NOT test this, which is stated here rather than hidden.
3. **Is a clock a rel or a phantom column?** LANG.md says both: "global tick T
   (phantom column; observed via clock rels)". If a clock rel is an ordinary
   rel, its row arrival is an arrival, and every edge rule joining it re-fires
   every tick. Measured above: 5 rows versus 13. Either `now()` is a body item
   that is read-only and never arrival-eligible (the lab's choice), or clock
   rels carry a non-triggering marker. This has to be decided before edge rules
   ship, because the wrong default is silent and only shows up as a growing
   table.
4. **No edge on departure.** `<+` fires on arrivals. There is no way to write
   `diag_closed_at(path, line, code, tick)`. The level workaround (a history row
   whose diagnostic no longer holds) retracts when the lint re-opens, so it
   cannot serve time-to-fix or "red at commit time" telemetry. Needs a decision:
   a departure form in an edge body, or an explicit statement that departures
   are not observable and history is append-only-on-arrival.
5. **Keys on edge rels.** `diag_history` holds two rows for the same location
   after a re-open, so the hook join fans out per episode. A latest-episode view
   wants `rel diag_open(path: Key(Path,1), line: Key(Int,2), code: Key(Code,3),
   opened_at: Tick)`, but a key replace emits `-old`, and edge rows never
   retract. Keyed edge rels are a contradiction under the current wording, and
   the spec does not say so.
6. **Aggregate heads are implicitly keyed.** `lint_count(code, count(...))`
   holds one row per code and the T2 delta is exactly a key replace
   (`-lint_count(...,2)/+lint_count(...,3)`). Does a `count()` head imply
   `Key` on the non-aggregate columns? If yes, say so and the checker gets it
   for free. If no, two count rows for one code can coexist in a bad fixpoint
   round and every downstream comparison is wrong.
7. **What does `count()` range over?** v5 writes `eprintln_count(source_file,
   count(line_number))`, which counts DISTINCT line numbers within the group.
   The lab had to write `count(hit(path, line))` to count locations across
   files. The surface needs to state that the count is over distinct bindings of
   the counted term, projected on the head's other columns as the group.
8. **One `diag` rel or one rel per lint?** 58 `.dl` files in this repo mention
   `severity` and 56 head a `diag(...)` (grep over `--include=*.dl`). v5 funnels
   every lint into one wide rel with stringly `severity` and `code` columns plus
   a `diag_stage(code, "agent-turn")` routing table. In the candidate, severity
   is an enum column and stage routing is not data at all: it is which ask a
   consumer runs. LANG.md does not say whether the universal-`diag`-rel shape is
   still wanted, and the answer changes what the checker can prove (a per-lint
   rel gets per-lint column types; the universal rel gets a `msg` string nobody
   can check).
9. **Demand rows in edge bodies replay the backlog.** `hook_window` in a LEVEL
   body gives correct catch-up: the window row arrives at T6 and immediately
   sees the T5 diagnostics. The same atom in an EDGE body fires the rule against
   the whole current set on the subscriber's arrival, which is LANG.md's known
   "late-subscriber backlog replay". Both behaviors are wanted somewhere. The
   spec should name the rule: ask surfaces are level rules; an edge rule joining
   a subscriber or demand rel is opting into replay.
10. **Who writes the ratchet?** In v5 the baseline rows are literal lines in the
    `.dl` file and tightening is a human edit. Here `ratchet` is a keyed rel; if
    a tool writes it, program facts and world-filled rows share a head. ARCH.pl
    says mixed heads are sound under count-IVM, so this is probably fine, but
    the keyed-rel conflict law ("jointly semidet per key per tick") has to cover
    a program fact and a world row colliding on one key, and it currently reads
    as a rules-only law.
11. **No arithmetic in heads or bodies.** `due_at is at + 200` (debounce) and
    any `line + 1` style join have no surface form. Body guards exist for
    comparison in the v5 corpus; computed columns do not appear in LANG.md at
    all.

## Deviations from the spec

- The lab uses `now(Tick)` as a body item to read the phantom tick column.
  LANG.md does not have it. Ambiguity 3 is the argument for adding it; the
  `diag_seen` twin is the measurement of the alternative.
- Types are not checked. Column types are in comments and severity is an atom
  (`warning`), not an enum-checked value; this lab grades delta semantics, and
  `enum_match.pl` already owns envelope typing.
- Edge rows are made visible to level rules within the same tick (the settle
  loop). LANG.md says a body is one time cut but does not say whether an edge
  row appended this tick is inside that cut. The hook needs it to be, otherwise
  `turn_diag` lags one tick behind the diagnostic that caused it, and a hook that
  fires immediately after an edit gets an empty answer.

## Tier-order implication

- **Boundary delta emission is a prerequisite, not a feature.** Ambiguity 1 is a
  property of the store's delta output, not of the surface. `count_ivm_port`
  (ARCH task, unbuilt) has to land with "one tick, one delta set, diffed at the
  boundary" as its contract before any LSP or hook surface is written, or the
  editor blinks and no surface change can fix it.
- **`now()` is kernel, not sugar.** It is the one body item that is not
  arrival-eligible, so it cannot be desugared into a rel join. It has to enter
  the kernel facts alongside `rule(level|edge)` before `surface_dcg` freezes the
  body grammar.
- **mode_lab is enough for the ask side.** The four consumers in the table above
  are typed by (cardinality, lifetime) with no new machinery: snapshot asks are
  finite by construction, the LSP tail is dominated by the connection scope, and
  the history tail is the one the CLI should warn about. `mode_lab` stays
  unblocked by this lab and can grade these five asks as extra rows.
- **Decide ambiguity 4 before the arrow set freezes.** Adding departure
  observation later means either a third arrow or a new body form, both of which
  are grammar changes. Deciding it now costs one line in LANG.md.
- **Retention follows the level/edge line exactly.** `retention_bound` only ever
  needs to run over edge-headed rels. That makes the analysis a one-pass fold
  over rule kinds instead of a data-size heuristic.
