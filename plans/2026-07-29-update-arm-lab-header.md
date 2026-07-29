# UPDATE-ARM LAB (planner contract; user deferred 2026-07-29 "vibe trust")

Closes the match-frontier lab's open UPDATE-ARM slot (and grades
SUGAR-SCOPE alongside). The hypothesis: the OLD/NEW update arm is
ALREADY expressible with zero constructs as

    changed(Key, Old, New) <- finalize(r(Key, Old)), r(Key, New).

because on a keyed replace, finalize fires with the departing row in
the same tick the current table holds the new row. If the hypothesis
holds across all cases, the inserted/deleted/OLD/NEW construct
question from the match lab's syntax ranking DISSOLVES; if it breaks,
name exactly where and what the smallest honest construct would be.

Lab home: v6/prolog/labs/update_arm/ + ONE verdict doc
plans/2026-07-29-update-arm-verdict.md. TOUCH NOTHING ELSE (concurrent
lanes own compile/* and labs/rel_spreading/). Labs die on landing.

## Cases (each = an executable check against the REAL oracle engine,
## engine.pl via the conformance harness pattern; PASS-only stdout)

U1 keyed replace: one row replaced under key -> the arm yields exactly
   one (Key, Old, New) row; tick placement recorded (same tick as the
   replace or +1 -- state which, from the oracle's own log).
U2 plain insert (no prior row): no finalize occurrence -> arm silent.
U3 plain delete (retraction with no successor): current side empty ->
   arm silent. If retraction cannot reach the rel kind chosen, use an
   edge-headed keyed rel where departure is real (the
   keyed_replace_departs_the_old_row fixture is the precedent).
U4 same-tick double replace (v1 -> v2 -> v3 in one tick): the ruled
   collapse says intermediates are invisible. Grade what the arm
   sees: the honest (v1, v3) pair, a phantom (v1, v2) or (v2, v3), or
   two rows. Whatever it is, record it as the defined semantics with
   the collapse-trace citation.
U5 log rels: finalize arms over log rels are statically dead
   (match-frontier finding). Grade: refusal or silent-dead today,
   name which, and whether the lifetime-checker-to-be should flag it.
U6 finalize INSIDE a match arm body: does `match r(K, V) ( changed(K)
   <- finalize(old_r(K, _)) ; ... )` compose through
   expand_match_program? One check, refusal or pass named.
U7 compiled-path status: does the finalize/departure word compile in
   tsv2 today or sit in an unsupported sweep bucket? DOCUMENT ONLY
   (run the sweep read-only, cite the bucket); fixing the bucket is
   not this lab.

## SUGAR-SCOPE rider

The match-frontier lab's other open slot: what scope does arm sugar
share (the trigger atom's bindings only, or body atoms too). Grade it
with two checks on the LANDED match block: an arm reading a trigger
column vs an arm trying to read a sibling arm's binding (must fail,
show the failure shape).

## Grades

Lab suite: swipl -q -l labs/update_arm/lab.pl -g go -g halt, exit 0,
PASS-only stdout, twice. Every graded spelling carries its rx
lowering in the verdict (U1's is groupBy(key) + pairwise, say so
concretely). No-drift: conformance go.pl (126 PASS) and roundtrip.sh
untouched and green. Fixture/5 candidates distilled for any case
worth promoting to conformance.

## Laws

Worktree agent: FIRST ACTION `git merge --ff-only <base sha stated at
dispatch>`; on failure or missing v6/, STOP AND REPORT. Commit per
logical step with git commit -n; do NOT merge. Descriptive
identifiers; no em dashes; banned words provenance, substrate,
load-bearing, regime. Verdict: per-case table, the
hypothesis-holds/breaks verdict line up top, named slots, all grades.
