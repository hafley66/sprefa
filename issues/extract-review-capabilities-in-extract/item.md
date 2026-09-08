---
created: 2026-09-08
updated: 2026-09-08
type: feature
reporter: fable
assignee: chris
status: open
priority: high
epic: extract-port-closeout
related: ['@extract-cfg-exception-edges', '@dl6-acquire-release-pairing', '@extract-dependency-corpus', '@extract-lsp-door']
labels:
- pkg:extract
- review-field-test
---

# extract: review capabilities as fact families (scope columns, smell family), fast tier install-free; answers to the CodeQL comparison

## Description

## Description

Answers to five questions on the 2026-09-08 CodeQL comparison (research/2026-09-08-fact-sources-beyond-scip.md §8), and the decision they produce: the review capabilities go into `extract` proper as fact families computed in Rust, so nothing waits on a QL or dl6 rule set. Fast tier stays install-free; slow tier may read dependencies.

### Q1. "stock rule set: 87 rules, 0 findings. Huh?"

CodeQL's default `codeql/javascript-queries` suite is security-shaped: injection, XSS, prototype pollution, regex DoS, path traversal. It ran 87 rules over the fixed package in 11 s and reported nothing. Every hit in the comparison came from five hand-written queries. Neither CodeQL's shipped rules nor extract ships detectors for the shapes this review needed (duplicated cleanup, unpaired acquire, listener never removed, require in ESM). "Draw" meant: no default vocabulary on either side.

### Q2. "40 lines of jq: how to fold that into extract proper, not QL"

The jq did four things: filter `cst` rows to `try_statement` / `finally_clause`; take `site` rows for calls; join by span containment; group by `receiver.method` across nested finallys. Each step is a column or a family extract can emit itself:

| jq step | extract fact that replaces it |
|---|---|
| span containment join | `scope` columns on every `site` / `node` row: ids of the enclosing `try`, `catch`, `finally`, function, loop. Containment becomes equality on a column |
| filter to try/finally | same columns; no cst scan |
| group duplicated cleanup | `smell` family: shape detectors computed in Rust from cst + scope + cfg, one row per hit citing spans |
| "runs twice on the normal path" | `cfg_postdom` from @extract-cfg-exception-edges |

### Q3. "acquire released on every path: big gap"

Yes. Closed by three pieces, all in extract: exception edges + post-dominance (@extract-cfg-exception-edges), the `scope` columns above, and a `smell.acquire_unpaired` detector that reads the variable binding from the df family (acquire site s bound to v; release site r on v inside finally F of try T; unpaired when s is outside T's body and a call site sits between s and T in the same block, or when no r exists). That is the exact predicate the QL used, ~15 lines of Rust over rows extract already has. @dl6-acquire-release-pairing stays as the rule-shaped twin for users who want to parameterise the pairs.

### Q4. "inferred types: what?"

SCIP stores, per symbol, the compiler's hover text as a markdown string (`documentation`). extract streams it as `scip_documentation`. The "inferred `any`" finding was a grep for `\bany\b` over those strings: it worked, and it is string-shaped. CodeQL's TypeScript extractor runs the checker and stores each expression's type as rows, so `expr.getType()` is relational. TypeScript 7 removed the JS compiler API; the structured door for extract is the `@typescript/api` preview on tsgo (@extract-lsp-door's `inlayHint` is the string-shaped fallback for every other language).

### Q5. "dependency corpus: fast tier must not need builds or installs"

Agreed, recorded as the tier rule: fast (`--family diet_scip`, per-file families, `smell`, `scope`, `cfg`) reads only the files handed to it and never spawns a toolchain; slow (`--family scip`, `--corpus`, the IR doors) may run an indexer and read `node_modules` or a target dir. CodeQL's extractor installs nothing either; it reads whatever `node_modules` is on disk. @extract-dependency-corpus is slow-tier only.

### License

sprefa is `MIT OR Apache-2.0` (Cargo.toml:21, v6/sprefa-extract/Cargo.toml:16); the analysis matrix said "Apache" and now says the dual form. The SCIP indexers are Apache-2.0.

## Plan (types first)

```rust
// scope columns, phase-1, every language: emitted on node/site rows
pub struct Scope { pub try_id: Option<NodeId>, pub catch_id: Option<NodeId>, pub finally_id: Option<NodeId>, pub fn_id: Option<NodeId>, pub loop_id: Option<NodeId>, pub block_id: NodeId, pub block_index: u32 }
// pseudo: one stack walk per file over the cst; push on try/catch/finally/function/loop/block entry, pop on exit; each emitted row copies the top of each stack.

// smell family, phase-1 unless noted
pub enum Smell {
  DuplicatedCleanup { call: Span, inner_finally: NodeId, outer_finally: NodeId, text: String },
  AcquireUnpaired { acquire: Span, var: String, release: Option<Span>, try_id: Option<NodeId>, reason: Unpaired /* NoRelease | BeforeTry { between: Span } */ },
  ListenerUnremoved { add: Span, target: String, event: String },        // add without any remove on the same target text in the corpus given
  RequireInEsm { call: Span },                                           // require() in a file with import declarations
}
// pseudo, DuplicatedCleanup: for finally F_in with enclosing try T_in whose ancestor chain has try T_out with finally F_out:
//   for call c_in in F_in, call c_out in F_out with text(c_in) == text(c_out): emit.
// pseudo, AcquireUnpaired: from df binding (var v <- call acquire at s): find release calls on v; r in finally F of T:
//   if s not in body(T) and exists call between s and T in block(s): BeforeTry; if no r: NoRelease.
```

Lifetimes: `Scope` stacks live for one file parse; `Smell` rows are pure functions of that file's rows (ListenerUnremoved is corpus-scoped: computed at the end over the files given, no state beyond a set of (target, event) seen). Storage: none; both stream as JSONL like every other family. Uniqueness: one `Smell` row per (kind, primary span).

Pair table for `AcquireUnpaired` and `ListenerUnremoved` ships as a default list (acquire/release, subscribe/unsubscribe, addEventListener/removeEventListener, open/close, lock/unlock) overridable by `--pairs a:b,...`.

## Acceptance Criteria

- [ ] `scope` columns present on `site` and `node` rows for ts, rust, go, python, kotlin fixtures; `block_index` orders siblings
- [ ] `extract --family smell FILE...` reproduces the CodeQL yardstick: research/2026-09-08-codeql-review-queries/fixture rows 3 / 1 / 1 / 1 and the fixed vitest-playwright package rows 0 / 0 / 1 / 0
- [ ] fast tier receipt: `smell`, `scope`, `cfg` run with no toolchain on PATH (test unsets PATH except the extract binary)
- [ ] `--pairs` override honoured
- [ ] `--schema` documents `scope` columns and the `smell` record

## Tests Run

- [ ] fixture run per language
- [ ] tier receipt

## Implementation Notes

- [ ] scope stack walk lands first; smell detectors read it; cfg post-dominance joins later for the "runs twice on the normal path" refinement of DuplicatedCleanup
