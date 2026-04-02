`<human-no-llm-kthnx>`
Hi, the main thesis of this is to ask the question of how far can source strings constants, their import/export identifiers, their filename paths, and their git repo names, take a semantic grep engine that just spits things into prolog aka recursive sqlite queries.

I want to basically allow extremely bespoke pattern chaining of any and all open source tools for parsing. 

Can't believe I'm gonna say this but just imagine html(really xml please), all ast's are representable as a dom tree.

So with that concept, and the filesystem as a tree, we now have a very large shit load of trees.

So this is an attempt at creating a very higher and lower scoped tree query/matching engine that attemps to unify all of that tree into 1 interface.

TLDR; Programmable ripgrep and fzf and ast-grep go BURRRRRR. But also what if ai could trace its steps into a system that allows describing every grep and filesystem find etc. were all encoded into a language that embeds that flow. In order to encode arbitrary filesystem+source AST into prolog for graph algorithms, I wanted to build this.

This system does a whacky refactoring that was the OG idea, that turned into journey studying ast-grep and sqlite and wanting to be able to encode cross filesystem relationships with whatever pattern matching tech I can find open source then make. `ast-grep` hauls ass. json/yaml/toml/xml are all trees already, so thats nice. 

So now you can imagine every tree node as an html element or a div with a classname. Okay now make that work like prolog.
`</human-no-llm-kthnx>`

# sprefa

Declarative cross-codebase extraction and graph queries. Glob files, walk structured data, match AST patterns, capture strings, link them across repos, run recursive queries over the result.

```
.sprf rules  ->  scan files  ->  extract refs  ->  SQLite index  ->  query / check / link
```

## The .sprf language

Four statement types, one delimiter (`>`), one terminator (`;`).

### rule -- extraction

Head declares a relation name and typed captures. Body is a selector chain that walks filesystem and data trees.

```sprf
rule package_name($NAME) >
  fs(**/Cargo.toml) > json({ package: { name: $NAME } });

rule dep_name($NAME) >
  fs(**/Cargo.toml) > json({ re:^(dev-)?dependencies: { $NAME: $_ } });

rule helm_image(repo($REPO), rev($TAG)) >
  fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } });

rule env_var_ref_ts($NAME) >
  fs(**/*.ts) > ast(process.env.$NAME);
```

**Capture annotations** drive runtime behavior:

| Annotation | Effect |
|---|---|
| `$VAR` | Plain string capture |
| `repo($VAR)` | Triggers demand scanning -- discovers and scans the repo named by this value |
| `rev($VAR)` | Triggers demand scanning -- checks out and scans this tag/branch in the target repo |
| `name($VAR)` | Semantic tag (display, future validation) |
| `file($VAR)` | Path resolution to file_id (future) |

**Rule unions**: multiple rules with the same name and same capture shape produce rows in the same relation. Use different names when the distinction matters.

```sprf
rule dep_source(name($DEP)) >
  fs(**/package.json) > json({ dependencies: { $DEP: $_ } });

rule dep_source(name($DEP)) >
  fs(**/Cargo.toml) > json({ dependencies: { $DEP: $_ } });
```

### Selector chain

Each rule body chains slots with `>`. Three slot types:

**`fs(glob)`** -- file matching

```sprf
fs(**/Cargo.toml)
fs(**/*.yaml)
fs(**/docker-compose*.yaml)
```

**`json(pattern)`** -- structural walk on parsed JSON/YAML/TOML

| Syntax | Meaning |
|---|---|
| `{ key: pat }` | Match key, descend into value |
| `{ $KEY: $VAL }` | Iterate all keys, capture each pair |
| `{ re:^pattern: $V }` | Regex on key name |
| `{ **: pat }` | Recursive descent (any nesting depth) |
| `[...$ITEM]` | Array: iterate elements, capture each |
| `$NAME` | Capture leaf value (SCREAMING_CASE) |
| `$_` | Match any value, bind nothing |

**`ast(pattern)`** -- ast-grep structural matching

```sprf
fs(**/*.ts) > ast(process.env.$NAME);
fs(**/*.rs) > ast[rust](fn $NAME($$$ARGS) -> $RET { $$$BODY });
```

Optional `[lang]` suffix overrides language detection. Captures use ast-grep's `$VAR` and `$$$VAR` (multi-node) syntax.

### link -- materialized edges

Pre-computed joins between rule relations, stored as edges in `match_links`.

```sprf
link(NAME > NAME, norm_eq) > $dep_to_package;
link(REPO > NAME, norm_eq) > $image_source;
link(NAME > KEY, norm_eq) > $env_var_binding;
```

Source kind `>` target kind, then predicates: `norm_eq`, `string_eq`, `target_file_eq`, `same_repo`, `same_file`, `stem_eq_src`, `ext_eq_src`, `dir_eq_src` (and `_tgt` variants).

### query -- recursive graph traversal

Body atoms are whitespace-delimited (implicit AND). Rule names are queryable relations. Recursive references compile to recursive CTEs.

```sprf
query transitive_dep($A, $C) >
  dep_to_package($A, $B)
  package_has_dep($B, $C);

query same_ecosystem($A, $B) >
  dep_to_package($A, $X)
  dep_to_package($B, $X);
```

**Built-in relations** (computed from base tables, no extraction needed):

| Relation | Source |
|---|---|
| `$.has_kind($M, "kind")` | matches with this kind |
| `$.has_norm($M, "val")` | matches whose string normalizes to val |
| `$.has_value($M, "val")` | matches with exact string value |
| `$.same_norm($A, $B)` | matches A and B share the same norm |
| `$.same_norm2($A, $B)` | matches A and B share the same norm2 (suffix-stripped) |
| `$.same_repo($A, $B)` | matches A and B are in the same repo |
| `$.same_file($A, $B)` | matches A and B are in the same file |
| `$.in_repo($M, "name")` | match M is in repo with this name |
| `$.in_file($M, "path")` | match M is in file matching this path |
| `$.repo_has_tag($REPO, $TAG)` | repo has this git tag |
| `$.repo_has_branch($REPO, $BRANCH)` | repo has this branch |
| `$.repo_has_rev($REPO, $REV)` | repo has this revision (tag or branch) |

### check -- invariant verification

Same syntax as query. A check with results means violations. Exit code 1.

```sprf
check missing_tag($SVC, $REPO, $TAG) >
  deploy_config($SVC, $REPO, $TAG)
  not $.repo_has_tag($REPO, $TAG);
```

`not` compiles to `NOT EXISTS`. Negated relations must be fully computed first (topological ordering enforces this).

## How it works

Every interesting string in a codebase is a **ref**: a file contains a string at a byte offset. **Matches** give refs semantic labels (rule name + kind). **Links** connect matches across files and repos. **Queries** traverse the resulting graph.

Co-extracted captures share a `group_id` in the matches table, preserving the "these values came from the same extraction site" relationship. Rule names become queryable relations through group_id joins -- a query like `deploy_config($SVC, $REPO, $TAG)` joins three match rows that share a group_id.

```
repos 1->M files 1->M refs M<-1 strings
                              |
                          matches (kind, rule_name, group_id)
                              |
                          match_links (source -> target, link_kind)
```

### Pipeline

```
git ls-files
  -> parallel rayon walk (content hash, skip set)
  -> per-file extraction (JS/TS via oxc, Rust via syn, structured data via rule engine, AST via ast-grep)
  -> bulk flush to SQLite (dedup strings, chunked inserts)
  -> resolve import targets (oxc_resolver with tsconfig support)
  -> resolve match links (execute link rules)
  -> demand scanning (repo/rev annotations trigger recursive discovery)
```

### Demand scanning

When a rule captures `repo($REPO)` and `rev($TAG)`, the scan pipeline:

1. Extracts string values from matches with `scan=repo` / `scan=rev` labels
2. Pairs them by file (repo + rev from the same source file)
3. Checks out and scans each discovered (repo, rev) target
4. Repeats until stable (fixed-point iteration, max 10 rounds)

This lets a Helm values.yaml referencing `repository: myorg/backend, tag: v2.1.0` trigger scanning of `myorg/backend` at tag `v2.1.0`, indexing its contents into the same graph.

## CLI

```
sprefa init                          # create sprefa.toml + SQLite DB
sprefa add <path> [--name <name>]    # register a repo
sprefa daemon [--repo <name>]        # scan + watch + serve
sprefa scan [--repo <name>]          # index repos
sprefa watch [--repo <name>]         # filesystem watcher, auto-rewrite
sprefa serve                         # HTTP server (127.0.0.1:9400)
sprefa query <term>                  # trigram substring search
sprefa check                         # run all check rules, exit 1 on violations
sprefa sql "<SELECT ...>"            # read-only SQL against the index
sprefa status                        # show indexed repos
```

`sprefa daemon` runs the full pipeline: scan all repos, start filesystem watchers, start ghcache subscriber (if configured), start HTTP server.

### Direct SQL

```bash
sprefa sql "SELECT s.value, m.kind, m.rule_name
            FROM strings s
            JOIN refs r ON r.string_id = s.id
            JOIN matches m ON m.ref_id = r.id
            LIMIT 20"
```

Read-only (SELECT, WITH, EXPLAIN, PRAGMA). Tab-separated output with header row.

## Extractors

| Extractor | Parser | Languages | Extracts |
|---|---|---|---|
| JS/TS | oxc | .js .jsx .ts .tsx .mjs .cjs .mts .cts | imports, exports, require(), re-exports |
| Rust | syn | .rs | use paths, declarations (fn/struct/enum/trait/impl), mod, extern crate |
| Rule engine | serde | JSON, YAML, TOML | configurable tree walks with captures |
| ast-grep | ast-grep-core | anything with a tree-sitter grammar | structural pattern matching |

## Watch + rewrite

The watcher classifies filesystem events and propagates renames:

| Event | JS/TS | Rust |
|---|---|---|
| File move | Rewrite all import/require/export paths targeting the moved file | Rewrite all `use` statements referencing the old module path |
| Declaration rename | Rewrite `import { OldName }` across consumers | Rewrite `use crate::mod::OldName` across consumers |
| File delete | Log broken references | Same |

Move detection correlates delete+create pairs by content hash within a 100ms window. Declaration renames are detected by diffing extractions by span proximity (within 64 bytes = same declaration, different name).

### Rust module mapping

```
src/lib.rs       -> crate
src/utils.rs     -> crate::utils
src/foo/mod.rs   -> crate::foo
src/foo/bar.rs   -> crate::foo::bar
```

All prefix styles (`crate::`, `self::`, `super::`, chained `super::super::`) resolve to absolute module paths at query time.

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
  cli/        clap CLI
  config/     TOML loading, filtering, source discovery
  schema/     SQLite migrations, types, queries
  extract/    Extractor trait + RawRef
  index/      parallel file walk, xxh3 hashing
  cache/      bulk flush, import resolution (oxc_resolver), match links, demand scanning
  rules/      rule engine: types, tree walker, link compiler, query compiler
  sprf/       .sprf parser: text -> AST -> RuleSet + DerivedRules
  js/         oxc JS/TS extractor
  rs/         syn Rust extractor
  watch/      filesystem watcher + rewrite pipeline
  scan/       Scanner coordinator
  server/     axum HTTP server
  sprf-lsp/   LSP server for .sprf files
editors/
  vscode/     tmLanguage syntax highlighting
```
