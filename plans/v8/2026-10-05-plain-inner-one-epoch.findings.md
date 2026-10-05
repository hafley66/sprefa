# Findings: `__ir_plain_inner` and the second epoch

Brief: `plans/v8/2026-10-05-plain-inner-one-epoch.brief.md`. Base sprefa `676f4c499`, sqlite_ivm
`3f7eb15` (unchanged; a clone inside the worktree carried temporary counters for the
measurements below and was reset). Debug binaries (`[profile.dev] opt-level = 1`, dependencies
`opt-level = 3`), `RCARGO_OFF=1`. Program: `dl8 build std/tsi_registry_emit.dl7`, output `cmp`
against `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs`.

Temporary counters (not committed): per settle, the constructor rows each extra dd epoch fed
(by functor); per Mint node and epoch, rows, fresh terms and rows whose minted id equals an input
column; per IR node, rows by `(tick, round)`; per sqlite closure pass, fresh `ivm_term` rows by
functor and cumulative statements; in lowering, every IR node that reads `Get` of
`__ir_plain_lists`, `__ir_plain_inner`, `9:2:[|]` or `9:1:const`.

## Before

### Readers

In all four lowerings of the registry build (comptime of the macro library, the macrotime
engine, the comptime main program twice), the non-decode readers of `9:2:[|]` and `9:1:const`
are the two joins inside the `plain_inner`/`plain_lists` LetRec:

| node | reads | join column |
|---|---|---|
| recursive step | `__ir_plain_inner`, `9:2:[|]` | `plain_inner.cell = cons.tail` |
| `plain_lists` body | `__ir_plain_inner`, `9:1:const` | `plain_inner.cell = const.arg` |

Every other read of `9:2:[|]` (116 joins) and `9:1:const` (278 joins) is a decode on the
constructor id. `__ir_plain_lists` has 0 readers in all four lowerings: each `intern` over a
list in this program takes the `built_tuples` path, which mints the plain list from the
elements. `dynamic_lists` is true in all four, so the LetRec is built anyway.

ivm-dd feeds a constructor's minted rows only when some operator reads it with `Get`
(`constructor_inputs.retain(|id, _| rel.read.contains(id))`); a decode reads the interner. The
two LetRec joins are what make `9:2:[|]` and `9:1:const` fed constructors, so every settle that
mints a list or a `const` term runs one more epoch.

### Epochs (ir:dd)

| install | settle | epochs | fed after epoch 1 | settle ms |
|---|---|---|---|---|
| macrotime engine | unit 1 wave 0 (`sent=10713`) | 2 | 44 `9:2:[|]` | 28.1 |
| comptime main | settle 1 (`sent=5095`) | 2 | 1180 `9:2:[|]`, 5 `9:1:const` | 54.5 |
| comptime main | settle 2 (`sent=2870`) | 2 | 394 `9:2:[|]` | 24.4 |

All other settles: 1 epoch. 852 `9:1:ref` and 852 `9:2:application` terms minted in the same
epoch are not fed: no `Get` reader.

What epoch 2 holds, comptime main settle 1 (node taps, which change scheduling, so the counts
below come from the tapped run except where noted):

- Epoch 1 (tick 0): rule Mints create the 1180 cons cells. The recursive join sees only
  `plain_inner([], [])`; no cons rows exist yet.
- Epoch 2 (tick 1): the 1180 cons rows enter. The recursion runs three rounds: join rows
  694, 266, 36; Mint rows 411, 266, 36. Rows whose minted id equals an input cell: 0. The rows
  are plain twins of rule-built lists whose heads carry `ref`/`const` wrappers: new terms, no
  identity pairs `plain_inner(c, c)`.
- Without taps the LetRec has no probed downstream (no reader of `plain_lists`, no output), so
  `step_while` stops before it finishes: its epoch-2 work runs inside settle 2's first epoch
  (Mint rows 677, fresh 394), and those 394 cons feed settle 2's second epoch, where the
  recursion produces 0 Mint rows.

### Passes (ir:sqlite)

The sqlite closure loop runs another pass while `count(*) FROM ivm_term` grows.

| install | settle | passes | fresh terms per pass | statements | rounds |
|---|---|---|---|---|---|
| comptime of the macro library | settle 1 | 1 | 0 | 2375 | 2 |
| macrotime engine | settle 1 (`sent=180`) | 1 | 0 | 1216 | 1 |
| macrotime engine | unit 1 wave 0 | 2 | 116, 0 | 12628 | 9 |
| comptime main | settle 1 | 3 | 2899 (1180 cons, 852 ref, 852 application, 5 const, 5 str, 5 text), 614 cons, 0 | 16022 | 72 |
| comptime main | settle 2 | 1 | 0 | 3702 | 0 |

The 614 cons of pass 1 are the plain twins (sqlite sees the rule-built cons in pass 1 and
finishes the recursion inside settle 1; dd lags it into settle 2). Same final rows.

### Why "see its own mints" leaves the epoch

Reading cons rows as `get_constructor(cons) ∪ (id, head, tail)` of the LetRec's own Mint
lets a twin minted in round r join in round r+1. The epoch-2 input in settle 1 is the 1180
cons cells minted by rules in epoch 1, outside the LetRec; they reach a `Get(9:2:[|])` reader
only in the next epoch, and the twins' own rows would still be fed (the `Get` reader remains),
costing one epoch that changes nothing. Since `plain_lists` has no reader, the brief's
alternative applies: the LetRec, and with it both argument-column reads, is dropped.

## Change

`src/_6_eval/_9a_ir_lower.rs`, `lower_declared`: the `plain_inner`/`plain_lists` LetRec is
built after the rules are compiled, and only when some IR node reads `Get(__ir_plain_lists)`. It
is inserted at the stratum index it held before (ahead of `__ir_semantic`'s insertion at the same
index, so the order is `semantic`, LetRec, rules, as before). Without a reader,
`__ir_plain_lists` has no defining stratum, as in a program without `intern`. The non-dynamic
case (`plain_lists` = the seeds) is unchanged. No engine change.

## How it works

`9:2:[|]` and `9:1:const` are read only by decodes now, so no operator `Get`s them and ivm-dd
feeds none of their minted rows: a settle that mints lists ends after the epoch that minted
them. On sqlite the closure loop still runs one pass after any pass that grew `ivm_term`; with
nothing reading the constructors by argument that pass mints nothing and stops. A program that
reads `plain_lists` gets the same LetRec, and the same extra epoch, as before.

## After

### Counts (traced run, same binaries as below plus counters)

| install | settle | dd epochs | dd settle ms | sqlite passes | sqlite statements | sqlite rounds |
|---|---|---|---|---|---|---|
| comptime of the macro library | 1 | 1 → 1 | 10.9 → 11.1 | 1 → 1 | 2375 → 2281 | 2 → 1 |
| macrotime engine | `sent=180` | 1 → 1 | 4.3 → 4.0 | 1 → 1 | 1216 → 1122 | 1 → 0 |
| macrotime engine | unit 1 wave 0 | 2 → 1 | 28.1 → 23.4 | 2 → 2 | 12628 → 12419 | 9 → 6 |
| comptime main | settle 1 | 2 → 1 (steps 740 → 737) | 54.5 → 46.4 | 3 → 2 | 16022 → 15550 | 72 → 63 |
| comptime main | settle 2 | 2 → 1 (steps 9 → 6) | 24.4 → 15.5 | 1 → 1 | 3702 → 3699 | 0 → 0 |

dd operators per install: 3992 → 3897, 4031 → 3936, 13865 → 13770. IR nodes: 1075 → 1057,
1087 → 1069, 3657 → 3640.

### Registry build, untraced, 5 interleaved rounds (base, after per engine per round)

Load 4.6-5.2 (`uptime`). Output `cmp`-identical on every run.

| engine | build | real s (5 runs) | median | max RSS MB |
|---|---|---|---|---|
| ir:dd | base | 0.47 0.37 0.37 0.38 0.37 | 0.37 | 116-129 |
| ir:dd | after | 0.38 0.35 0.34 0.35 0.35 | 0.35 | 104-122 |
| ir:sqlite | base | 1.90 1.90 1.91 1.94 1.89 | 1.90 | 230-259 |
| ir:sqlite | after | 1.87 1.82 1.84 1.83 1.85 | 1.84 | 221-256 |

### Byte identity

- Registry `cmp`: identical, both engines, every run above.
- Macrotime oracle: `dl8 expand` on all 24 `oracle/macrotime/*.json` cases, both engines, base
  vs after stdout+stderr+exit code: 48 pairs, 0 differences.
- Suites: below.

### Suites (after)

Load 1.6-3.5. The first run failed 27 `_4_sqlite` tests per engine on a missing
`sqlite_ivm/target/extension/release/libsqlite_ivm.dylib` (fresh clone); rebuilt with
`sqlite_ivm/scripts/0_build.sh release` and rerun:

| suite | result | wall |
|---|---|---|
| `cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book --test-threads=1` (ir:sqlite) | 164 passed, 0 failed, 24 filtered | 3m57s |
| same with `DL8_ENGINE=ir:dd` | 164 passed, 0 failed, 24 filtered | 1m57s |

No golden or snapshot file changed (`git status`: the lowering file and this findings file).
