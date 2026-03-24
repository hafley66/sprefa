# Scan Pipeline Plan

## Goal

Wire the rule engine into the Extractor trait, build the scan pipeline,
get `sprefa scan` producing real index data from real repos.

## Architecture constraints

- **Batch DB writes**: scans accumulate refs in memory, flush to SQLite
  in bulk at threshold (e.g. 10k refs or end of file). No per-ref INSERT.
  Use SQLite transactions with batch INSERT.
- **Parallel parse**: files are independent. Parse + extract in parallel
  via rayon or tokio::spawn_blocking. Only the DB write is serialized.
- **Pre-compile rules**: globs, regexes compiled once at load time into
  CompiledRule. No recompilation per file.

## Build order

### 1. RuleExtractor (`crates/rules/src/extractor.rs`)

Pre-compiled rule set implementing the Extractor trait.

```
RuleExtractor {
    rules: Vec<CompiledRule>,
}

CompiledRule {
    name: String,
    git: Option<CompiledGitSelector>,
    file: CompiledFileSelector,
    steps: Vec<StructStep>,
    value_pattern: Option<ValuePattern>,
    value_regex: Option<Regex>,   // pre-compiled
    action: Action,
}
```

- `from_ruleset(ruleset) -> Result<Self>`: compile all rules
- `from_json(path) -> Result<Self>`: load + compile
- `impl Extractor`: parse source by extension (json/yaml/toml),
  run matching rules, walk + emit, return `Vec<RawRef>`

New dep: `serde_yaml` in workspace.

Tests: load rules JSON, parse fixture files, snapshot RawRef output.

### 2. normalize (`crates/scan/src/normalize.rs`)

```
fn normalize(value: &str) -> String
fn normalize2(value: &str, config: &Option<NormalizeConfig>) -> Option<String>
```

Tests: snapshot norm + norm2 for various inputs.

### 3. list_files (`crates/scan/src/files.rs`)

```
fn list_files(repo_path: &Path, filter: &Option<FilterConfig>) -> Result<Vec<PathBuf>>
```

- Try `git ls-files` first (respects .gitignore)
- Fallback: walkdir, skip .git
- Apply CompiledFilter from config crate

Tests: temp dir with filter.

### 4. Scanner (`crates/scan/src/scanner.rs`)

Orchestrates the full scan.

```
struct Scanner {
    extractors: Vec<Box<dyn Extractor>>,
    db: SqlitePool,
    normalize_config: Option<NormalizeConfig>,
}
```

**Scan flow per repo:**

1. `git ls-files` -> file list
2. Apply filter (global + repo + branch cascade)
3. Group files by extension -> matching extractor
4. **Parallel**: for each file, read bytes + extract refs (rayon)
5. **Batch**: accumulate (file_id, raw_refs) in memory
6. **Flush**: at threshold or end-of-repo, open transaction:
   - Batch upsert files
   - Batch upsert strings (dedup in-memory first via HashMap)
   - Batch insert refs
   - Single COMMIT
7. Return ScanResult { repo, files, refs }

**Batch strategy:**
- In-memory string dedup: `HashMap<String, (norm, norm2)>` avoids
  re-normalizing and re-inserting the same string
- Flush threshold: 10k refs or end of file batch
- Single SQLite transaction per flush (WAL mode for concurrent reads)

**Parallelism:**
- rayon::par_iter over files for parse + extract
- Collect results into Vec, then single-threaded DB write
- Alternative: crossbeam channel with dedicated writer thread

### 5. Wire CLI (`crates/cli/src/main.rs`)

- `sprefa scan [--repo <name>]`: load config, load rules, create scanner, run
- `sprefa query <term>`: already works once DB is populated
- Rules file path: `$SPREFA_RULES` > `./sprefa-rules.json` > `~/.config/sprefa/rules.json`

### 6. End-to-end test

```
sprefa init
sprefa add /tmp/test-repo --name test
sprefa scan
sprefa query "express"
```

## New workspace deps

```toml
serde_yaml = "0.9"
rayon = "1"
walkdir = "2"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
```

## Normalization future

Per-rule normalization via transform pipes (URTSL `value:` property):
```
value: $raw | strip(/-(chart|go|ts)$/) | lowercase;
```
Not built now. Current: global strip_suffixes + per-rule value regex.
The value regex already handles per-rule transforms in practice.

## Open questions

- Content hash: xxh3 (fast) or sha256 (collision-proof)?
  Recommend xxh3 for file-changed detection, not security.
- Should scanner track git hash per branch for incremental scan?
  Schema has repo_branches.git_hash for this. Wire it.
- rayon vs tokio::spawn_blocking? rayon is simpler for CPU-bound
  parse work. tokio for async DB writes. Can mix: rayon for parse,
  tokio for DB.
