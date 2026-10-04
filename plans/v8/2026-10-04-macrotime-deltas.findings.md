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
