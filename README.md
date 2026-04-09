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

## Quick start

```bash
sprefa init                          # create config + SQLite DB
sprefa add ~/repos/my-frontend       # register a repo
sprefa add ~/repos/my-backend
sprefa scan                          # index everything
sprefa sql "SELECT * FROM import_path_data LIMIT 20"
sprefa eval 'json({ name: $N })' package.json   # one-off extraction, standalone
```

## The .sprf language

Three statement types: `rule`, `query`, `check`.

### rule -- extract structured data from files

```sprf
rule(package_name) {
  fs(**/Cargo.toml) > json({ package: { name: $NAME } })
};

rule(deploy_image) {
  fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } })
};

rule(env_var_ref_ts) {
  fs(**/*.ts) > ast(process.env.$NAME)
};
```

Each `$VAR` becomes a column in the rule's output table. Co-extracted captures from the same site share a `group_id`, preserving "these values came from the same place."

**Chain with `>`** -- sequential pipeline: glob files, then walk structure, then match AST
**Branch with `{ }`** -- fork into independent extraction paths

```sprf
rule(config_value) {
  fs(**/config.yaml) > json({
    database: { host: $HOST };
    database: { port: $PORT };
  })
};
```

**Cross-rule references** -- bind columns from an upstream rule's table, creating a dependency edge:

```sprf
rule(svc_version) {
  deploy_config(repo: $REPO, pin: $PIN)
  repo($REPO) > rev($PIN) > fs(**/package.json) > json({ version: $VERSION })
};
```

**Rule unions** -- same name + same capture shape = rows in the same table:

```sprf
rule(dep_source) { fs(**/package.json) > json({ dependencies: { $DEP: $_ } }) };
rule(dep_source) { fs(**/Cargo.toml) > json({ dependencies: { $DEP: $_ } }) };
```

**Namespaces** -- each `.sprf` file's stem becomes a table prefix. `infra.sprf` containing `rule(image)` produces `infra__image_data`.

### Selector tags

| Tag | What it does |
|---|---|
| `fs(glob)` | Match files by path |
| `json(pattern)` | Walk JSON/YAML/TOML with destructuring, captures, regex keys, recursive descent |
| `ast(pattern)` or `ast[lang](pattern)` | Structural match via ast-grep (any tree-sitter language) |
| `line(pattern)` | Line-based regex or segment capture |
| `repo(pattern)` | Match/capture repo name; triggers demand scanning |
| `rev(pattern)` / `branch()` / `tag()` | Match/capture git ref; triggers demand scanning |
| `folder(pattern)` / `file(pattern)` | Match directory or full path |

**json() pattern syntax:**

| Syntax | Meaning |
|---|---|
| `{ key: pat }` | Match key, descend into value |
| `{ $KEY: $VAL }` | Iterate all keys, capture each pair |
| `{ re:^pattern: $V }` | Regex on key name |
| `{ **: pat }` | Recursive descent (any nesting depth) |
| `[...$ITEM]` | Array iteration with capture |
| `$NAME` | Capture leaf value |
| `$_` | Wildcard, bind nothing |

**Segment captures** in patterns like repo/rev/file: `$ORG/$REPO` captures both sides of a `/`. `${NAME}` for adjacent captures.

**Extract-time constants** -- pre-bound captures for constraining patterns to current context:

| Constant | Value |
|---|---|
| `$currentRepo` | Current repo name |
| `$currentRev` | Current branch or tag |
| `$currentFile` | File path being extracted |
| `$currentDir` | Parent directory |
| `$currentStem` | Filename stem (extension stripped) |
| `$currentExt` | File extension |

### query -- SQL over extracted data

Each rule produces a table (`{rule}_data` or `{namespace}__{rule}_data`). Queries are raw SQL against those tables.

```sprf
query(transitive_dep) {
  SELECT a.dep, c.dep
  FROM dep_to_package_data a
  JOIN package_has_dep_data b ON a.pkg = b.pkg
  JOIN dep_to_package_data c ON b.dep = c.dep
};
```

**Built-in SQL functions:**

| Function | What it does |
|---|---|
| `re_extract(text, pattern, group)` | Regex capture group extraction |
| `split_part(text, delim, n)` | Nth part of delimited string |
| `repo_name(repo_id)` | Repo name lookup |
| `file_path(file_id)` | File path lookup |
| `fzy_score(haystack, needle)` | Fuzzy match score (0.0-1.0) |

**Built-in views:** `repo_tags` (semver-tagged revisions), `repo_branches` (branch revisions).

### check -- CI-friendly invariant verification

Same as query. Rows returned = violations. Exit code 1.

```sprf
check(missing_tag) {
  SELECT dc.svc, dc.repo, dc.tag
  FROM deploy_config_data dc
  LEFT JOIN repo_tags rt ON rt.repo = dc.repo AND rt.tag = dc.tag
  WHERE rt.repo IS NULL
};
```

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

When a rule captures `repo($REPO)` and `rev($TAG)`, the scan pipeline extracts those values, checks out the referenced repo at that tag, scans it, and repeats until all (repo, rev) pairs are scanned. A deploy values.yaml referencing `repository: myorg/backend, tag: v2.1.0` automatically triggers scanning of `myorg/backend@v2.1.0`.

## CLI

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

## Built-in extractors

| Extractor | Parser | Files | Extracts |
|---|---|---|---|
| JS/TS | oxc | .js .jsx .ts .tsx .mjs .cjs .mts .cts | imports, exports, require(), re-exports |
| Rust | syn | .rs | use paths (crate/self/super), declarations, mod, extern crate |
| Rule engine | serde | JSON, YAML, TOML | configurable tree walks via .sprf rules |
| ast-grep | ast-grep-core | anything with a tree-sitter grammar | structural pattern matching |

## Watch + rewrite

The watcher detects file moves (content hash correlation within 100ms) and declaration renames (span proximity diffing within 64 bytes), then rewrites all affected import/use statements across the index.

| Event | JS/TS | Rust |
|---|---|---|
| File move | Rewrite import/require/export paths | Rewrite `use` statements |
| Declaration rename | Rewrite `import { OldName }` | Rewrite `use crate::mod::OldName` |
| File delete | Log broken references | Same |

Rust module mapping: `src/lib.rs` -> `crate`, `src/foo/bar.rs` -> `crate::foo::bar`. All prefix styles (`crate::`, `self::`, `super::`, chained `super::super::`) resolve correctly.

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
branches = ["main"]

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
  schema/     SQLite migrations, per-rule table DDL
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
