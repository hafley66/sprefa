# v5 study tracker (`ls -la`)

The goal: SQLite + Rust + tree-sitter → a typed, LSP-enabled Datalog/shell language
where path/glob/repo/rev are **language-defined typed literals** (unquoted, with
typeahead), not bash-style untyped convention. Glean is reference only, not a base.

This file is the maintained `ls -la`. When a tangent ends, come back here. Each row:
status, effort (S<1d / M~few days / L~weeks), track, item, why.

```
STAT  EFF  TRACK  ITEM                                         WHY
next   M   A      Datafun (Arntzenius, papers + thesis)        types + aggregation + recursion, fixes Value=Text|Int + no-agg
next   S   B      Angle schema language (Glean docs)           rich-typed fact lang already shipped; your type target
next   S   B      Nix path literals + lexer                    unquoted typed path; the exact "no quotes" ergonomic
todo   M   A      Soufflé type system (subtypes/records/ADTs)  practical typed datalog you can copy decisions from
todo   M   B      reader/lexer for unquoted path/glob/rev      the hard syntax problem: lex paths without ambiguity
todo   M   B      bidirectional type checking (Dunfield-Krishnaswami tutorial)  how to actually check the rich types
todo   S   B      Nix/refinement: optional repo/rev as refined path   "(optional) repo/rev" = refinement over a path type
todo   M   C      LSP completion + semantic tokens spec        typeahead IS completion; you have lsp.rs, deepen it
todo   M   C      Salsa (rust-analyzer incremental queries)    keeps LSP responsive on edit; the v4 lesson, done right
todo   S   E      tree-sitter query language + incremental     fact source; you already lean on it, learn its query lang
todo   S   E      ast-grep-core internals                      you depend on it (sg matcher); know the engine
tang   S   A      Flix (datalog + lattices in an ML)           ergonomics of embedding typed datalog; nice-to-have
tang   S   A      Dedalus / Bloom (time/space as first-class)  rev/time coordinate prior art; only if time-queries grow
done   -   D      Cozo storage trait                           docs/ext-cozo-storage-trait.md
done   -   D      Datafrog memstore                            docs/ext-datafrog-memstore.md
done   -   D      DBSP incremental                             docs/ext-dbsp-incremental.md
done   -   D      SQLite × graph landscape                     docs/research-sqlite-graph-landscape.md
done   -   D      incremental-reactive-datalog landscape       research/2026-05-21-...md
done   -   C      LSP-made-easy                                docs/ext-lsp-made-easy.md
done   -   D      petgraph SCC swap                            docs/ext-petgraph-scc-swap.md
```

## READ NEXT (the path, when you tangent back)

1. **Datafun** — first. It is the answer to v5's two biggest gaps at once: a real
   type system over relations, and aggregation, with recursion kept sound via
   monotonicity/lattice types. `ast.rs:27` `Value = Text|Int` and the README's
   "no aggregation" line are exactly what Datafun's type theory removes. Read the
   POPL paper, then the thesis chapters on the lattice/monotonicity types.
2. **Angle schema docs** — your rich-type target already exists in the wild:
   records, sums, enums, arrays, maybe, type aliases, predicate references. Do not
   reinvent; decide which of these v5 needs and steal the shape.
3. **Nix path literals + Datafun in hand** — design the unquoted typed literal.
   Nix proves `./a/b` can be a typed `path` with no quotes; the open problem is
   lexing it unambiguously next to globs and revs. This is Track B's crux.

## Tracks

- **A — typed Datalog semantics** (the core gap). Datafun, Soufflé types, Flix,
  Dedalus. Outcome: replace `Value = Text|Int` with a real type lattice; add
  aggregation; keep stratified recursion sound.
- **B — rich types + surface syntax** (the vision). Angle schema, Nix paths,
  bidirectional typing, refinement types, reader/lexer design. Outcome: `scan(HEAD,
  src/**/*.rs)` parses path/glob/rev as typed terms with LSP typeahead, no quotes.
- **C — LSP / editor** (typeahead = completion). LSP completion + semantic tokens,
  Salsa. You have `lsp.rs` and `docs/ext-lsp-made-easy.md`; extend to completion.
- **D — engine / storage / incrementality** — mostly DONE (see `done` rows). Glean
  ownership/stacked-DB is the one remaining reference read if incrementality grows.
- **E — tree-sitter / extraction depth** — tree-sitter query lang, ast-grep-core,
  SCIP. Fact-source mastery.

## Architecture principle (pinned)
**Retraction lives at source-adapter boundaries via snapshot-diff, not in the
engine.** Each mutable/external source (editor buffer, GitHub PR state, Jira
status, a re-poll) holds old+new snapshots, computes asserts/retracts at its own
boundary, and feeds them in. The engine stays a sync fixpoint scoped by
`affected_derived` (`engine.rs:342`). This is how v5 supports LSP-push + external
sources without rebuilding v4's internal delta (DBSP/DRed) engine. Only a perf
threshold (per-edit tick too slow on a real repo) ever forces real incremental
view maintenance; correctness never does.

## Programmable LSP (first-class, in the DSL)
- Each LSP method = a typed relation + a seeding convention. `diag` is the working
  prototype (`lsp.rs`: tick_paths + publish, replace-by-URI = free retraction).
- Method kinds: PULL (hover/completion/def/refs/codeAction) = demand point query,
  no state; PUSH-replace (publishDiagnostics) = recompute affected + resend;
  PUSH-refresh (semanticTokens/inlayHint) = invalidate via affected-set, editor
  re-pulls. Protocol is invalidate-and-recompute, not delta-streaming.
- DEPENDS ON Track B types: code_action/completion/hover responses need
  records/sums, not Text|Int. Build the type layer first.
- Reclaim ONE v4 idea: dirty-buffer overlay (`v4/src/dirty_source.rs`) so
  completion/hover read the unsaved buffer, not disk. Currently save-driven
  (`lsp.rs:1-3`, didChange ignored).

## External fact sources (Jira / GitHub / general `sh`)
- Generalize the missing `sh` source into pollers/webhook receivers writing
  `gh_pr(repo,number,state,head_sha,author,t)`, `jira(key,status,assignee,t)`.
- Join to code via `(repo, rev)` (PR head sha) and `pr_touches(pr,path)` →
  `module_edge`. e.g. "blast radius of open PRs over the focused module."
- Bound the pull: GitHub ETag/304 + webhooks, Jira `updated>=since` + webhooks;
  prefer webhook→assert, poll as fallback, debounce burst→one tick (push-pull-dam).
- States are enums (open/merged/closed) → another Track B types dependency.

## SCIP resolved tier (not tree-sitter lexical guessing)
- KEY: SCIP monikers = stable symbol identity ACROSS revs. This single property
  powers BOTH accurate resolution AND temporal path-analysis. Tree-sitter nodes
  have no cross-rev identity. So "full SCIP" and "evolving graph over time" are
  the same requirement.
- Two tiers, not two programs (you have callgraph-resolved.dl vs -ast.dl): lexical
  spine (tree-sitter/syn/ast-grep, always present, syntactic) + resolved overlay
  (SCIP, when indexer ran). Rules prefer resolved, fall back to ast. Same overlay
  pattern as the dirty-buffer.
- Feeders: `rust-analyzer scip` (Rust), scip-typescript (TS today), tsgo SCIP
  emitter (future, fast). `scip_import.rs` already ingests `scip_*`.

## tsc-go / TS7 (perf ceiling raised)
- TS 7.0 Beta 2026-04-21, Go/Corsa, ~10x (VS Code 1.5M LOC 78s->7.5s). `tsgo`,
  `@typescript/native-preview`. Embeddable `@typescript/api` is WIP, stable not
  until 7.1. Ingest today via tsgo LSP server, not a library embed.
- Validates the sync-fixpoint bet: fast enough to re-index TS per tick ("full fuck
  it" full recompute), no TS incremental machinery needed. Debounce -> tick.

## AI-authorship loudspeaker
- Facts: `ai_rev <- commit_trailer(rev,"Co-Authored-By",x), x=~"Claude|Copilot"`;
  `ai_line <- blame(path,ln,rev), ai_rev`. blame = new source (no git2 in v5 yet).
- `protected(p,lo,hi)` from .sprefa.toml/marker -> `diag` on overlap.
- Three outputs off one `diag`: --lsp (squiggle), --check (CI gate, have it),
  PreToolUse hook running `dl --check` (HARD gate, refuses the agent edit live).
  This is the fix for "markers the AI edited anyway": make it impossible, not
  advisory. Edit-level human-vs-AI: atomic didChange vs keystroke dynamics.

## Track F — temporal / tribal knowledge (MSR)
- Facts through (rev,t) become the explicit shared knowledge base: `coupled`
  (evolutionary coupling, co-change without an import edge = hidden dep),
  `hotspot` (churn x complexity), `owns`/bus-factor (blame x author over time),
  symbol `lifecycle` (SCIP moniker tracks identity across rename). Surface via
  loudspeaker as inline hints at edit point.
- Study: Zimmermann/Gall evolutionary coupling ("Mining Version Histories to
  Guide Software Changes"); Adam Tornhill CodeScene + Code Maat (open tool, the
  algorithms); SCIP monikers for cross-rev identity. Needs `time.dl`/
  `module-history.dl` (have) + blame + aggregation (Track A gap).

## Decisions already made
- Not Glean. Fixed-language server; forecloses language design + embeddability.
- v5 over v4. Kept stable identity + query language; dropped the bounded-RSS
  incremental-view-maintenance machinery (correct but brutal). See `../v4`.

## Open design questions (park tangents here)
- Ground facts: should the DSL allow `edge("a","b").` literal assertions, or stay
  extraction-only? (Glean has them; v5 does not — `parse.rs:86`.)
- Path literal lexing: how to disambiguate unquoted path vs glob vs rev vs ident
  in one lexer without a mode stack that fights LSP incremental lexing.
- Type of a path: opaque `Path`, or refined `Path @ rev` with existence proofs?
- Self-telemetry as a fact source: ingest own `cd`/`checkout`/editor-focus/edit
  events as located relations keyed on `(repo, path, rev, t)`, join against
  `module_edge` etc. "Reference calculus" = empirical traversal minus derived
  graph (`walked(a,b), !module_edge(a,b)`). Start from atuin's SQLite schema
  (command+cwd+ts); ActivityWatch for window focus. Keylog tier only if
  within-edit dynamics needed. Event-granularity gets ~90% of signal.
  Three cursor spaces normalize to `located(coord, t)`: shell `(repo,rev,cwd)`
  via atuin, editor `file-focus` via LSP didOpen, browser `url+DOM-element` via
  a content script / rrweb (NOT a service worker — SW sees only fetch/network,
  no DOM events). rrweb = the session-replay library for the browser layer.
  Editor layer: NOT a DOM plugin (VS Code sandboxes the workbench DOM from
  extensions; only your own webview is reachable). Two collectors into one
  SQLite store: (1) the LSP server `src/lsp.rs` — already running — logs
  didOpen/didChange + definition/references/hover requests, editor-portable,
  ~80% of the "seeking" signal; (2) thin per-editor shim for what LSP can't
  see: VS Code `onDidChangeActiveTextEditor`/selection/visibleRanges/
  windowState + `onDidStartTerminalShellExecution`; neovim BufEnter/
  CursorMoved/FocusGained. Prior art: wakatime/codetime do exactly (2).
