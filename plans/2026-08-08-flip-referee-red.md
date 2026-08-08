# Flip attempt 2026-08-08: referee said NO

One atom flipped (`compile.pl:153` `default_intern_mode(direct)` -> `dict`),
full sweep run EXECUTING at dict. Result: `RUN wrong=13, FINAL wrong=17`
against the direct oracle. Atom reverted the same hour; this doc is the
blocker list the next lane works from. Everything compiled green and
`unexplained=0` held — rev 3.1's "a green compile means nothing" clause
earned its keep on the first pull of the trigger.

## The 17, bucketed by failure family

| family | modules | symptom in the first diff |
|---|---|---|
| A. departure/pre reads render ids or miss | `departed_fires_next_tick_on_retraction`, `keyed_replace_departs_the_old_row`, `pairwise_reads_state_at_the_departure_tick`, `pairwise_pairs_adjacent_values_when_the_source_idles`, `finalize_over_log_fires_on_retention_prune` | `closed_at add [[null,3]]`, `replaced_value [[null,null]]`, pairwise reads wrong VALUE (9 vs oracle 14), a tick-3 delta empty where oracle has one — the departed/pre state-read path is not decode-aware, and NULL says the id lookup happens where no intern ever ran |
| B. ordered aggregates over decoded text (FINAL only) | `ordered_group_concat_value`, `ordered_group_concat_ordinal`, `ordered_mermaid_line_assembly`, `ordered_fragment_line_assembly` | RUN identical, FINAL wrong: `value_joined [["north","null,null,..."]]` — group_concat concatenates NULLs, so the final-snapshot render of an ordered aggregate reads ids where the delta log path decodes |
| C. struct/json boundary renders null | `struct_nested_value_renders_whole_tree`, `struct_ghcacher_stars_normalization`, `json_typed_capture_folds_into_a_keyed_int_total`, `zombie_scope_negative_case_a2b` | `"file":null`, `"full_name":null` inside rendered trees — a text member inside a struct/json rendering resolves through `__str` and misses (value never interned on that path), or the renderer emits the raw id slot as null |
| D. demand keys | `switch_as_keyed_replace`, `merge_policy`, `exhaust_policy`, `concat_program_queue` | `demanded.add [["route_data(settings)","session_one"]]` shape present but diff at line 1 — demand-key text (left-of-arrow rel-term keys) crossing the dictionary on one side only |

Overlap note: families C and D share members with I-K's 41-module list; the
write side interns, so these are READ/RENDER paths the intern arc never
covered — every door has a sibling that bypasses it (the boot-seed lesson,
third appearance).

## What this means

- I-C/I-K covered: literals, built-string writes, value-demand decode in rule
  bodies. The referee exposed four MORE planes that touch text at dict:
  departed/pre state reads, final-snapshot ordered aggregates, struct/json
  boundary rendering, and demand keys.
- Nothing regressed at direct: the same tree sweeps `wrong=0` with the atom
  reverted (receipt in PR #30's gate block).
- Next lane (I-L, unnamed): one family at a time, A first (it has actual
  wrong VALUES, not just nulls — pairwise reading 9 where the oracle says 14
  smells like an id leaking into arithmetic, the worst class on the board).

## Receipts

Full first-diff lines per module: `v6/prolog/compile/out/run-results.json`
from the red run (not committed; regenerate by flipping the atom in a
worktree and running `bash scripts/sweep.sh`). The four family buckets above
were assigned from those lines verbatim.
