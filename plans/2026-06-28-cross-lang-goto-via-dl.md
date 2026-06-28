# Cross-language goto / refs / impls driven by dl codegen-rhythm pages

North star: a spec (OpenAPI) codegens client+impl code across langs/repos; the
codegen naming rhythm (the operationId verbatim, or a per-lang transform of it)
is captured as dl rules on a PAGE; the LSP then does cross-lang goto-def /
references / implementation / declaration with file paths + transitive closures,
including jumping into the contract repo on disk. Bias: express the linking in dl,
keep bespoke Rust small and bounded.

## What already exists (the load-bearing surprise)

- **references = ref-spine string identity** (lsp.rs:317). Cursor -> innermost
  located span -> `StringId` -> `eng.string_spans(id)` = EVERY located span
  interning that exact string, across files. The codegen rhythm IS this: an
  interned `operationId` ties every generated file together. Cross-lang refs work
  today with zero new code, provided the token is byte-identical across langs.
- **definition is already DSL-driven** (engine.rs:1438 `definition_targets`). It
  reads a user relation **`def_target(name, file, line)`** by column name (flexible
  ordering, like `diag`), falling back to `module_edge` segment match. So goto-def
  follows dl-expressed links: write the rule, the LSP uses it.
- **diagnostics** (`diag`) and **hover** (type_entity/call_def auto + planned flow
  overlays) are the same convention-relation pattern.
- **multi-repo coordinate** exists: `scan("*")` fans over every config repo, `_file`
  keyed on (repo, path, rev), `repo_roots()` maps repo -> on-disk root. The
  "contract repo on disk" is just another config repo in view.
- **call_edge is now cross-lang** (Rust+Kotlin+TS), so client call sites in any of
  the three are first-class graph rows.

## The capability map

| Capability | Mechanism | Bespoke Rust |
|---|---|---|
| refs (callsite <-> all occurrences, cross-lang) | ref-spine `string_spans` | none (works now) |
| goto-definition (callsite -> impl) | `def_target(name,file,line)` dl rule | none (handler exists) |
| goto-implementation (iface -> all impls) | new `impl_target` handler | small (copy `definition_targets`) |
| goto-declaration | new `decl_target` handler | small (same copy) |
| transitive closure (refs-of-refs, impl chains, spec reach) | `reaches()` over xref + a hover overlay | dl + small overlay |
| cross-repo paths (contract repo on disk) | repo-aware path resolution in LSP | small (`root.join` -> `repo_roots`) |
| codegen-rhythm capture (per-lang call shapes, camel<->snake) | the .dl page | none (pure dl) |

## The codegen-rhythm page (pure dl)

```
# One engine over spec repo + client repos + impl repo (multi-repo).
rel xref(name: text, file: file, line: int, role: text).

# spec operationIds
xref(op, f, l, "spec") <-
  scan("*", "**/openapi.{yaml,json}", p, rev),
  jsonp(p, rev, "paths.*.*.operationId", op), <locate op -> f,l>.

# client call sites: callee name == operationId (TS + Kotlin, now that call_edge
# is cross-lang). call_def gives the site's file/line; call_edge confirms it's a call.
xref(op, f, l, "client") <- call_def(sym, _, f, l, _), call_name(sym, op), spec_op(op).

# backend impl defs: a function whose name == operationId.
xref(op, f, l, "impl") <- type_entity(_, _, op, "function", _, f, l), spec_op(op).

# what goto-def reads: from any callsite on `op`, jump to its backend impl.
def_target(op, file, line) <- xref(op, file, line, "impl").

# what goto-impl reads (after the new handler lands): iface -> every implementor.
impl_target(iface, file, line) <- type_edge(impl, iface, "impl"),
                                  type_entity(_, _, impl, _, _, file, line).

# rhythm normalization when spellings differ (TS camelCase vs RS snake_case):
#   define a canonical(op_ts, op_rs) mapping rule, then key def_target by BOTH
#   spellings -> the literal-identity refs still work per-lang, and goto crosses.
```

Where the rhythm is NOT a literal shared string, the page's normalization rules
ARE the bespoke part — but they live in dl, per codegen convention, not in Rust.

## Bounded bespoke Rust (the only engine work)

1. **`textDocument/implementation` handler** + `eng.implementation_targets(file, text)`
   — a near-verbatim copy of `handle_definition` / `definition_targets` reading
   `impl_target(name, file, line)`. Register the capability in `lsp.rs`
   `ServerCapabilities` (add `implementation_provider`).
2. **`textDocument/declaration`** (optional, same copy reading `decl_target`).
3. **Cross-repo path resolution**: `handle_definition`/`handle_references` do
   `root.join(&path)`, assuming one root. For a target in another config repo,
   resolve via `eng.repo_roots()` (path is repo-relative; pick the owning repo
   root). Needed for "goto into the contract repo on disk". Touches `path_to_uri`
   call sites in lsp.rs + likely a repo column threaded through `string_spans` /
   `def_target`.
4. **Transitive-closure overlay** (optional): extend `hover` to append a "reaches"
   block from a convention rel (same seam as the planned `flow_member` overlay) so
   hovering a callsite shows the impl + its downstream reach as a markdown list of
   links.

## Open dependency / decision
- Cross-repo goto needs the `file` column in `def_target`/`impl_target`/the ref
  spine to be resolvable to a repo root. Decide: thread a `repo` column through
  these relations, or make `file` an absolute path at the LSP boundary. (The ref
  spine stores content FileId, not path; `string_spans` already returns a
  repo-relative path — confirm it's the RIGHT repo's relative path under multi-repo
  before wiring cross-repo join.)
- This composes with `2026-06-28-openapi-speclang-flows.md` (spec graph) and
  `2026-06-28-entry-point-dataflow-fidelity.md` (slice/closure). The spec rels
  there make the `xref(...,"spec")` rule structural instead of regex.

## Build order
1. Cross-lang `call_edge` — DONE (Kotlin+TS landed).
2. The codegen-rhythm page on a real spec+client+impl multi-repo fixture; confirm
   refs already cross langs and `def_target` drives goto-def callsite->impl.
3. `implementation` handler + `impl_target` (the iface->impls goto).
4. Cross-repo path resolution (goto into the contract repo).
5. Closure overlay in hover + a d2/atlas of the xref graph.
6. Oracle: SCIP (scip-typescript / scip-java / rust-analyzer) confirms the
   cross-lang def/ref/impl hits — precision (our hits subset of SCIP) + recall.
