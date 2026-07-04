---
name: sprefa-v5-working-conventions
description: Repo conventions for the v5 dl engine (~/projects/sprefa): e2e test sandbox, macOS timeouts, style rules.
---

# v5 Working Conventions

## E2E test sandbox pattern

All e2e tests use the compiled binary, located via `env!("CARGO_BIN_EXE_dl")`. Each test builds a temp dir, writes `.dl` program text and fixture files into it, runs `dl` with `--root` and `--db` flags, and asserts on stdout.

```rust
const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mytest_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(),
               "--db",   dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}
```

Reference files: `tests/it/data_ops.rs`, `tests/it/discover.rs`, `tests/it/rule_edit.rs` —
a single integration harness (`tests/it/main.rs`); adding a new test file means
registering its module there.

## Hard timeouts on macOS

macOS ships no `timeout` binary. For foreground test runs that could hang, wrap with:

```sh
perl -e 'alarm N; exec @ARGV' -- cargo test ...
```

Replace `N` with seconds. Sends `SIGALRM` after N seconds, killing the child process.

## Style rules

| Rule | Detail |
|---|---|
| No per-row writes | Collect the full set, call `Db::insert_rows` / `refresh_rel` once. The tick N+1 counter fires on violations. |
| Plural Db seam | `db.rs` is the chokepoint. `conn()` is a metered escape hatch (grep `.conn()` across `src/engine/*.rs` to count live sites; `engine.rs` was split into `mod.rs`/`tick.rs`/`extract.rs` in the 2026-06-30 breakdown). |
| Collect-then-flush | Populate a `Vec`, then one batched write. Never stream rows out one at a time inside a processing loop. |
| One rel = one rule kind | Never head a rel with both a source rule (`scan`/`match`/`ast`/`sg`/`json`/`cmd`/`comment`) and a derived rule: `rebuild_derived`'s full `DELETE FROM rel` would drop the reconciled source rows. The engine bails; split into two rels and union in a third. Same hazard for a term-extract rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule. |
| Descriptive dl variable names | Every rule/query/decl variable names the thing it binds (`import_path`, `callee_name`, `severity`), never a single letter (`p`, `l`, `x`, `t`). A rule capture variable names the thing captured. This applies as much to a throwaway scratch `.dl` as to a checked-in `examples/*.dl`. |
| Banned identifiers | `provenance` → `source`, `substrate` → `base`, `load-bearing` → `critical`, `regime` → `mode`. Flag existing ones for rename. |

## Branch and build conventions

- Active work goes on feature branches; merge to `main` fast-forward after suite passes.
- Install the binary before running tests against a warm db: `cargo install --path . --bin dl`
  (v5 was lifted to the repo root 2026-07-01; no more `v5/` subdir).
- The test suite runs in parallel by default; sandbox dirs use a `tag` suffix to avoid collisions.

## Facts and persisted rels (for .dl authors)

- Bare facts work: `tour("hubs", "title").` — body-less clauses are rows.
- Every declared rel persists to the db as `rel_<name>` with the DECLARED column
  names plus a `__src` bookkeeping column (strip `__`-prefixed columns when
  reading from outside).
- `ref` is reserved (the span spine `ref(id, string, file, lo, hi)`); programs
  needing a ref-shaped rel use another name (anim uses `node_ref`).
- TS/TSX feed `type_edge` too (oxc extractor, 2026-06-12); `.kts` routes to the
  Kotlin extractor even though `LIKE '%.ts'` matches it.
