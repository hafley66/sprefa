# REPORT — pass 1 of 2: extract --help rewrite

Owned file: `v6/sprefa-extract/src/bin/extract/help.rs`. Both `LONG_ABOUT` and
`FAMILY_LONG` match brief.md exactly (verified by comparison against the
brief's code fences). No other constant, clap attribute, or flag touched.
`git status` tracked: only help.rs modified.

## Gate outputs (verbatim)

### 1. cargo build --release --features cli --bin extract (v6/sprefa-extract)
```
    Finished `release` profile [optimized] target(s) in 0.28s
```

### 2. ./target/release/extract --help | head -40
```
Read source files and print facts about the code as JSONL (one JSON object per
line) on stdout. No daemon, no database, no network: point it at files, get
facts, pipe them anywhere.

QUICK START
  extract src/app.ts                       every fact kind for one file
  extract --family call src/app.ts         only call-graph facts
  extract --resolve a.ts b.ts              cross-file call edges, parse-based
  extract --family scip .                  whole-project facts from the real
                                           compiler index (exact, slower)
  extract --schema                         every record shape this can emit

WHAT --family MEANS
  One flag, two jobs; the second grew out of the first.

  Job 1, fact kinds. A comma-separated subset of what to extract from each
  file, default all four:
    cst    the syntax tree
    type   type declarations and annotations
    call   calls and definitions
    df     dataflow
  `--family call,type src/app.ts` narrows one file's output.

  Job 2, whole-project modes. Two special names that change the run's shape
  instead of filtering it (mixing them with fact kinds is an error):
    scip        exact facts from the language's own compiler/indexer
    diet_scip   fast facts from parsing alone, no compiler involved
  "diet" names the technique (parse + name matching). It never means partial
  SCIP data.

EXACT MODE: --family scip ROOT
  Detects the project kind from marker files (Cargo.toml -> rust-analyzer,
  tsconfig.json or package.json -> scip-typescript, go.mod -> scip-go), builds
  or reuses the compiler's index, and streams it as scip_* relations: scip_def,
  scip_name, scip_ref, scip_edge, scip_fn_edge, scip_callee_type, scip_local,
  scip_impl, plus one scip_index header row. Every fact is compiler-resolved.
  An index already on disk is reused untouched; a fresh build runs once under a
  time budget (the indexer's whole process group is killed at the deadline) and
  is cached for next time.
```

### 3. cargo test --features cli (v6/sprefa-extract)
```
     Running unittests src/lib.rs (target/debug/deps/sprefa_extract-de4a16ff31f1a8d1)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running unittests src/bin/extract.rs (target/debug/deps/extract-00a6610a8361201e)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/0_prolog.rs (target/debug/deps/0_prolog-d00ad1eefebfbc66)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
     Running tests/1_resolve_cli.rs (target/debug/deps/1_resolve_cli-c72acbcce1c9a341)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.68s
     Running tests/2_df_aux_cli.rs (target/debug/deps/2_df_aux_cli-d07d2ba38ded6325)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
     Running tests/3_ast_pattern_cli.rs (target/debug/deps/3_ast_pattern_cli-0ddc97cfb2ceeb74)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/4_capability_parity.rs (target/debug/deps/4_capability_parity-b9c59fc9d1f0b831)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.27s
     Running tests/5_scip_facts_cli.rs (target/debug/deps/5_scip_facts_cli-5a315c113911635b)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
     Running tests/6_document_formats.rs (target/debug/deps/6_document_formats-3f8cfbb24ce89a1e)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
     Running tests/7_diet_deps_cli.rs (target/debug/deps/7_diet_deps_cli-58174d586ee38bea)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/8_scip_families_cli.rs (target/debug/deps/8_scip_families_cli-b3e9680e02180351)
test result: FAILED. 13 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.33s
```

The failing test:
```
---- the_binary_states_what_diet_means stdout ----
thread 'the_binary_states_what_diet_means' (1652956782) panicked at tests/8_scip_families_cli.rs:542:5:
missing from --help: <the full new LONG_ABOUT text>

failures:
    the_binary_states_what_diet_means

test result: FAILED. 13 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.30s
```

### 4. git diff --stat
```
 v6/sprefa-extract/src/bin/extract/help.rs | 205 ++++++++++++++++--------------
 1 file changed, 109 insertions(+), 96 deletions(-)
```

## Deviations (expected per brief: none)

1. `cargo test --features cli` is not green. One test fails:
   `the_binary_states_what_diet_means` in `tests/8_scip_families_cli.rs:541`.
   It greps the rendered `--help` for the sentence
   `DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA`, which
   brief.md's mandated replacement text removes (new wording:
   `"diet" names the technique (parse + name matching). It never means partial
   SCIP data.`). The brief-gated test the brief names
   (`the_cli_help_names_the_fallback_formats`, `tests/6_document_formats.rs`)
   passes, and the LANGUAGE COVERAGE table lines survive.
2. This lane owns exactly one file (`help.rs`); `tests/8_scip_families_cli.rs`
   is outside that scope. Per the brief's "STOP and report; do not improvise",
   the test file is left untouched. Fixing the stale assertion is a
   coordinator/pass-2 decision, not a pass-1 ownership.
3. The commit is BLOCKED. The pre-commit rail's comment-budget stage
   (`v6/tsv2/scripts/comment-budget-rail.sh`) starts a node server
   (`v6/tsv2/serve/main.ts`) that imports `rxjs`, but `v6/tsv2/node_modules`
   does not exist and no `package-lock.json` is present; the server fails with
   `ERR_MODULE_NOT_FOUND: Cannot find package 'rxjs'`. Installing the declared
   deps (npm install) would be an npm dependency change, which the task
   explicitly forbids, and the failure is environmental, unrelated to the
   one-file change. Per the brief's "STOP and report; do not improvise", the
   commit was not forced (no `git commit -n`) and was not attempted with dep
   installs. State at stop: `git status` = staged `REPORT.md` + modified
   `help.rs`, untracked `brief.md`/`brief2.md`; `git log` HEAD unchanged at
   `173d308c`.
   The brief's claim that the rail "needs the extract binary you just built, so
   it should pass" only covers the `DL_EXTRACT_BIN` probe (which IS satisfied);
   the rail additionally requires the rxjs node server, which is the blocker.
