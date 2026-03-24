# sprefa - (s)u(p)er(refa)ctor

Rename a symbol, every reference updates. Across files, across repos, across languages. No LLM, no datacenter -- a pre-built index and a graph traversal.

sprefa is a daemon that watches project folders, maintains a SQLite index of every interesting string in every source file (imports, exports, config keys, YAML values, dependency names), and performs instant deterministic rename propagation when you change something. The index makes renames O(lookup) instead of O(parse-everything).

## How it works

```
watch files -> extract refs -> index in SQLite -> rename = lookup + rewrite
```

Every interesting string in a codebase is a **ref**: a file contains a string at a byte offset, classified by kind (import, export, JSON key, YAML value, dependency name, etc.). The string is deduplicated and normalized for fuzzy matching. Refs link files to strings. Resolved imports link refs to target files. That's the whole model.

```
repos 1->M files 1->M refs M<-1 strings
ref.target_file_id -> files         (resolved cross-file link)
refs.parent_key_string_id -> strings (key-value pairings)
```

## Why this doesn't already exist

Plenty of tools do code intelligence for a single language (rust-analyzer, tsserver, gopls). They all stop at one of three walls:

1. **Single-language.** Your TS frontend imports a string that matches a Go service name in a K8s manifest that references a Helm value from a TOML config. No single-language tool sees the full chain.

2. **Build-system coupling.** SCIP indexers and rust-analyzer require a successful build. If the project doesn't compile, or you're looking at 500 repos and can't build all of them, you get nothing.

3. **Precision religion.** IDE tooling won't ship anything less than 100% semantic precision. But most renames are unambiguous string matches within a known module graph. You don't need full type inference to propagate `UserService` through `import { UserService } from './user-service'`.

sprefa operates at the **string + module graph** level. Normalized strings in SQLite with byte spans, module-aware resolution for languages that have it, honest confidence scoring instead of pretending to be a compiler. Fast enough to run as a daemon on a laptop.

## Current status

**Phase 1 complete**: config, schema, CLI, server skeleton.

- Config system with TOML loading, glob-based filtering (exclude/include), per-repo and per-branch overrides
- Source auto-discovery from checkout roots with configurable layout patterns
- SQLite schema: repos, files, strings (FTS5 trigram), refs, branch_files, repo_branches, git_tags, repo_packages
- CLI: `init`, `add`, `scan`, `status`, `query`, `serve` + `--readme` for embedded docs
- Axum server: `/status`, `/repos`, `/query`

**Phase 2 in progress**: rule engine core complete, extractors and scan pipeline next.

- Declarative JSON rule engine with structural tree walking, named captures, grouped emit
- Git context matching (repo/branch/tag globs), file path matching, pipe-delimited alternatives
- Value regex for capture splitting (scoped packages: `@scope/name@version`)
- JSON Schema generated from Rust types via schemars
- 49 insta snapshot tests

---

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

## CLI

```
sprefa init                          # create sprefa.toml + DB
sprefa add <path> [--name <name>]    # register a repo
sprefa scan [--repo <name>]          # index repos
sprefa query <term>                  # trigram substring search
sprefa status                        # show indexed repos
sprefa serve                         # start HTTP daemon
sprefa --readme                      # print this document
```

## Rule engine

Declarative JSON rules for "how do strings in structured files point to things in other repos." The entire indexed space is a DOM:

```
root
├── repo[name="org/frontend"][branch="main"]
│   ├── file[path="package.json"][ext="json"]
│   │   └── (json tree nodes)
│   ├── file[path="values.yaml"][ext="yaml"]
│   │   └── (yaml tree nodes)
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
ImportPath, ImportName, ExportName, DepName, DepVersion,
RsUse, RsDeclare, RsMod
```

**String normalization:**
- `norm`: strip non-alphanumeric, lowercase. `my-UI` -> `myui`
- `norm2`: configurable suffix stripping (`-service`, `-api`, `-v2`)

## Parser strategy

| Priority | Parser | Languages |
|----------|--------|-----------|
| 1 | oxc_parser | JS, TS, JSX, TSX |
| 2 | SCIP consumption | Any language with a SCIP indexer (Rust, Go, Java, Python, etc.) |
| 3 | Custom | JSON, YAML, TOML (lodash-style dot-path key encoding) |
| 4 | ast-grep (lib) | Everything else |

## Workspace

```
crates/
  config/       config types, TOML loading, filtering, source discovery
  schema/       SQLite types, migrations, query functions
  extract/      extractor trait + language-specific implementations
  rules/        declarative rule engine: types, JSON schema, tree walker, emit
  scan/         git integration, scanner orchestration, resolution pass
  server/       axum HTTP daemon
  cli/          clap CLI
```

## Testing

All tests use `insta` for snapshot assertions. 49 tests covering config parsing, filter resolution, source auto-discovery, rule deserialization, JSON schema generation, tree walking with captures, ref emission with parent linkage, cross-format dep extraction (package-lock + pnpm-lock), git context matching, file glob matching, and template expansion.
