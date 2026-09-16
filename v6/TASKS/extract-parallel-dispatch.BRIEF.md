# extract-parallel-dispatch (issue: extract-blob-cache-parallel, PARALLEL half only)

FIRST ACTION: `git merge --ff-only 55e15e7478918a2a7c8b0c63ad4679f00250d099`.
Failure or missing tree = STOP AND REPORT, do not work around it.
Then read `CLAUDE.md` at the repo root.

## GOAL

`sprefa-extract` reads a corpus one file at a time on one thread. Make the
per-file extraction run on a rayon pool with a hard thread cap, with the output
order byte-identical to today.

This is the PARALLEL half of `@extract-blob-cache-parallel` only. The CACHE half
is NOT yours, is not decided, and must not be started.

## RECEIPTS (verified at 55e15e747, re-check before you edit)

| fact | receipt |
|---|---|
| the single-threaded loop, plain path | `v6/sprefa-extract/src/project.rs:456` `read_inputs_plain` |
| the single-threaded loop, git-batched path | `v6/sprefa-extract/src/project.rs:472` `read_inputs_batched` |
| the call each loop makes | `crate::dispatch(&path, &content, FamilyMask::ALL)` at `project.rs:461` and `:497` |
| `Source::extract` owns its arenas, nothing shared crosses the call | `v6/sprefa-extract/src/types.rs:1748-1750` |
| the trait is already `Sync + Send` | `v6/sprefa-extract/src/types.rs:1745` |
| rayon is absent from `[dependencies]` while the crate description claims it | `v6/sprefa-extract/Cargo.toml:14` |
| the declaration this closes | `v6/sprefa-extract/src/types.rs:2304-2305` |
| full analysis, with the library comparison already done | `plans/2026-08-17-extract-blob-cache-parallel.ANALYSIS.md` (PR #339) |

## MEASURED BASELINE, this is what you must beat

```
cd ~/projects/sprefa
FILES=$(git ls-files '*.rs' '*.ts' '*.tsx' '*.js' '*.go' '*.kt' '*.py' '*.html')
/usr/bin/time -l ./v6/sprefa-extract/target/release/extract --resolve $FILES > /dev/null
```

2343 files. Three runs on the driver's machine: **4.67 / 4.32 / 4.38 s real**,
4.00 s user (so `user` is within 7% of `real`: one thread, CPU-bound), peak RSS
400 MB. 12 logical cores, 8 performance.

Report the same three-run table before and after your change, from YOUR
worktree's release binary, built with
`cargo build --release --features cli --bin extract`.

## WHAT TO BUILD

1. `rayon = "1"` in `v6/sprefa-extract/Cargo.toml` `[dependencies]`.
2. A dedicated pool, never rayon's global pool, so nothing else in the process
   inherits it. A `static` `LazyLock<rayon::ThreadPool>` in `project.rs` built
   from `rayon::ThreadPoolBuilder::new().num_threads(cap)`.
3. The cap, in this order: `SPREFA_EXTRACT_THREADS` if set and parseable and
   nonzero, else `std::thread::available_parallelism()` clamped to at most 8,
   minus 1, floored at 1. The machine must stay usable while extraction runs
   ("nothing seizes the machine", `CLAUDE.md`).
4. Both loops become an order-preserving parallel map inside
   `pool.install(...)`: index the input list, `par_iter().map(...)` to
   `Result<Vec<Option<ProjectInput>>, ProjectError>`, then flatten the `None`s
   out in order. The returned `Vec<ProjectInput>` must be element-for-element
   identical to today's, including which files are skipped.
5. The error behavior does not change: the FIRST error by path order wins, the
   same one today's `?` would have returned. A parallel run that surfaces a
   different file's error than the sequential run did is a defect.

## WHAT NOT TO TOUCH

FORBIDDEN, any edit to these is a failed lane:

- `v6/sprefa-extract/src/lang/**` (another driver owns every language arm)
- `v6/sprefa-extract/src/types.rs`, `src/dispatch.rs`, `src/0_query.rs`,
  `src/wire.rs`, `src/scip.rs`, `src/shape.rs`
- every `df_*` and `doc_*` plane, anywhere
- `v6/sprefa-engine-rs/**` including its `Cargo.lock`
- `v6/prolog/**`, `v6/tsv2/**`, `v6/tools/**`, `.github/**`
- goldens and fixtures: regenerate nothing

YOURS, and nothing else:

- `v6/sprefa-extract/Cargo.toml`
- `v6/sprefa-extract/Cargo.lock` (the rayon rows ONLY)
- `v6/sprefa-extract/src/project.rs`
- one new file `v6/sprefa-extract/tests/26_parallel_dispatch.rs`

## TESTS

`tests/26_parallel_dispatch.rs`, with a header carrying CONTROL and at least two
SABOTAGE rows, each with its measured pass/fail split, in the shape of
`v6/sprefa-extract/tests/25_query_digest_repo_from_path.rs`.

Required assertions:

1. ORDER. Build a fixture directory of at least 30 source files with distinct
   contents, run the resolve path, and assert the produced order matches the
   input path order exactly. This is the test that catches an unordered
   `par_iter` collect.
2. SKIPS. Mix in files no `Source` matches (for instance `.txt`); assert they
   are absent and that the surviving order is still input order.
3. CAP. `SPREFA_EXTRACT_THREADS=1` produces byte-identical output to the
   unset default. Assert equality of the two runs.

Sabotage candidates that must each turn exactly one of those RED: collect
without preserving index order; drop the cap and use the global pool.

## GATE, run each TWICE, paste both runs

```bash
cd v6/sprefa-extract
cargo build --release --features cli --bin extract    # rc=0
cargo test --features cli                             # 143 + your new tests, 0 failed
cd ../.. && python3 v6/tools/soopy-lockstep.py        # PASS: one soopy closure, 127 crates
```

`cargo test --features cli` is THE gate. Bare `cargo test` is NOT: the `extract`
bin is behind `required-features`, so bare `cargo test` hands a nonexistent
`CARGO_BIN_EXE_extract` to the CLI tests and reports failures on a clean tree.

If `cargo test` dirties `v6/sprefa-engine-rs/Cargo.lock`, run
`git checkout -- v6/sprefa-engine-rs/Cargo.lock`. Check `git status` before
every commit.

## STYLE LAWS, non-negotiable

- Max 2 consecutive comment lines in new code. Comments state only constraints
  the code cannot show. No change-log narrative, no dates, no arc references.
- No `eprintln!` in `src/**`. `tracing` only.
- No em dashes anywhere, prose or code.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base, critical, mode.
- Descriptive names, never single letters, in every binding.
- NEVER run bare `cargo fmt`. Format only the lines you wrote.
- Do not rename anything you did not add.

## COMMIT AND PR

Commit message: what was wrong, what moved, the measured numbers. Trailer:

```
Refs-Issue: @extract-blob-cache-parallel
```

Then push and open a PR whose body carries: the before/after three-run timing
table, the two gate runs, the CONTROL and SABOTAGE table, and the thread-cap
rule you implemented. State the base sha.

Do not merge it yourself. Do not spawn subagents.
