# Turnkey query surface: `dl what` / `dl q <verb>` / `dl find`

## Context

dl has 11 built-in rel families (call, type, dataflow, module, scip, CST node, string/ref
spine, comment/doc, git, demand, meta) but the on-ramp is "write a .dl file, import std,
learn syntax." Chris wants the CLI turnkey for devs and AI agents: one turbo-generic query
across all graph families for a given anchor (name / path / path:line / string), plus
pre-baked named concepts (blast radius, who-calls, flows-to) that work from the first
command with no authoring. Research done 2026-07-10 by a Fable subagent (read-only sweep of
cli/, engine rel decls, std/, examples/, README catalog).

## Findings that shape the design

### Existing surface to reuse

| thing | where | role in this plan |
| --- | --- | --- |
| `dl daemon rows <rel>` / `query_rel` RPC | src/cli/daemon.rs:1821 | precedent for a new RPC verb |
| `dl/query` paging `{limit, offset, count}` | src/lsp.rs:417 | the result envelope shape |
| `rel_catalog(name, group, cols, doc)` + `rel_col(rel, pos, col, type, variants)` | meta rels | schema-driven search (Tier 3) |
| synthesized-program merge | `--move` driver, multi-file one-shot merge | param injection for verbs |
| `run_reaches_pair` condensation walk | engine closure machinery | blast-radius without materializing closure |
| `doc_text_rels_used` family-force seam | engine/extract | meta-query must force type/call/df/scip families used |
| examples/symbol-profile.dl | examples/ | IS "what-is-this-symbol", target is a hand-edited fact |
| std/flow.dl, std/callgraph.dl, doc-coverage.dl, taint.dl | std/, examples/ | verb bodies already written |

### The join-spine problem (why nothing generic exists today)

Five anchor keyspaces, four concrete mismatches:

1. **sym qualification drift**: call_edge endpoints = `repo::file::kind::name`; df_node.fn =
   bare `file::kind::name`; type_entity.sym bare with repo as a column. std/flow.dl pays a
   per-row `replace_re` strip (std/flow.dl:20-31).
2. **name-keyed vs sym-keyed**: type_edge name-keyed; type_link/call_edge sym-keyed; scip_*
   moniker-keyed with scip_name (canonical) / scip_binding (local alias) as the bridge.
3. **position bases**: df/call/diag lines 1-based; scip_occurrence 0-based; CST `node` has
   byte lo/hi only; normalize at the RESULT ENVELOPE, never in stored rels (D5 note:
   re-basing stored spans desyncs loop_over/nest).
4. **ref.file is a content FileId** (blake3/blob oid), not a path; path recovery joins
   `file.content`, one content can map to multiple paths.

An anchor resolver (string -> {(repo, sym)} ∪ {path:line} ∪ {StringId}) exists nowhere;
every .dl program re-derives a slice.

### Ledger corrections (do regardless of scope)

- CLAUDE.md batch-2 items **S1 and S2 are already fixed in v0.6.24**: `scip_occurrence`
  carries line/col/role, `scip_binding` carries the local alias name. Mark [x].
- Stale comment at src/cli/daemon.rs:168 still says `--rows` (the flag became
  `dl daemon rows`).

## Design (3 tiers)

### Tier 1: `dl what <anchor>` + `dl summary [path]` (items 1-2)

Rust-side meta-query: new daemon RPC beside `query_rel` + CLI verb + in-process fallback.
Result rows `(family, rel, match_kind, repo, file, line, detail)` in the dl/query paging
envelope.

Anchor resolution algorithm:
1. Classify: trailing `:digits` -> path:line; matches `file.path` or contains `/` -> path;
   else name term (glob `*` allowed).
2. Name fan-out (UNION): type_entity.name, call_name.name, scip_name.name +
   scip_binding.local_name (alias hits), df_node.var, string.norm (text tier). Collect
   candidate (repo, sym) pairs + StringIds. Normalize the call_edge repo-prefix ONCE here.
3. Per-sym neighborhood: def location (call_def/type_entity), caller/callee counts
   (call_edge), type_link in/out, type_sig slots, doc_comment presence, scip_occurrence
   count.
4. Text tier: string -> ref -> file.content join to recover (path, byte span), bytes->line
   via the engine's line index.
5. path:line anchor: call_site/df_node/comment_node at that line; CST containment via node
   lo/hi after line->byte conversion; enclosing fn via call_def spans.

`dl summary <path>`: entities declared, imports in/out + unresolved, callables + fan
totals, doc coverage ratio (type_entity anti-join doc_comment), comment count, df node
count.

Engine requirement: the meta-query marks type/call/dataflow/scip families used (the
doc_text_rels_used seam) so demand-gated extraction fires. Daemon-first; in-process
fallback scans.

### Tier 2: `dl q <verb> [args]` concept verbs (items 3-4)

Verbs = **embedded .dl programs + a parameter-injection seam** (runner synthesizes
`target("<arg>").` facts and merges, the `--move` synthesized-program precedent). NOT baked
Rust (wasm-generality law: engine ships seams, policy lives in programs). Discoverability
via a new `verb_catalog(name, args, doc)` meta rel beside rel_catalog, listed by bare
`dl q` and `dl docs`.

| verb | powered by |
| --- | --- |
| `who-calls <name>` / `calls-of <name>` | call_name + call_edge + call_site (new ~15-line embedded program) |
| `blast-radius <sym\|name>` / `dependents` | Rust-assisted: pins into `run_reaches_pair`, NOT materialized closure (closure-read restriction + unpinned guard) |
| `flows-to <anchor>` | embeds `use "std/flow.dl".`, seeded recursion from resolved df_nodes |
| `where-defined <name>` | type_entity + call_def + scip_def |
| `undocumented [glob]` | doc-coverage.dl promoted, glob injected |
| `unused` | std/callgraph.dl `unused` promoted |
| `cycles` | scc(call_edge) / scc(module_edge) |
| `what <anchor>` | alias for Tier 1 |

### Tier 3: `dl find` schema-driven filter (item 6, deferred)

`dl find 'name:parse* group:call file:src/**'` — every catalogued rel whose columns include
a recognized anchor name (sym, name, file, path, line, repo, var, specifier) is
automatically searchable; compiles to UNION ALL over `rel_<name>` tables using `rel_col`.
The flow-panel `_node`/`_edge` convention generalized from drawable to searchable; covers
user-declared rels for free. Generic graph TRAVERSAL language: ruled out (datalog
reinvented); Tier 3 is index lookup + filtering only.

### MCP (item 5)

Built-in tools in the `--mcp` adapter, advertised even when the served program defines
none: `dl.what` (Tier 1), `dl.verb` (Tier 2), `dl.rows` (existing query_rel), `dl.sql`
(existing query_sql, gated). This IS the decided-unbuilt ledger item "adapter built-in
dl.query/eval tool bridging the daemon's eval RPC". JSON envelope
`{family, rel, repo, file, line, detail, total, offset}`. Agents get who-calls /
blast-radius / flows-to as one tool call; authoring datalog becomes the escalation path,
not the entry.

## Build order

| # | item | size | files (primary) |
| --- | --- | --- | --- |
| 1 | Anchor resolver engine fn (name -> (repo,sym)/StringId/path:line; sym normalization incl. call_edge repo-prefix strip done once in Rust) | S | src/engine/ (new module), tests |
| 2 | `dl what <anchor>` + `dl summary` + daemon RPC + family-force seam | M | src/cli/, src/cli/daemon.rs, src/engine/extract.rs |
| 3 | Param injection for embedded programs + `dl q` runner + verb_catalog | S | src/cli/mod.rs, meta rel decl |
| 4 | First 6 verbs + pinned-reach Rust path for blast-radius/dependents | M | assets or embedded .dl, engine closure path |
| 5 | Built-in MCP tools bridging the RPCs | S | src/mcp.rs |
| 6 | `dl find` schema-driven filter | M-L | deferred, separate arc |

Plus the two ledger corrections (S1/S2 marked done, daemon.rs:168 comment) in whichever
arc lands first.

## Verification

- Unit: resolver classification + normalization table tests (bare vs repo-prefixed syms,
  glob names, path:line parse).
- e2e per tier in tests/it/: `dl what <known sym>` on a fixture repo returns rows from >=3
  families; `dl summary src/lsp.rs` on this checkout non-empty; `dl q who-calls` /
  `blast-radius` against the existing callgraph fixtures; MCP tools/list advertises dl.what
  and tools/call round-trips (mcp_lifecycle.rs pattern).
- Dogfood: run `dl what Engine` and `dl q blast-radius tick` on this repo via the daemon;
  confirm paging envelope + no unpinned-closure guard trip.
- Suites: cargo test lib + it green; `dl --check` rails (magic-rel audit: the new meta rel
  verb_catalog must be a catalogued RelDecl, never a literal-name read).
