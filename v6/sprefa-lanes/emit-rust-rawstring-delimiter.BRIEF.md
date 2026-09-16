# Lane: emit_rust writes Rust that rustc rejects

## Base
`git merge --ff-only e70417d9` is your FIRST action, then merge
`origin/feature/emit-rust-climb-3` (PR #226, grade 280) in.
Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/fix/emit-rust-rawstring-delimiter`.

## The bug, measured 2026-08-12 by the rust-dl6-reload lane

`rustc` REJECTS the large emitted source as `emit_rust` currently writes it. The
raw-string delimiter collides at `color="#1d4ed8"`.

```
large source, as currently emitted          0.15 s   rustc ERROR, 3 of 3 runs
large source, delimiter repaired by hand    2.07 s   compiles
```

`emit_rust` picks a raw-string delimiter that can occur inside the JSON payload
it is wrapping. When it does, the emitted file is not valid Rust.

## Why nothing caught it

`v6/sprefa-engine-rs/grade.sh` compiles ONE program as a compile check
(`door-handwritten.dl6`) and runs the other 391 through a harness that reads the
emitted file as text. A fixture whose emitted Rust does not compile is invisible
to the grade.

## Scope

1. Choose a raw-string delimiter that cannot occur in the payload. The safe
   shape is to scan the payload for the longest run of `"` followed by `#`
   characters and pick one more `#` than that. Do NOT pick a fixed
   `r###"..."###` and hope.
2. Prove it on the failing program. Report the exact fixture name and its
   `rustc` exit before and after.
3. Extend the gate so this class cannot return. `grade.sh` compiles one program
   today. Make it compile enough to catch a bad delimiter, and say what that
   costs in wall time. The 10-second law binds: grade.sh is 14s cold and ~9s
   warm now, and it builds one crate total. If your change pushes it past 10s
   warm, say so plainly and propose the split rather than normalising it.
4. `byte-clean` must not fall below **280**.

## Anchors
- `v6/prolog/emit_rust.pl` the backend
- `v6/sprefa-engine-rs/grade.sh` the compile-check block writes
  `$scratch/compile-check` and builds `door-handwritten.dl6` only
- `plans/2026-08-12-rust-dl6-reload.RESEARCH.md` section 2, the measurement
- Rust raw strings: `r"..."`, `r#"..."#`, `r##"..."##`; the delimiter must have
  more `#` than any `"#`-run inside the body

## Gates, three runs each, never from the whole gate
```
bash v6/sprefa-engine-rs/grade.sh      # byte-clean must be >= 280
just conformance                       # 392 PASS / 0 FAIL
cargo test --no-fail-fast
```
`just green-all` is RED by design. `.github/CI-KNOWN-RED.md` is the real gate and
is stale by 9 rows. Do not chase anything in it.

## Files you own
`v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/grade.sh`, plan doc
`plans/2026-08-12-emit-rust-rawstring-delimiter.md`.

## Files you must NOT touch
`v6/prolog/lower.pl`, `compile/7_emit_ts_types.pl`, `compile/8_emit_rust_types.pl`
(lane `fix/catalog-two-module-collapse` owns them), `v6/boop/**`, any
`Cargo.toml`, `v6/justfile` (lane `fix/rust-hygiene-sqlite-pin` owns those).

## COMMIT YOUR WORK
Six lanes today wrote their whole deliverable and exited rc=0 WITHOUT
COMMITTING. Commit on the branch before you exit. An uncommitted tree is an
undelivered lane.

## Laws
- The 10-second law: over 10s is a defect to investigate, never a budget.
- No `eprintln!` in src/**, `tracing` only.
- Comments state only constraints the code cannot show. No dates, no narrative.
- Doubt yourself before asserting. Measure, do not estimate.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.

## Report
The failing fixture name, rustc before and after, the new gate's wall time, and
the grade number.
