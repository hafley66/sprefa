# Findings: macrotime on deltas

Brief: `plans/v8/2026-10-04-macrotime-deltas.brief.md`. Base sprefa `50aca5922`, sqlite_ivm
`b791f71` (unchanged). Debug binaries (`[profile.dev] opt-level = 1`, dependencies
`opt-level = 3`), `RCARGO_OFF=1`. Numbers come from `tracing` lines added for this measurement
(target `dl8::macrotime`: one line per wave and per unit; target `dl8::ir`: rows sent to and
changed by each settle, rows read by each snapshot and each intern read). Command:
`RUST_LOG=dl8::macrotime=info,dl8::ir=info dl8 build std/tsi_registry_emit.dl7 --out <tmp>`
and `dl8 expand oracle/macrotime/15_standard_plus_1.json` (the c15 case), `DL8_ENGINE` per row.

## Before

Load 6.8-7.1 during the runs.

### Wall, three runs each

| build | engine | real s |
|---|---|---|
| registry (`cmp`-identical every run) | ir:sqlite | 2.76, 2.79, 2.76 |
| registry | ir:dd | 0.43, 0.40, 0.41 |
| c15 expand | ir:sqlite | 0.34, 0.34, 0.34 |
| c15 expand | ir:dd | 0.06, 0.07, 0.06 |

### Per unit and per wave

`seeds` is the full seed list built for the wave (`syntax_seeds(graph.rows)` plus macro seeds);
`sent` is the source change list after the session diffs it against `last_seeds` (includes one
`__ir_term_map` row per mapped term on wave 0); `changed` is the engine delta; `snapshot` is
the rows `snapshot_closure` reads back (every visible relation); `intern` is the rows
`decode_terms` reads (every constructor relation). Each unit runs one `ir install`.

| unit | wave | seeds | sent | changed | snapshot | intern | sqlite ms | dd ms |
|---|---|---|---|---|---|---|---|---|
| registry 1 | 0 | 3934 | 10893 | 4094 | 4093 | 5694 | 320 | 49-50 |
| registry 1 | 1 | 3934 | 268 | 178 | 3934 | 5694 | 38 | 20-21 |
| registry 2 | 0 | 558 | 2109 | 559 | 558 | 1185 | 168 | 21-23 |
| registry 3 | 0 | 402 | 1520 | 403 | 402 | 870 | 159-161 | 20 |
| registry 4 | 0 | 3518 | 12109 | 3519 | 3518 | 6472 | 295-300 | 42-43 |
| c15 | 0 | 110 | 527 | 126 | 125 | 354 | 170 | 23 |
| c15 | 1 | 110 | 32 | 18 | 110 | 354 | 10 | 12 |

Unit wall (`unit_us`, sqlite / dd ms): registry 364 / 74-77, 168 / 21-23, 159-161 / 20,
296-301 / 42-44; c15 180 / 35. The unit wall minus its evaluate time is under 5 ms: the Rust
rewrite is not where the time goes.

### What a wave re-does today

- `IrSession::evaluate` re-lowers the whole program every wave
  (`src/_6_eval/_9b_ir_eval.rs:208`), then hashes the serialized IR to decide whether the
  installed session matches (`:217`). On these inputs wave 1 always matched (one install per
  unit).
- Wave 1 builds 3934 seed rows to send 268 changes (registry 1), 110 to send 32 (c15).
- Wave 1 reads back 3934 snapshot rows plus 5694 intern rows to learn that 178 rows changed.
- `seen` holds a full clone of every wave's row set (`src/_1_macrotime/_4_expand.rs:119,181`).

Wave counts are small: registry units take 2, 1, 1, 1 waves, c15 takes 2. Wave 0 (install plus
the first settle) is most of each unit; the delta path changes waves 1 and later plus the
per-wave glue.

## After

Load 4.5-4.7 during the runs. Same commands. Output `cmp`-identical: registry every run, c15
`dl8 expand` JSON byte-identical to the before run on both engines.

### Wall, three runs each

| build | engine | before real s | after real s |
|---|---|---|---|
| registry | ir:sqlite | 2.76, 2.79, 2.76 | 2.88, 2.83, 2.78 |
| registry | ir:dd | 0.43, 0.40, 0.41 | 0.42, 0.40, 0.40 |
| c15 expand | ir:sqlite | 0.34, 0.34, 0.34 | 0.34, 0.35, 0.34 |
| c15 expand | ir:dd | 0.06, 0.07, 0.06 | 0.06, 0.06, 0.06 |

Whole-build wall does not move: wave 0 (install plus first settle) is most of every unit and
waves 1+ are one wave in the registry build and one in c15.

### Per wave

`sent`: source rows given to the settle. `read`: closure rows the expander reads (before: the
snapshot of every visible relation; after: protocol rows of the delta). `intern`: interned rows
read to decode cells.

| unit | wave | sent before / after | read before / after | intern before / after | sqlite ms before / after | dd ms before / after |
|---|---|---|---|---|---|---|
| registry 1 | 0 | 10893 / 10893 | 4093 / 4042 | 5694 / 5694 | 320 / 326-336 | 49-50 / 52-54 |
| registry 1 | 1 | 268 / 268 | 3934 / 132 | 5694 / 0 | 38 / 31-33 | 20-21 / 11-13 |
| registry 2 | 0 | 2109 / 2109 | 558 / 552 | 1185 / 1185 | 168 / 170-171 | 21-23 / 21 |
| registry 3 | 0 | 1520 / 1520 | 402 / 396 | 870 / 870 | 159-161 / 163-165 | 20 / 19-21 |
| registry 4 | 0 | 12109 / 12109 | 3518 / 3512 | 6472 / 6472 | 295-300 / 295-316 | 42-43 / 44-45 |
| c15 | 0 | 527 / 527 | 125 / 115 | 354 / 354 | 170 / 167-171 | 23 / 24-25 |
| c15 | 1 | 32 / 30 | 110 / 13 | 354 / 0 | 10 / 8 | 12 / 9-10 |

Seed rows built per wave: before, the whole graph every wave (3934 on registry 1 wave 1);
after, the graph once at prepare and then only the removed and added rows (268 on the same
wave). Re-lowers per unit: before one per wave, after one per unit. Re-prepares
(`macrotime reprepare`): zero on both inputs. `seen`: one `u64` per wave instead of a row-set
clone.

## How it works

### The problem, in numbers

Registry unit 1 takes two waves. Its second wave changes 268 seed rows of 3934. Before this
change that wave rebuilt all 3934 seed rows, re-lowered the whole macro program, diffed two
`BTreeSet`s of every seed to find the 268, then read 3934 closure rows and 5694 interned rows
back to learn which 178 output rows moved. After it, the wave sends the 268 rows, reads the 177
changed rows the engine returns (132 of them protocol rows), and reads no interned rows.

### Step trace of one expansion (`src/_1_macrotime/_4_expand.rs:93`)

1. Build the graph from the reified rows (`Graph::build`, `_0_rows.rs`); dispatch check as
   before.
2. `prepare` (`_4_expand.rs:251`): macro seeds plus `syntax_seeds(graph.rows)`, then
   `IrSession::prepare` (`src/_6_eval/_9b_ir_eval.rs:243`): lower once, install once,
   `start_session` (`:760`) settles the whole seed frontier (`bootstrap_changes`, `:673`, the
   prologue `run_session` also uses) and decodes the delta (`decode_delta`, `:799`).
3. Each wave: `Available::apply` (`src/_1_macrotime/_2_protocol.rs:538`) folds the delta in.
   It keeps tagged-row weights, the untagged syntax graph the closure derives
   (`Available::graph`), identity counts for the `node`/`item` filter, the present claim,
   expansion and diagnostic rows (`results`), and `extra`: derived rows the expanded graph
   does not hold.
4. `macro_results` (`_2_protocol.rs:454`) reads claims, expansion edges and reported
   diagnostics from `results` only, filtered by the active set.
5. `rewrite_active_graph` (`src/_1_macrotime/_3_rewrite.rs:35`) walks the frontier as before;
   `apply_rewrite` (`:181`) computes the new reachable set, then `removed` (graph rows no longer
   kept, `:234`) and `added` (new frontier rows, new override edges, and `extra` rows on
   reachable nodes, `:249`).
6. Cycle check (`_4_expand.rs:207`): the predicted `Graph::hash` (wrapping sum of
   `row_hash`, `_0_rows.rs:337`) against the hashes of earlier waves.
7. `Graph::apply` (`_0_rows.rs:160`) edits rows and every index in place, keeping index order
   equal to sorted-row order. `Available::expanded_moved` (`_2_protocol.rs:612`) keeps `extra`
   in step.
8. `IrSession::apply` (`_9b_ir_eval.rs:262`, `apply_session` `:905`) gets the removed rows at
   weight -1 and the added rows at +1. New terms are installed (`install_fresh`, `:874`) with
   the term-map rows and `__ir_plain_list_seeds` / `__ir_bad_lists` rows `ir_lower::lower` seeds
   for its own terms (`Lowered::plain_source`, `Lowered::bad_lists`,
   `src/_6_eval/_9a_ir_lower.rs:66`). A row whose relation has no source, or a term whose shape
   has no constructor in the prepared IR, returns `ApplyError::Unprepared`; the expander logs
   `macrotime reprepare` and prepares again from the whole graph.
9. No claims: the rows are sorted once and returned.

### Where the remaining time is

Per unit, the install plus the first settle (sqlite 160-330 ms, dd 20-55 ms) is nearly all of
it. Page 2 of the plan (one engine shared across units) is where the wall time is.
