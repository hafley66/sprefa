# sprefa - (s)u(p)er(refa)ctor

Rename a symbol, every reference updates. Across files, across repos, across languages. No LLM, no datacenter -- a pre-built index and a graph traversal.

sprefa is a daemon that watches project folders, maintains a SQLite index of every interesting string in every source file (imports, exports, config keys, YAML values, dependency names), and performs instant deterministic rename propagation when you change something. The index makes renames O(lookup) instead of O(parse-everything).

## How it works

```
scan files -> extract refs -> index in SQLite -> watch -> detect change -> plan rewrites -> apply
```

Every interesting string in a codebase is a **ref**: a file contains a string at a byte offset. Semantic interpretation lives on **matches**: each ref can have multiple matches from different extraction rules, each with a kind string and rule name. The string is deduplicated and normalized for fuzzy matching. Refs link files to strings. Resolved imports link refs to target files. That's the whole model.

```
repos 1->M files 1->M refs M<-1 strings
                        |
                    matches (kind TEXT, rule_name TEXT)
                        |
                    match_labels (key-value metadata)

ref.target_file_id -> files         (resolved cross-file link)
refs.parent_key_string_id -> strings (key-value pairings)
```

When something changes, the watcher classifies the event (file move, declaration rename, delete), queries the index for every ref affected, computes new values using language-specific path rewriters, and applies the edits to disk.

## Quick start

```bash
sprefa init                          # create sprefa.toml + SQLite DB
sprefa add /path/to/repo             # register a repo
sprefa daemon                       # scan + watch + serve, all in one
```

One command does everything: scans all repos to build the index, starts filesystem watchers for auto-rewrite, and runs the HTTP server for queries. Move or rename files freely.

## What the watcher handles

| Event | Detection | JS/TS rewrite | Rust rewrite |
|-------|-----------|---------------|--------------|
| **File move** | Rename events (macOS `Modify(Name)`) or delete+create pairs correlated by content hash within 100ms window | Rewrite all `import`/`require`/`export...from` paths targeting the moved file. Preserves extension convention and index-file stripping. | Rewrite all `use` statements referencing the old module path. Preserves `crate::`, `self::`, `super::` prefix style. |
| **Declaration rename** | Re-extract the changed file, diff declarations by span proximity (within 64 bytes = same declaration, different name) | Rewrite all `import { OldName }` to `import { NewName }` in files importing from the source. | Rewrite all `use crate::mod::OldName` to `use crate::mod::NewName`. |
| **File delete** | Delete with no matching create | Log warning with count of now-broken references. | Same. |
| **New file** | Create with no matching delete | Indexed on next scan. | Same. |

### How file-to-module mapping works (Rust)

sprefa converts file paths to module paths using directory structure:

```
src/lib.rs         -> crate
src/main.rs        -> crate
src/utils.rs       -> crate::utils
src/foo/mod.rs     -> crate::foo
src/foo/bar.rs     -> crate::foo::bar
```

When `src/utils.rs` moves to `src/helpers/utils.rs`, every `use crate::utils::Foo` becomes `use crate::helpers::utils::Foo`. If the importing file used `super::utils::Foo` and the new path is still expressible as `super::`, the prefix is preserved. All prefix styles (`crate::`, `self::`, `super::`, including chained `super::super::`) are resolved to absolute module paths at query time, so they are all caught by file moves and declaration renames.

## Why this doesn't already exist

Plenty of tools do code intelligence for a single language (rust-analyzer, tsserver, gopls). They all stop at one of three walls:

1. **Single-language.** Your TS frontend imports a string that matches a Go service name in a K8s manifest that references a Helm value from a TOML config. No single-language tool sees the full chain.

2. **Build-system coupling.** SCIP indexers and rust-analyzer require a successful build. If the project doesn't compile, or you're looking at 500 repos and can't build all of them, you get nothing.

3. **Precision religion.** IDE tooling won't ship anything less than 100% semantic precision. But most renames are unambiguous string matches within a known module graph. You don't need full type inference to propagate `UserService` through `import { UserService } from './user-service'`.

sprefa operates at the **string + module graph** level. Normalized strings in SQLite with byte spans, module-aware resolution for languages that have it, honest confidence scoring instead of pretending to be a compiler. Fast enough to run as a daemon on a laptop.

## CLI

```
sprefa init                          # create sprefa.toml + DB
sprefa add <path> [--name <name>]    # register a repo
sprefa daemon [--repo <name>]        # scan + watch + serve, all in one
       [--no-scan]                   # skip initial scan (index already populated)
sprefa scan [--repo <name>] [--once] # index repos only
sprefa watch [--repo <name>]         # watch and auto-rewrite only
sprefa serve                         # HTTP server only (127.0.0.1:9400)
sprefa query <term> [--once]         # trigram substring search
sprefa sql "<SELECT ...>"            # read-only SQL against the index DB
sprefa status                        # show indexed repos
sprefa --readme                      # print this document
sprefa --json <command>              # structured JSON logs (all commands)
```

### Modes of operation

**`sprefa daemon`** is the recommended way to run sprefa. It runs the full pipeline in sequence:

1. Initial scan of all registered repos (builds the index)
2. Start filesystem watchers on all repos (auto-rewrite on changes)
3. Start the HTTP server (queries, status, trigger re-scans)

Use `--no-scan` to skip step 1 if the index is already populated. Use `--repo` to limit to a single repo.

The individual pieces are also available as separate commands for flexibility:

**`sprefa scan`** -- one-shot indexing. Builds/updates the index and exits. Re-run after large branch switches or merges.

**`sprefa watch`** -- filesystem watching only, no HTTP server, no initial scan. Requires a prior `sprefa scan` to populate the index.

**`sprefa serve`** -- HTTP server only, no watching, no scanning. When `[daemon].url` is set in config, CLI commands (`scan`, `query`) delegate to the daemon over HTTP.

### Direct SQL access

```bash
sprefa sql "SELECT COUNT(*) FROM refs"
sprefa sql "SELECT s.value, m.kind, m.rule_name
            FROM strings s
            JOIN refs r ON r.string_id = s.id
            JOIN matches m ON m.ref_id = r.id
            LIMIT 20"
```

Opens the index DB (resolved from config) and runs the query. Only SELECT, WITH, EXPLAIN, and PRAGMA are allowed -- DML is blocked. Output is tab-separated with a header row. The database is the query language; this command just removes the need to find the file path.

### Structured logging

All commands support `--json` for structured JSON log output. Each line is a JSON object with timestamp, level, target, span context, and structured fields including `phase`, `repo`, `change_count`, `edit_count`, etc.

```bash
sprefa daemon --json                 # JSON logs for process managers
sprefa daemon --json 2>&1 | jq .     # pretty-print
RUST_LOG=sprefa=debug sprefa daemon  # verbose human-readable
RUST_LOG=sprefa=trace sprefa daemon  # everything
```

Phases logged: `initial_scan`, `initial_scan_complete`, `watcher_started`, `server_starting`, `changes_detected`, `change_detail`, `plan_complete`, `edit_detail`, `rewrite_applied`, `rewrite_failed`, `lock_acquire`, `lock_acquired`, `lock_timeout`.

## Config (`sprefa.toml`)

```toml
[db]
path = "~/.sprefa/index.db"

[daemon]
bind = "127.0.0.1:9400"
# url = "http://localhost:9400"     # if set, CLI delegates to daemon

[scan]
# workers = 4

[scan.normalize]
strip_suffixes = ["-service", "-api", "-v2", "-client", "-server"]

# Auto-discover repos from a checkout root managed by an external tool.
# sprefa does NOT clone or fetch -- it only reads what's on disk.
[[sources]]
root = "~/checkouts"
layout = "{org}/{branch}/{repo}"    # -> ~/checkouts/acme/main/frontend/
# default_org = "myco"
# default_branch = "main"

# Explicit repo entries (in addition to discovered sources)
[[repos]]
name = "my-frontend"
path = "/home/me/repos/my-frontend"
branches = ["main"]

[[repos]]
name = "my-backend"
path = "/home/me/repos/my-backend"
branches = ["main", "release/v3"]

# per-branch overrides
[[repos.branch_overrides]]
branch = "release/v3"
[repos.branch_overrides.filter]
mode = "include"
include = ["src/**", "config/**"]

# global file filtering
[filter]
mode = "exclude"
exclude = [
  "node_modules/**", "vendor/**", "dist/**", "target/**",
  ".git/**", "*.min.js", "*.lock", "*.map",
]
```

Config loading: `$SPREFA_CONFIG` > `./sprefa.toml` > `~/.config/sprefa/sprefa.toml`.

Filter resolution: global -> per-repo -> per-branch. Most specific wins.

## Architecture

### Extraction pipeline (scan)

```
git ls-files
  -> parallel rayon walk (content hash, skip set check)
  -> per-file extraction (JS extractor, Rust extractor, rule extractor)
  -> bulk flush to SQLite (dedup strings, chunk inserts)
  -> resolve import targets (oxc_resolver with tsconfig support, bare specifier fallback)
```

### Watch pipeline (watch)

```
notify OS events (Create, Remove, Modify(Name) for renames on macOS)
  -> debounce 100ms batches
  -> classify: correlate delete+create by content hash -> Move
               Modify(Name(From/To/Both)) -> split into Removed+Created for move detection
               re-extract modified files, diff by span proximity -> DeclChange
  -> plan_rewrites: query index for affected refs, compute new values
  -> apply: splice edits into source files (descending offset order)
```

### Extractors

| Extractor | Languages | What it extracts |
|-----------|-----------|------------------|
| **JsExtractor** (oxc) | .js, .jsx, .ts, .tsx, .mjs, .cjs, .mts, .cts | ImportPath, ImportName, ImportAlias, ExportName, ExportLocalBinding, require() calls |
| **RsExtractor** (syn) | .rs | RsUse (full paths, flattened from use-trees), RsDeclare (fn, struct, enum, trait, impl items, type, const, static), RsMod, DepName (extern crate) |
| **RuleExtractor** (JSON/YAML rules) | Any structured format | Configurable: JSON keys/values, YAML keys/values, TOML keys/values, dependency names/versions. Rules define tree-walking patterns with captures and emit actions. |

### Path rewriters

| Rewriter | When triggered | What it does |
|----------|---------------|--------------|
| **JsPathRewriter** | .js/.ts file is the source of an ImportPath ref | Computes new relative path from importing file to moved target. Matches original extension convention (keep/strip). Strips `/index` for directory imports. Ensures `./` prefix. |
| **RsPathRewriter** | .rs file move or RsDeclare rename | Converts file paths to module paths (`src/foo/bar.rs` -> `crate::foo::bar`). Replaces old module path prefix with new in use statements. Preserves `crate::`/`self::`/`super::` prefix style. |

### Import resolution

Import targets are resolved using oxc_resolver (v11), which handles:
- Relative paths with extension probing (.ts, .tsx, .js, .jsx, etc.)
- tsconfig.json paths, baseUrl, and extends chains
- node_modules resolution
- Bare specifiers fall back to the repo_packages table

## Rule engine

Declarative JSON rules for "how do strings in structured files point to things in other repos." The entire indexed space is a DOM:

```
root
+-- repo[name="org/frontend"][branch="main"]
|   +-- file[path="package.json"][ext="json"]
|   |   \-- (json tree nodes)
|   +-- file[path="values.yaml"][ext="yaml"]
|   |   \-- (yaml tree nodes)
```

Each rule is a CSS-style selector against this DOM with three dimensions:

1. **Git context** -- repo/branch/tag globs (`"repo": "*/helm-charts"`, `"branch": "main|release/*"`)
2. **File path** -- glob on repo-relative path (`"file": "values*.yaml"`)
3. **Structural position** -- step chain that walks the parsed tree depth-first

Structural steps: `key`, `key_match` (glob), `any` (descend arbitrary depth), `depth_min`/`depth_max`/`depth_eq`, `parent_key`, `array_item`, `leaf`, `object` (capture sibling values).

Steps can **capture** values by name as they match. A `value` regex can split/filter captures (e.g. `"express@4.18.2"` into `name` + `version`). The `action.emit` array turns captures into refs with explicit parent linkage for grouped output (dep name + version as linked refs).

```json
{
  "name": "npm-deps",
  "file": "package-lock.json",
  "select": [
    { "step": "key", "name": "dependencies" },
    { "step": "key_match", "pattern": "*", "capture": "name" },
    { "step": "key", "name": "version" },
    { "step": "leaf", "capture": "version" }
  ],
  "action": {
    "emit": [
      { "capture": "name", "kind": "dep_name" },
      { "capture": "version", "kind": "dep_version", "parent": "name" }
    ]
  }
}
```

Rules replace hard-coded Rust for each new file format or naming convention. When the way services reference each other changes, you edit a JSON rule, not source code. JSON Schema is generated from the Rust types for IDE intellisense.

## Schema

**RefKind enum:**
```
StringLiteral, JsonKey, JsonValue, YamlKey, YamlValue, TomlKey, TomlValue,
ImportPath, ImportName, ImportAlias, ExportName, ExportLocalBinding,
DepName, DepVersion,
RsUse, RsDeclare, RsMod
```

**String normalization:**
- `norm`: strip non-alphanumeric, lowercase. `my-UI` -> `myui`
- `norm2`: configurable suffix stripping (`-service`, `-api`, `-v2`)

## Workspace

```
crates/
  cli/          clap CLI (init, add, scan, watch, serve, daemon, status, query)
  config/       config types, TOML loading, filtering, source discovery
  schema/       SQLite migrations, types, query functions
  extract/      Extractor trait + RawRef type
  index/        pure extraction: file enumeration, parallel rayon walk, xxh3 hashing
  cache/        DB writes: bulk flush, import target resolution (oxc_resolver),
                scan context (skip set by content hash + scanner hash)
  rules/        declarative JSON rule engine: types, tree walker, emit, JSON Schema
  js/           oxc-based JS/TS extractor (imports, exports, require, re-exports)
  rs/           syn-based Rust extractor (use trees, declarations, mod, extern crate)
  watch/        filesystem watcher + rewrite pipeline:
                  watcher.rs   - notify v8, debounce, move detection by content hash
                  diff.rs      - declaration diffing by span proximity
                  plan.rs      - rewrite planning (query index, compute edits)
                  rewrite.rs   - edit application (splice by descending offset)
                  js_path.rs   - JS/TS relative path rewriter
                  rs_path.rs   - Rust module path rewriter (crate/self/super)
                  queries.rs   - DB queries for affected refs
                  change.rs    - FsChange + DeclChange types
  scan/         coordinator: Scanner struct, spawn_blocking bridge, scan_repo
  server/       axum HTTP daemon (/status, /repos, /query, /scan)
```

## Parser strategy

| Priority | Parser | Languages |
|----------|--------|-----------|
| 1 | oxc_parser | JS, TS, JSX, TSX, MJS, CJS, MTS, CTS |
| 2 | syn | Rust |
| 3 | Declarative rules | JSON, YAML, TOML (any structured format) |
| 4 | SCIP consumption | Any language with a SCIP indexer (Go, Java, Python, etc.) |
| 5 | ast-grep (lib) | Everything else |

## Testing

180+ tests using `insta` snapshot assertions and sqlx in-memory SQLite. Coverage spans: config parsing, filter resolution, source discovery, rule deserialization + tree walking + ref emission, cross-format dep extraction (package-lock, pnpm-lock), bulk flush correctness (dedup, idempotency, chunk boundaries), scan context skip set loading and invalidation, import target resolution (relative, tsconfig paths, bare specifiers, directory indexes), JS/TS extraction (all import/export variants including side-effect, type-only, dynamic exclusion, namespace re-export), Rust extraction (use trees with nested groups/self/glob/rename, declarations, async fn, pub(crate), impl-for-trait, spans), Rust module path conversion (workspace crates, mod.rs, multiple src/ dirs), JS path rewriting (extension conventions, index stripping, cross-tree monorepo, scoped packages), Rust module path rewriting (double super::, self:: fallback, mod.rs moves, super beyond crate root), declaration diffing (threshold boundaries, cross-kind isolation, swap detection), edit application (grow/shrink, multi-file, boundary edits, missing files).
