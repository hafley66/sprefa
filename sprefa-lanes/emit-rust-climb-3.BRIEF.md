# Lane: emit_rust climb 3, take 230 higher

## Base
`git merge --ff-only e70417d9` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/feature/emit-rust-climb-3`.
`e70417d9` is PR #224, which took 109 to 230. Read
`plans/2026-08-12-emit-rust-climb-2.md` before touching anything.

## Where the number is

`bash v6/sprefa-engine-rs/grade.sh` prints `RUST-GRADE graded=392 byte-clean=230`.
14s cold, ~9s warm, builds one crate total.

| verdict | count | meaning |
|---|---:|---|
| clean | 230 | byte-identical to the oracle tick log |
| diff | 50 | ran, output differs |
| unsupported | 106 | prolog cannot emit Rust for it yet |
| error | 3 | Rust panicked |
| compiled | 3 | built, no oracle |

## The 50 diffs, already grouped

`diff_cause.py` writes `<category> first-tick=<n>` into `graded.tsv`. Nine
buckets. Three causes hold all 50:

| cause | count | what is missing |
|---|---:|---|
| ordered programs | 26 | `run_ordered_tick`, needs NEW emitter output, not a Rust-side fix |
| struct plane | 18 | `StructPlane.intern`, the sibling of the text plane fixed in `60eb43cc` |
| departure frontier | 6 | |

The text plane went in at `60eb43cc` and moved 109 to 169. The struct plane is
the same shape of work and is the cheapest 18 on the board. Start there.

## The 106 unsupported are the other half of the job

Nobody has triaged them. Classify each: is it a construct emit_ts supports and
emit_rust has not implemented, or is it unbuilt in both? Produce the table before
fixing any of them. That table is a deliverable on its own even if you fix none.

## How the previous lane got its classification wrong

It ran `diff -u oracle out | awk '/^[+-]/ {print; exit}'`. `diff -u` prints the
oracle `-` line before the rust `+` line, and `exit` fires on the first. Every
diff therefore read as "oracle line missing" when 108 of 171 were mixed and 49
were wrong-only. Compare per tick per rel, never on the first differing line.

An oracle line is one JSON object per tick:
```json
{"tick":1,"deltas":{"dispatch_ack":{"add":[[1]],"del":[]},"dispatch_note":{"add":[[1,"acked"]],"del":[]}}}
```
One wrong rel makes the whole line differ.

## Anchors
- `v6/prolog/emit_rust.pl` the backend
- `v6/prolog/emit_ts.pl:2518-2571` emits 15 tick phases; `run_tick` ran 6 before
  PR #224. Diff the two phase lists first.
- `v6/sprefa-engine-rs/**` the runtime, `graded.tsv` 392 rows, `grade.sh`,
  `diff_cause.py`
- `v6/tsv2/runtime/tickLoop.ts:30-32` the carry-fold order `7225a6e5` matched
- `v6/sprefa-engine-rs/src/program.rs:69` edge rules port status
- `v6/sprefa-engine-rs/src/sql.rs:25-27` holds a bare `Connection`, not `Sync`

## Gates, three runs each, never from the whole gate
```
bash v6/sprefa-engine-rs/grade.sh      # must not fall below 230
just conformance                       # 392 PASS / 0 FAIL
swipl -g go -t halt v6/prolog/ARCH.pl
cd v6/tsv2 && bash scripts/sweep.sh    # RUN identical / wrong / rejection
cargo test --no-fail-fast
```
`just green-all` is RED by design. `.github/CI-KNOWN-RED.md` is the real gate and
is stale by 9 rows. Do not chase anything listed there.

## Files you own
`v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/**`, plan doc
`plans/2026-08-12-emit-rust-climb-3.md`.

## Files you must NOT touch, other lanes own them
- `v6/prolog/lower.pl`, `compile/7_emit_ts_types.pl`, `compile/8_emit_rust_types.pl`
  (lane `fix/catalog-two-module-collapse`)
- any `Cargo.toml`, `v6/justfile`, `src/bin/emit_rust_harness.rs`
  (lane `fix/rust-hygiene-sqlite-pin`)
- `v6/boop/**`
- `plans/2026-08-12-rust-dl6-reload.*` (lane `plans/rust-dl6-reload`)

## Laws
- One commit per cause, with the before and after grade number in the message.
- No `eprintln!` in src/**, `tracing` only.
- Comments state only constraints the code cannot show. No dates, no narrative.
- The 10-second law: anything over 10s is a defect, never a budget.
- Never claim the language does not support X without citing the throw site.
- Doubt yourself before asserting. Measure, do not estimate.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.
- Language design belongs to the user. Report cited forks, do not decide.

## Report
The new grade number, one line per commit with its before and after, the
106-unsupported triage table, and the gate outputs.
