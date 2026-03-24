# sprefa - (s)u(p)er(refa)ctor

Generic cross-repo code intelligence indexer. Parses source files from N git repos into a single SQLite DB, extracts every interesting string as a ref with byte-level spans, normalizes for fuzzy matching, and resolves cross-file/cross-repo imports. The schema is language-agnostic; language-specific extraction happens in a pluggable scan layer.

The system operates in two modes sharing the same transaction logic:
- **CLI**: one-shot index run, direct SQLite access
- **Daemon**: HTTP server wrapping the same operations, CLI becomes a client

---

## Phase 1: Config + CLI/Server Foundation

### Config (`sprefa.toml`)

```toml
[db]
path = "~/.sprefa/index.db"        # where the SQLite DB lives

[daemon]
# url = "http://localhost:9400"     # if set, CLI delegates to daemon
bind = "127.0.0.1:9400"
# auto_start = true                 # CLI starts daemon if not running

[scan]
# workers = 4                       # parallel scan threads

# norm2 suffix stripping rules
[scan.normalize]
strip_suffixes = ["-service", "-api", "-v2", "-client", "-server"]

# per-repo config
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
include = ["src/**", "config/**"]   # LTS branch: only care about src + config

# global file filtering - applied to all repos
[filter]
mode = "exclude"                    # "exclude" or "include"

# glob patterns
exclude = [
  "node_modules/**",
  "vendor/**",
  "dist/**",
  "target/**",
  ".git/**",
  "*.min.js",
  "*.lock",
  "*.map",
]

# include = ["src/**", "lib/**"]    # only used when mode = "include"
```

Config loading priority: `$SPREFA_CONFIG` env var > `./sprefa.toml` > `~/.config/sprefa/sprefa.toml`.

Filter resolution: global -> per-repo -> per-branch. Most specific wins.

### CLI (clap)

```
sprefa init                          # create default config + DB
sprefa add <path> [--name <name>]    # add repo to config
sprefa scan [--repo <name>]          # index repos (all or specific)
sprefa query <term>                  # search strings table (FTS5)
sprefa serve                         # start daemon
sprefa status                        # show indexed repos, file counts
```

When daemon URL is configured, `scan`/`query`/`status` become HTTP calls. Otherwise, direct SQLite.

### Server (axum)

```
POST /scan          { repo?: string }
GET  /query?q=term
GET  /status
GET  /repos
POST /repos         { name, path, branches? }
```

Same functions as CLI, different transport.

### Workspace Layout

```
sprefa/
  Cargo.toml              # workspace root
  crates/
    0_config/             # config types, loading, filtering
    1_schema/             # DB types, migrations, sqlx queries
    2_extract/            # extractor trait + implementations
    3_scan/               # orchestrates git + extraction + DB writes
    4_server/             # axum HTTP server (daemon mode)
    5_cli/                # clap CLI
```

---

## Phase 1 Build Order

1. **`0_config`** - config structs, TOML parsing, filter glob matching, config file discovery
2. **`1_schema`** - migrations, types, query functions, DB init
3. **`5_cli`** - skeleton with `init`, `add`, `status` subcommands
4. **`4_server`** - axum skeleton with `/status` endpoint
5. Wire CLI to detect daemon and delegate

---

## Schema (SQLite + sqlx)

Core model: `repos 1->M files 1->M refs M<-1 strings`

Additional edges: `ref.target_file_id -> files` (resolved cross-file link), `refs.parent_key_string_id -> strings` (key-value pairings)

**repos** - git repositories being tracked
**files** - every file from git ls-files, content-addressed (same path + different content across branches = separate rows)
**strings** - deduplicated string values with normalized forms + FTS5 trigram index
**refs** - file X contains string Y at byte offset Z, classified by RefKind
**branch_files** - junction for multi-branch indexing (1 file row, N branch rows)
**repo_branches** - git hash per repo+branch for incremental sync
**git_tags** - tag tracking with semver detection
**repo_packages** - dependency manifest tracking (npm, cargo, pip, etc.)

### RefKind enum

```
StringLiteral, JsonKey, JsonValue, YamlKey, YamlValue, TomlKey, TomlValue,
ImportPath, ImportName, ExportName, DepName, DepVersion,
RsUse, RsDeclare, RsMod
```

### String normalization

- `norm`: strip non-alphanumeric, lowercase. `my-UI` -> `myui`
- `norm2`: additional suffix stripping via `[scan.normalize].strip_suffixes`

---

## Phase 2: Extractors + Scan

### Parser Strategy (layered, minimize dynamic deps)

| Priority | Parser | Languages | Notes |
|----------|--------|-----------|-------|
| 1 | oxc_parser | JS, TS, JSX, TSX | Pure Rust, fast, MVP language |
| 2 | syn/ra_ap_syntax | Rust | Pure Rust |
| 3 | Custom | JSON, YAML, TOML | serde_json, serde_yaml, toml. Lodash-style dot paths |
| 4 | ast-grep (lib) | Everything else | Catchall, linked as Rust lib |

JSON/YAML path encoding: `{"a": {"b": "c"}}` produces ref `a.b` (kind: JsonKey) and ref `c` (kind: JsonValue, parent_key: `a.b`). Every nested key becomes a dotted path string in the strings table.

### Rust Module Resolution

Four-step process:

**Step 1: Find crate roots.** Locate each `Cargo.toml`. Parent dir = crate root. `src/lib.rs` or `src/main.rs` maps to `crate::`. Crate name from `[package].name` (with `-` normalized to `_`).

**Step 2: Build module path map.** Walk `.rs` files, derive module path from filesystem position:
- `src/foo.rs` -> `crate::foo`
- `src/foo/mod.rs` -> `crate::foo`
- `src/foo/bar.rs` -> `crate::foo::bar`

Result: `HashMap<(RepoId, String), FileId>`

**Step 3: Resolve use paths with tail stripping.** For `use crate::x::y::z`:
- Try exact match in map, then strip tail segments until hit
- Trailing stripped segments are symbols within the matched module
- `super::x` and `self::x`: rewrite relative to source file's own module path, then same lookup

**Step 4: Cross-crate resolution.** For `tokio::runtime::Runtime`:
- `repo_packages` maps crate name -> repo_id
- Replace first segment with `crate::`, look up in that repo's module map
- Cargo normalizes `-` to `_` (`my-lib` -> `my_lib`)

---

## Testing

All tests use `insta` for snapshot assertions.

- Config parsing: snapshot deserialized struct from various TOML inputs
- Filter matching: snapshot which files pass/fail given glob patterns
- Schema migrations: snapshot DB schema after running all migrations
- Extractors (Phase 2): snapshot `Vec<RawRef>` output for fixture files
- Resolution (Phase 2): snapshot resolved `target_file_id` mappings
