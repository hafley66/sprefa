# feature/seeded-pre CONTINUATION: fix the oracle red set, land pre/2

## Where you are
Your branch starts at WIP commit f1ed1f82 (banked from the previous attempt).
The pre/2 surface is DONE and verified: parse_dl.pl spelling, print_dl
round-trip, 4 conformance/TEXT_DOOR fixtures compiled byte-identical on both
doors, rulings.pl row present. Read the diff first:
`git log -1 -p f1ed1f82` (142 insertions across 11 files).

## The defect you own
The oracle edits in that WIP broke tests that are GREEN on base d60d9990:
- conformance, 5 failures: finalize_over_log_fires_on_retention_prune,
  keyed_replace_departs_the_old_row,
  pairwise_reads_state_at_the_departure_tick,
  pairwise_pairs_adjacent_values_when_the_source_idles,
  concat_program_queue.
- plunit, 14 failures: catalog_plane_rail:level_plane_family_corpus_counts,
  rel_rule_observers:finalize_departure_frontier, departure-frontier
  interning checks, stored-snapshot checks (run `just plunit` for the full
  list).
None of these mention pre. The WIP's occurrence-path edits (body.pl,
merge_family.pl fixtures, level_eval.pl, lower.pl, analyze.pl) changed
behavior beyond the no-prior-row case. Diagnose by reverting your oracle
hunks one at a time against the 5 conformance names; the correct fix touches
ONLY the path where pre/2's seed binds, and rulings.pl:70-93 (R6/R1) fixes
what pre/1 must keep doing untouched.

## Non-negotiable rails
- NEVER run git merge / pull / rebase in this worktree. Base integration is
  the coordinator's job.
- Blocked -> write FAILURE-REPORT-SEEDED-PRE.md (NOT FAILURE-REPORT.md, that
  file belongs to another arc) with exact command + output, and exit
  NONZERO. Exiting 0 with uncommitted work or red gates is a defect.
- NEVER --no-verify. If the pre-commit comment rail reports a missing
  extractor binary, build it:
  `cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract`

## Remaining deliverables after the red set is green
1. Emitter/tsv2: seed lowers as COALESCE/default on the prior-row read,
   inlined, never a second statement (check whether the WIP's lower.pl hunk
   already does this correctly; verify against an emitted fixture).
2. Clock check: state in your final commit message which ring the seed read
   lands in (3_clock_check.pl reads the expanded program; the WIP touched
   it, verify the edit is needed at all).

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate (ALL must pass; known-red exceptions listed)
```bash
cd <worktree>/v6 && just conformance && just plunit
cd <worktree>/v6 && just text-door ; just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
conformance and plunit must be FULLY green. text-door and roundtrip have a
known-red family on main (rel_element_list_round_trips,
nested_rel_element_list_round_trips, list_interned_set_relation_element_refused,
column_type_unknown(fighter_summary)) owned by another lane; your gate
passes when the ONLY failures are that family and all pre/2 fixtures pass.

## Commit rail
Up to 3 commits, prefix `prolog:`. Comment budget: max 2 consecutive
comment lines in any touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
dl variable names descriptive, never single-letter.
