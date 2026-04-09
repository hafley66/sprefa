# sprefa

You touch 30 repos a week. You `cd` into one, grep for a string, find it references something in another repo, `cd` there, grep again, open a YAML, squint at an image tag, check if that tag exists, open the other repo at that tag, look at its package.json. That loop eats hours every day -- for you, for your teammates, for any LLM agent doing the same thing through tool calls.

sprefa encodes that loop into declarative rules and runs it across all your repos at once. Every captured string lands in a SQLite table. The connections between repos become SQL joins.

```
deploy values.yaml has    image: backend-api, tag: v2.1.0
package.json has          name: backend-api, version: 2.1.0
Cargo.toml has            name: backend-api
k8s manifest has          env: DATABASE_URL
.env has                  DATABASE_URL=postgres://...
```

One `sprefa scan` indexes all of that. One SQL query connects it.

### What if you had something that

- Scans all your repos once and remembers every interesting string, where it lives, and what it means
- Knows that `image: backend-api` in a deploy config and `name: backend-api` in a package.json are talking about the same thing
- When it sees `repo: X, tag: v2.1.0` in a config file, automatically checks out repo X at that tag and scans that too
- Lets you ask "which deploy configs reference a tag that doesn't exist" and get an answer in milliseconds
- When you move a file or rename a symbol, rewrites every import and use statement that references it

## Getting started

```bash
sprefa init                                      # create sprefa.toml + SQLite DB
sprefa add ~/repos/my-frontend                   # register repos
sprefa add ~/repos/my-backend
sprefa scan                                      # index everything
sprefa status                                    # see what got indexed
sprefa sql "SELECT * FROM rs_use LIMIT 10"       # query the index
```

### One-off extraction (eval)

`eval` runs a rule against files directly, no DB needed:

```bash
# extract package names from all Cargo.toml files
sprefa eval 'fs(**/Cargo.toml) > json({ package: { name: $N } })'

# extract from a specific file
sprefa eval 'json({ name: $N })' package.json

# pipe stdin
cat values.yaml | sprefa eval 'json({ image: { repository: $R } })'
```

### Scanning your own project

```bash
# after sprefa add . --name myproject
sprefa scan
sprefa sql "SELECT * FROM import_path LIMIT 20"
sprefa sql "SELECT value, COUNT(*) c FROM rs_use GROUP BY value ORDER BY c DESC LIMIT 10"
```

Every built-in extractor (JS/TS imports, Rust use paths, declarations) produces a table automatically. Every `.sprf` rule you write produces its own table too.

## The .sprf language

Three statement types: `rule`, `query`, `check`.

### rule -- extract structured data from files

```sprf
rule(package_name) {
  fs(**/Cargo.toml) > json({ package: { name: $NAME } })
};

rule(dep_name) {
  fs(**/Cargo.toml) > json({ re:^(dev-)?dependencies: { $NAME: $_ } })
};

rule(deploy_image) {
  fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } })
};

rule(env_var_ref_ts) {
  fs(**/*.ts) > ast(process.env.$NAME)
};
```

Each `$VAR` becomes a column in the rule's output table. Co-extracted captures from the same site share a `group_id`, preserving "these values came from the same place."

**Chain with `>`** -- sequential pipeline: glob files, then walk structure, then match AST.

**Branch with `{ }`** -- fork into independent extraction paths:

```sprf
rule(config_value) {
  fs(**/config.yaml) > json({
    database: { host: $HOST };
    database: { port: $PORT };
  })
};
```

Each semicolon-delimited line inside `{ }` is an independent branch (monomorphized at compile time into separate rules).

**Cross-rule references** -- bind columns from an upstream rule's table, creating a dependency edge:

```sprf
# level 0: extract repo+rev from deploy config
rule(deploy_ref) {
  fs(**/values.yaml) > json({ image: { repository: $REPO, tag: $TAG } })
};

# level 1: in each referenced repo at that rev, find internal deps
rule(internal_dep) {
  deploy_ref(repo: $REPO, rev: $TAG) {
    fs(**/package.json) > json({ dependencies: { $DEP: $SPEC } })
  }
};

# level 2: resolve pinned revs from the lock file
rule(lock_pin) {
  internal_dep(dep: $DEP, repo: $REPO) {
    fs(**/package-lock.json) > line($DEP.*resolved.*#$PINNED_REV)
  }
};
```

Rules execute in dependency order (DAG-guided). `deploy_ref` runs first, its results feed into `internal_dep`, which feeds into `lock_pin`.

**Rule unions** -- same name + same capture shape = rows in the same table:

```sprf
rule(dep_source) { fs(**/package.json) > json({ dependencies: { $DEP: $_ } }) };
rule(dep_source) { fs(**/Cargo.toml) > json({ dependencies: { $DEP: $_ } }) };
```

**Namespaces** -- each `.sprf` file's stem becomes a table prefix. `infra.sprf` containing `rule(image)` produces `infra__image_data`.

### Selector tags

| Tag | What it does |
|---|---|
| `fs(glob)` | Match files by path glob |
| `json(pattern)` | Walk JSON/YAML/TOML with destructuring and captures |
| `ast(pattern)` or `ast[lang](pattern)` | Structural match via ast-grep (any tree-sitter language) |
| `line(pattern)` | Line-based regex or segment capture |
| `repo(pattern)` | Match/capture repo name; triggers demand scanning |
| `rev(pattern)` / `branch()` / `tag()` | Match/capture git ref; triggers demand scanning |
| `folder(pattern)` / `file(pattern)` | Match directory or full file path |

---

## Pattern matching

sprefa has three pattern systems. Which one fires depends on the pattern string:

### 1. Segment captures (patterns containing `$`)

Segment patterns split strings on natural boundaries (separators like `/`, `.`, `:`, `-`) and bind captures.

| Syntax | Matches | Example |
|---|---|---|
| `$NAME` | One segment (stops at separator) | `$ORG/$REPO` matches `myco/backend` -> {ORG: myco, REPO: backend} |
| `$$$NAME` | Zero or more chars including separators | `$$$PATH/$FILE` matches `src/lib/utils.ts` -> {PATH: src/lib, FILE: utils.ts} |
| `${NAME}` | Braced capture (adjacent to identifiers) | `use${ENTITY}Query` matches `useUserQuery` -> {ENTITY: User} |
| `$_` | One segment, bind nothing | `$NAME: $_` matches any key-value pair |
| `$$$_` | Multi-segment wildcard | `FROM $$$_.$TABLE` matches `FROM public.users` |
| `literal` | Exact text | `/`, `.`, `-`, any fixed string |

**Pre-bound captures act as constraints.** If `$REPO` is already bound from a cross-rule reference, the segment matcher checks that the value matches rather than capturing a new one. This is how cross-rule filtering works -- upstream bindings constrain downstream patterns.

### 2. Regex patterns (patterns starting with `re:`)

```sprf
# regex on a json key name
json({ re:^(dev-)?dependencies: { $NAME: $_ } })

# regex in a line matcher
line(FROM\s+$IMAGE:$TAG)
```

The `re:` prefix triggers regex mode. Named groups `(?P<name>...)` become captures. In json key matchers, the regex tests the key string. In line matchers, the regex tests each line of the file.

### 3. Glob patterns (everything else)

```sprf
fs(**/Cargo.toml)
fs(**/*.{ts,tsx})
repo(myorg/*)
rev(main|release/*)
```

Standard glob syntax. Pipe `|` splits into multiple alternatives (e.g. `main|develop` matches either branch).

### How the three interact

In a json() body:
- Key matchers: `re:` prefix -> regex, `$VAR` -> segment capture, literal -> exact match
- Value matchers: `$VAR` -> capture, `$_` -> wildcard, `{ }` -> descend, `[...]` -> iterate

In a line() body:
- If pattern contains `$` -> segment capture against each line (regex parts still work: `FROM\s+$IMAGE:$TAG` is a hybrid)
- Pure `re:` prefix -> regex with named groups

In fs/repo/rev/folder/file:
- Glob matching with pipe alternatives

### Extract-time constants

Pre-bound captures for constraining patterns to the current extraction context:

| Constant | Value |
|---|---|
| `$currentRepo` | Current repo name |
| `$currentRev` | Current branch or tag |
| `$currentFile` | File path being extracted |
| `$currentDir` | Parent directory |
| `$currentStem` | Filename stem (extension stripped) |
| `$currentExt` | File extension |

These use camelCase intentionally -- the capture collector only picks up SCREAMING_CASE for rule columns, so these are invisible to table creation but still act as constraints during matching.

---

## json() pattern syntax

| Syntax | Meaning |
|---|---|
| `{ key: pat }` | Match key, descend into value |
| `{ $KEY: $VAL }` | Iterate all keys, capture each pair |
| `{ re:^pattern: $V }` | Regex on key name |
| `{ **: pat }` | Recursive descent (any nesting depth) |
| `[...$ITEM]` | Array iteration with capture |
| `$NAME` | Capture leaf value |
| `$_` | Wildcard, bind nothing |

Works on JSON, YAML, and TOML. All three parse into the same tree structure.

**Recursive descent** (`**`) is useful for deeply nested configs:

```sprf
# find image refs anywhere in a helm values.yaml
fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } })
```

---

## ast() -- extending ast-grep

sprefa wraps ast-grep-core for structural pattern matching on any language with a tree-sitter grammar.

```sprf
# basic: capture env var access in TypeScript
fs(**/*.ts) > ast(process.env.$NAME);

# with language override
fs(**/*.rs) > ast[rust](fn $NAME($$$ARGS) -> $RET { $$$BODY });

# multi-node capture
fs(**/*.tsx) > ast[tsx](import { $$$NAMES } from "$PATH");
```

**Language detection**: inferred from file extension, or override with `ast[lang](...)`. Supported: anything tree-sitter supports (ts, tsx, js, jsx, rs, py, go, java, c, cpp, etc).

**Capture syntax** follows ast-grep conventions:
- `$NAME` -- single node capture
- `$$$NAME` -- multi-node capture (zero or more nodes)

**Segment captures on metavars**: after ast-grep matches, sprefa can apply segment patterns to the matched text for finer extraction:

```sprf
# ast-grep captures "useUserQuery", segment pattern extracts "User"
fs(**/*.ts) > ast(use${ENTITY}Query)
```

---

## line() -- regex and segment matching on file lines

```sprf
# segment capture: split on natural boundaries
rule(dockerfile_from) {
  fs(**/Dockerfile) > line(FROM\s+$IMAGE:$TAG)
};

# cross-language column tracking
rule(py_orm_ref) {
  ddl_table(table: $TABLE) {
    fs(**/*.py) > line(__tablename__\s*=\s*['"]$TABLE['"])
  }
};
```

Line matchers test each line of the file against the pattern. `$VAR` captures use segment matching (boundary-aware). Regex escapes like `\s+` work alongside segment captures -- the pattern is a hybrid.

---

## SQLite schema

### Per-rule tables

Every rule (built-in or user-defined) gets its own table:

```
{rule_name}_data          -- raw data (foreign keys to strings/refs)
{rule_name}               -- view: joins strings for human-readable values
{rule_name}_refs          -- view: adds byte spans and node paths for provenance
```

With namespaces: `{namespace}__{rule_name}_data`, etc.

**Data table columns:**

```sql
CREATE TABLE "deploy_image_data" (
  id INTEGER PRIMARY KEY,
  "repo_ref" INTEGER,    -- refs.id (provenance: where in the file)
  "repo_str" INTEGER,    -- strings.id (the captured value)
  "tag_ref" INTEGER,
  "tag_str" INTEGER,
  repo_id INTEGER,        -- which repo
  file_id INTEGER,        -- which file
  rev TEXT                 -- which branch/tag
)
```

**Main view** (join strings for readable values):

```sql
SELECT t.id,
  s0.value AS "repo", s0.norm AS "repo_norm",
  s1.value AS "tag", s1.norm AS "tag_norm",
  t.repo_id, t.file_id, t.rev
FROM "deploy_image_data" t
LEFT JOIN strings s0 ON t."repo_str" = s0.id
LEFT JOIN strings s1 ON t."tag_str" = s1.id
```

**Refs view** (adds byte-level provenance):

```sql
-- same as main view, plus:
  r0.span_start AS "repo_span_start",
  r0.span_end AS "repo_span_end",
  r0.node_path AS "repo_node_path"
```

### Built-in extractor tables

These are created automatically from the JS/TS and Rust extractors:

| Table | Extractor | What it captures |
|---|---|---|
| `import_path` | JS/TS (oxc) | import/require module specifiers |
| `import_name` | JS/TS (oxc) | named import bindings |
| `import_alias` | JS/TS (oxc) | aliased imports |
| `export_name` | JS/TS (oxc) | exported identifiers |
| `export_local_binding` | JS/TS (oxc) | internal names when different from export |
| `dep_name` | JS/TS (oxc) + Rust (syn) | dependency names |
| `dep_version` | JS/TS (oxc) | dependency version specifiers |
| `rs_use` | Rust (syn) | use paths (crate/self/super resolved) |
| `rs_declare` | Rust (syn) | fn/struct/enum/trait/impl declarations |
| `rs_mod` | Rust (syn) | module declarations |

### Base tables

```
repos           -- registered repositories
files           -- indexed files (path, content_hash, per repo)
strings         -- deduplicated string values (value, norm, norm2)
refs            -- every extracted string occurrence (file, span, string)
rev_files       -- which files belong to which revision
repo_revs       -- known revisions per repo (branches, tags)
sprf_meta       -- rule hash tracking for incremental re-extraction
```

### SQL UDFs (custom functions)

Available in `sprefa sql`, `query()`, and `check()` blocks:

| Function | Signature | What it does |
|---|---|---|
| `re_extract(text, pattern, group)` | `(TEXT, TEXT, INT) -> TEXT` | Extract regex capture group. Group 0 = full match. Compiled regex cached per query. |
| `split_part(text, delim, n)` | `(TEXT, TEXT, INT) -> TEXT` | Nth part of delimited string (1-indexed, like PostgreSQL). |
| `repo_name(repo_id)` | `(INT) -> TEXT` | Lookup repo name by ID. |
| `file_path(file_id)` | `(INT) -> TEXT` | Lookup file path by ID. |
| `fzy_score(haystack, needle)` | `(TEXT, TEXT) -> REAL` | Fuzzy subsequence match score (0.0-1.0). Case-insensitive, rewards contiguous matches. |

**Built-in views**: `repo_tags` (semver-tagged revisions), `repo_branches` (branch revisions).

---

## query -- SQL over extracted data

```sprf
query(transitive_dep) {
  SELECT a.dep, c.dep
  FROM dep_to_package_data a
  JOIN package_has_dep_data b ON a.pkg = b.pkg
  JOIN dep_to_package_data c ON b.dep = c.dep
};
```

Query bodies are raw SQL SELECT statements. They run against the per-rule tables and views. Use UDFs for string manipulation:

```sprf
query(semver_major) {
  SELECT value, split_part(value, '.', 1) AS major
  FROM deploy_image
  WHERE re_extract(value, 'v(\d+)\.\d+\.\d+', 1) IS NOT NULL
};
```

## check -- CI-friendly invariant verification

Rows returned = violations. Exit code 1.

```sprf
check(missing_tag) {
  SELECT dc.repo, dc.tag
  FROM deploy_image dc
  LEFT JOIN repo_tags rt ON rt.repo_id = dc.repo_id AND rt.rev = dc.tag
  WHERE rt.rev IS NULL
};
```

```bash
sprefa check                       # run all checks
sprefa check missing_tag           # run one check
sprefa check --list                # show stored violations
```

---

## How it works

```
git ls-files
  -> parallel rayon walk (content hash dedup, skip set)
  -> per-file extraction (JS/TS via oxc, Rust via syn, structured data via rule engine, AST via ast-grep)
  -> bulk flush to per-rule SQLite tables (dedup strings, chunked inserts)
  -> resolve JS/TS import targets (oxc_resolver with tsconfig support)
  -> demand scanning (repo/rev captures trigger recursive discovery until stable)
```

### Demand scanning

When a rule captures `repo($REPO)` and `rev($TAG)`, the scan pipeline extracts those values, checks out the referenced repo at that tag, scans it, and repeats until all (repo, rev) pairs are scanned. Fixed-point iteration, max 10 rounds.

A deploy values.yaml referencing `repository: myorg/backend, tag: v2.1.0` automatically triggers scanning of `myorg/backend@v2.1.0`, which might reference more repos, continuing the chain.

### Incremental scanning

- Files are tracked by content hash; unchanged files are skipped
- Rules are tracked by schema hash + extract hash; changed rules trigger table rebuild
- `scan_diff(old_sha)` only re-extracts files changed since a git commit

---

## CLI reference

```
sprefa init                              # create sprefa.toml + SQLite DB
sprefa add <path> [--name <name>]        # register a repo
sprefa scan [--repo <name>] [--once]     # index repos
sprefa daemon [--repo <name>]            # scan + watch + serve (all-in-one)
sprefa watch [--repo <name>]             # filesystem watcher, auto-rewrite
sprefa serve                             # HTTP server (127.0.0.1:9400)
sprefa query <term> [--scope committed|local|all]  # trigram substring search
sprefa check [name] [--list]             # run/list invariant checks
sprefa sql "<SELECT ...>"                # read-only SQL against the index
sprefa eval '<rule>' [files...]          # one-shot extraction, standalone
sprefa status                            # show indexed repos
sprefa config                            # print resolved config
sprefa reset                             # drop + reinit DB
```

## Watch + rewrite

The watcher detects file moves (content hash correlation within 100ms) and declaration renames (span proximity diffing within 64 bytes), then rewrites all affected import/use statements across the index.

| Event | JS/TS | Rust |
|---|---|---|
| File move | Rewrite import/require/export paths | Rewrite `use` statements |
| Declaration rename | Rewrite `import { OldName }` | Rewrite `use crate::mod::OldName` |
| File delete | Log broken references | Same |

Rust module mapping: `src/lib.rs` -> `crate`, `src/foo/bar.rs` -> `crate::foo::bar`. All prefix styles (`crate::`, `self::`, `super::`, chained `super::super::`) resolve correctly.

## Built-in extractors

| Extractor | Parser | Files | Extracts |
|---|---|---|---|
| JS/TS | oxc | .js .jsx .ts .tsx .mjs .cjs .mts .cts | imports, exports, require(), re-exports |
| Rust | syn | .rs | use paths (crate/self/super), declarations, mod, extern crate |
| Rule engine | serde | JSON, YAML, TOML | configurable tree walks via .sprf rules |
| ast-grep | ast-grep-core | anything with a tree-sitter grammar | structural pattern matching |

## Config

```toml
[db]
path = "~/.sprefa/index.db"

[daemon]
bind = "127.0.0.1:9400"

[scan.normalize]
strip_suffixes = ["-service", "-api", "-v2", "-client", "-server"]

[[sources]]
root = "~/checkouts"
layout = "{org}/{branch}/{repo}"

[[repos]]
name = "my-frontend"
path = "/home/me/repos/my-frontend"
revs = ["main"]

[filter]
mode = "exclude"
exclude = ["node_modules/**", "vendor/**", "dist/**", "target/**", ".git/**"]
```

Config resolution: `$SPREFA_CONFIG` > `./sprefa.toml` > `~/.config/sprefa/sprefa.toml`.

## ghcache integration

With `[ghcache]` configured, `sprefa daemon` subscribes to checkout events from [ghcacher](../ghcacher) and auto-scans repos as they appear on disk.

```toml
[ghcache]
db = "~/.ghcache/ghcache.db"
poll_ms = 500
```

## Workspace

```
crates/
  cli/        clap CLI, eval subcommand
  config/     TOML loading, filtering, source discovery
  schema/     SQLite migrations, per-rule table DDL, UDFs
  extract/    Extractor trait + RawRef
  index/      parallel file walk, xxh3 hashing
  cache/      bulk flush, import resolution, demand scanning
  rules/      rule engine: tree walker, pattern matcher, emitter
  sprf/       .sprf parser: text -> AST -> RuleSet
  js/         oxc JS/TS extractor
  rs/         syn Rust extractor
  watch/      filesystem watcher + rewrite pipeline
  scan/       Scanner coordinator
  server/     axum HTTP server
  sprf-lsp/   LSP server for .sprf files
editors/
  vscode/     tmLanguage syntax highlighting
```

---

## Security and I/O boundaries

### What runs where

```
┌─────────────────────────────────────────────────────┐
│ Local machine only                                   │
│                                                      │
│  sprefa CLI ──▶ SQLite (~/.sprefa/index.db)          │
│       │                                              │
│       ├──▶ git2 (read-only repo access)              │
│       ├──▶ fs notify (file watcher)                  │
│       └──▶ HTTP 127.0.0.1:9400 (optional daemon)    │
│                                                      │
│  No data leaves the machine.                         │
│  No cloud services. No telemetry. No auth tokens.    │
└─────────────────────────────────────────────────────┘
```

sprefa is a local-only tool. All data stays on disk in a SQLite database. The HTTP server binds to loopback (`127.0.0.1`) by default. The CLI talks to the daemon over localhost when configured, otherwise operates directly on the database file.

### Network exposure

| Component | Binds to | Purpose |
|---|---|---|
| `sprefa serve` / `sprefa daemon` | `127.0.0.1:9400` (configurable) | JSON API for query, scan, status |
| `sprefa` CLI (reqwest) | outbound to `127.0.0.1` | POST /scan, GET /query to local daemon |
| sprf-lsp | stdin/stdout | LSP protocol over pipes, no sockets |

The daemon address is configurable via `[daemon].bind` in `sprefa.toml`. reqwest is compiled without default TLS features (localhost only). No external HTTP requests are made.

### Filesystem access

| Operation | Paths | Read/Write |
|---|---|---|
| Config loading | `$SPREFA_CONFIG`, `./sprefa.toml`, `~/.config/sprefa/sprefa.toml` | Read |
| Database | `~/.sprefa/index.db` (configurable) | Read/Write |
| Repository indexing | Registered repo paths | Read |
| File watcher | Registered repo paths | Read (watch), Write (rewrite imports on rename) |
| Git access | `.git/` in registered repos | Read (via libgit2) |

The watcher writes back to source files only during auto-rewrite of import/use paths after detecting a rename. All other filesystem access is read-only.

### Database

Single SQLite file in WAL mode. Tables store:
- File paths and content hashes (deduplicated)
- Extracted string values and byte-level provenance (spans)
- Per-rule data tables (one per .sprf rule)
- Rule schema hashes for incremental re-extraction

No PII, credentials, or secrets are stored. The database contains structural metadata about code: import paths, dependency names, config values, and cross-references.

### Unsafe code

Three categories, all confined:

| Where | What | Why |
|---|---|---|
| `memmap2::Mmap::map()` (3 sites) | Memory-mapped file reads | Fast content hashing during indexing and move detection |
| `schema/udfs.rs` | SQLite UDF registration via `libsqlite3_sys` FFI | Custom SQL functions: `re_extract`, `split_part`, `repo_name`, `file_path`, `fzy_score` |
| `schema/migrations.rs` | `register_all(handle.as_raw_handle().as_ptr())` | Passes raw SQLite handle to UDF registration |

No other unsafe code exists in the codebase.

### Dependency profile

All dependencies are well-known, audited crates from established ecosystems.

**Code analysis** (read-only, no network):
- `ast-grep-core` / `ast-grep-language` / `ast-grep-config` -- structural pattern matching via tree-sitter
- `oxc_parser` / `oxc_ast` / `oxc_resolver` -- JS/TS parsing and module resolution
- `syn` / `proc-macro2` -- Rust syntax parsing
- `winnow` -- parser combinators for .sprf

**Data handling** (no network):
- `serde` / `serde_json` / `serde_yaml` / `toml` -- serialization
- `sqlx` (SQLite runtime) / `libsqlite3-sys` -- database
- `globset` / `regex` -- pattern matching
- `xxhash-rust` / `sha2` / `sha1` -- content hashing

**Filesystem** (no network):
- `walkdir` -- directory traversal
- `notify` -- filesystem event monitoring
- `memmap2` -- memory-mapped file reads
- `git2` (compiled without default features) -- repository access via libgit2

**Runtime + server** (localhost only):
- `tokio` -- async runtime
- `axum` -- HTTP framework (loopback binding)
- `reqwest` (no default TLS) -- HTTP client for localhost daemon communication
- `tower-lsp` -- LSP server over stdio

**CLI + logging**:
- `clap` -- argument parsing
- `tracing` / `tracing-subscriber` -- structured logging to stderr
- `anyhow` / `thiserror` -- error handling

**Optional**:
- `ghcache-client` (behind `ghcache` feature flag) -- subscribes to local checkout events from a sibling tool. Reads from a local SQLite database, no network.

### Build-time

One `build.rs` in `crates/scan/`: runs `git rev-parse HEAD` to embed the commit hash for change detection. No other build scripts, proc macros with side effects, or code generation.
