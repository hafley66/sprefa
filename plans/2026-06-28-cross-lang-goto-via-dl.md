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

---

## Captured ideas (2026-06-28, scoped, not yet scheduled)

### I1 — case / rhythm normalization (`get_users` ~ `getUser`)
Codegen spells the same op differently per lang: spec/TS `getUser` (camel), Rust
`get_user` (snake), Kotlin `getUser` (camel). Matching needs a canonical key.
- Add a dl string builtin `canon(s)` = lowercase + strip non-alphanumeric
  (`getUser`/`get_user` -> `getuser`). Sits beside the existing `split`/`replace`/
  `int` builtins (engine.rs ~6304). Small, generic, reusable.
- The page keys `xref`/`def_target`/`impl_target` by `canon(name)`.
- For the LSP lookup to be case-insensitive, `definition_targets` /
  `implementation_targets` must `canon` the cursor `text` before the
  `WHERE name = ?` query (~3 lines Rust each). Until then, prove at the relation
  level via CLI `?` queries.
- Plural/stem drift (`getUsers` vs `getUser`) is NOT covered by canon (different
  letters); leave fuzzy stemming out of v1, flag if the real corpus needs it.

### I2 — proof fixture: Kotlin interface-hiding + N impls + delegation
The validation target. One spec op `getUser`; clients/impls spread as:
- spec `openapi.{yaml,json}` with `operationId: getUser`
- TS frontend client call site `getUser(...)`
- Kotlin `interface Api { fun getUser(...) }` + TWO impls (one direct, one via a
  Kotlin `by` delegate) — the "interface hiding / DI-style soup" shape
- a Rust `fn get_user(...)` impl (exercises I1 snake-case folding)
Prove (CLI `?` first, LSP after): refs cross all langs on the canon key;
`impl_target(Api.getUser)` returns BOTH Kotlin impls (the interface-hiding goto);
`def_target` jumps a TS call site -> the backend impl; canon matches the Rust
`get_user`. This is the e2e that guards the whole cross-lang nav story; pair with
the scip-java/scip-ts oracle once it lands.

### I3 — multi-checkout / worktree identity
A user may have N checkouts of ONE repo, plus M worktrees off those checkouts.
Current model: `_file` keyed on (repo, path, rev); `repo` is a slug from the
nearest `.git`/config; the ref spine is content-addressed (`FileId` = blake3), so
byte-identical files across checkouts collapse to ONE `FileId`. Consequences to
decide:
- references via `string_spans` would MERGE spans across identical checkouts (find
  every occurrence everywhere) — feature for "all refs", hazard for "which
  checkout am I in". Goto should prefer the cursor's own checkout root.
- repo slug collision: two checkouts of the same repo at different roots — same
  slug (merge) or distinct (per-root)? Worktrees share `.git` (linked) — detect
  via `git rev-parse --git-common-dir` to group worktrees under one logical repo.
- The cross-repo path-resolution work (item 3 above) must pick the RIGHT root per
  result; the `repo` column on `def_target`/the spine is the lever. Decision still
  open: thread `repo` through, or hand the LSP absolute paths.

### I4 — DSL-driven hover sections (LSP control from a dl page)
Generalize the hover overlay the same way `diag` and `def_target` already work:
a convention relation the page populates, the handler appends. Today `hover`
(engine.rs:1488) hard-synthesizes from `type_entity`/`call_def`. Add a seam:
- `hover_section(name: text, title: text, body: text)` (or per-kind
  `ref_section`/`type_section`/`impl_section`) — read by `eng.hover` by column
  name, appended as markdown blocks (links + lists) after the auto sections.
- Then a page adds, e.g., a "Participates in flows" / "Implementations" /
  "Contract" section purely in dl — no Rust per section. This is the same
  convention-rel decision as `flow_member`; unify them: hover reads any
  `*_section(name, title, body)` rels the program declares.
- Composes with the OpenAPI flow overlay (`flow_member`) and the impl/def nav:
  one hover shows refs + impls + flows + contract link, all dl-authored.
