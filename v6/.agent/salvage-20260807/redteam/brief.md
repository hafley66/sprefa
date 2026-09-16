# Lane: BREAK the interning contract (adversarial review, pass 1 of 2)

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/redteam && git rev-parse HEAD`
— must be e8bb9911. Anything else: STOP and report.

You are pass 1 of 2: the coordinator verifies every claimed break, so a false
break costs you nothing and a found break is the whole point. READ-ONLY on all
tracked files; your only writes are REPORT-BREAK.md at the worktree root and, if
useful, throwaway .sql/.mjs probe scripts under scratch/ (create the dir). You
may run sqlite3 and node against throwaway db files in scratch/ to prove a
break concretely. Never run the sweep, never edit source, no commits except the
final one below, no subagents.

## The target
plans/2026-08-08-interning-contract.md and its amendment sections (the gun §15,
telemetry §16), plus plans/2026-08-08-interning-contract.visual.human.unga.md.
Your job: find inputs, schedules, or environments where the contract's claims
are wrong, its enumerations incomplete, or its designs self-contradictory.
A break = a concrete scenario with the doc section it defeats, reasoning or a
runnable probe, and severity (correctness / silent-wrong-answer / perf / ops).
Silent wrong answers outrank crashes.

## Where to swing (work this list top to bottom, then freestyle)

1. **Mixed encoding join, one db.** A rel column under `direct(col)` waiver
   joined against an interned column of another rel: id compared to text is
   silently empty. Find whether the contract refuses it, and the exact rule
   text; if it relies on a checker, name the program shape that slips past.
2. **The one text-order rule** ("text-demanding expressions read __str,
   identity-demanding read the id"). Attack its enumeration: ORDER BY inside
   json_group_array/group_concat over a column that mixes types, eq_lit against
   a text literal, DISTINCT-like dedup semantics, boundary read ordering, the
   claim that `<`/`>`/min/max already refuse text (verify in
   v6/prolog/registry.pl and lower.pl:3811-3816 — is the refusal total, or is
   there a reachable text comparison?).
3. **`INSERT OR IGNORE ... RETURNING`** as the interned-rows source. Verify on
   BOTH sqlite builds this repo runs (CLI 3.43.2 and @libsql 3.45.1 — the jsonb
   precedent says builds diverge): does RETURNING yield exactly the inserted
   rows under OR IGNORE, including the empty case and duplicate keys within
   one statement?
4. **Crash between intern and swap.** The two-statement ingest invariant under
   SIGKILL after statement 1 commits: what state greets the restart, and does
   the contract's recovery story cover it? Also the running totals in
   __str_stats after a kill mid-tick (the doc claims I-G-R owns this; check
   the design survives on paper).
5. **keep(count(4096)) vs running totals.** rows/content_bytes = previous row +
   delta, read via ORDER BY rowid DESC LIMIT 1. After the keep-trim deletes old
   rows, after 4096 wraps, after a tick with zero arrivals writes no row: does
   the chain ever read a wrong previous value?
6. **The gun's byte-identity gate.** `--intern=direct` corpus compile must be
   byte-identical to base. Is that actually achievable given the contract also
   changes recursive-head shapes (rowid+unique) for fixpointIr carriers, or do
   the two features contaminate the gate?
7. **The migration-out one-statement route** (`CREATE TABLE ... AS SELECT *
   FROM __txt_...`): column types after CTAS (affinity loss), PK/uniqueness
   loss, and whether the doc says so.
8. **TEMP VIEW `__txt_*` per connection**: multiple connections (serve + cli),
   a reader that connects without running DDL, view/name collisions with user
   rels that start with __txt_.
9. **NULL and empty-string keys.** UNIQUE treats NULLs distinct; '' is a valid
   text. Walk both through intern, swap, view render, and the stats bytes.
10. **dbstat availability** on both builds for the serve-boundary true-bytes
    read.

## Deliverable and commit
REPORT-BREAK.md: numbered findings, most severe first, each = {doc section
attacked, concrete scenario or probe transcript, why it breaks, severity};
then a "did not break" list for the attacks that held, one line each — a held
attack is a receipt, and pass 2 checks you actually swung. Commit ONLY
REPORT-BREAK.md + scratch/ on lab/interning-redteam, message
"lab: interning contract red-team, pass 1". Never push.
