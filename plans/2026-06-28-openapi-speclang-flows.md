# OpenAPI `SpecLang` + flow participation overlay

Scope (chill): ONE trait `SpecLang`, ONE impl `OpenApiSpec`. Emit the spec's
request/response/schema graph as sym-keyed rows in the SAME shape as
`type_link`/`type_entity`, so the existing `closure()`/`reaches()` machinery
unifies the spec graph with the code graph (imports via `module_edge`, types via
`type_link`, calls via `call_edge`, dataflow via `df_edge`). The tie between a
code function and an op is a DSL rule the user writes ("where WE SAY a function
is tied to an opname"); the engine only supplies the spec nodes/edges + a generic
LSP overlay that reads a conventionally-named flow-membership relation.

Pattern source of truth: `ingest::IngestLang` (`src/ingest/mod.rs`) + its engine
refresh `refresh_doc_rels` (engine.rs:4033) + the `doc_ref` name-match bridge
(engine.rs:4063). `refresh_spec_rels` is a near-clone; the bridge is DSL-side.

Goal queries the design must answer:
- "is X on the path for the request/response of op Y" = `reaches(Y_root, X)` over
  the unioned `flow` edge relation.
- "all flows symbol X participates in" = `{ op | reaches(op, X) }`, surfaced in
  the LSP hover popover.

---

## 1. Type signatures

```rust
// src/spec/mod.rs  (new module, mirrors src/ingest/mod.rs)

pub struct SpecOp {
    pub sym: String,     // "{file}::op::{operationId}"  (fallback "{file}::op::{METHOD}_{sanitized_path}")
    pub method: String,  // "get" | "post" | ...
    pub path: String,    // "/users/{id}"
    pub opname: String,  // operationId, else "{METHOD} {path}"  (the human label + the name-bridge key)
    pub file: String,
    pub line: u32,       // 1-based source line of the operation (from the YAML/JSON span)
}

pub struct SpecSchema {
    pub sym: String,     // "{file}::schema::{ComponentName}"  (inline: "{file}::schema::{opId}.{role}")
    pub name: String,    // bare component name (the bridge key to type_entity.name)
    pub file: String,
    pub line: u32,
}

pub struct SpecEdge {
    pub src: String,     // op sym or schema sym
    pub dst: String,     // schema sym (resolved within-file by component name)
    pub kind: &'static str, // "param" | "request" | "response" | "field" | "ref"
}

#[derive(Default)]
pub struct SpecFacts {
    pub ops: Vec<SpecOp>,
    pub schemas: Vec<SpecSchema>,
    pub edges: Vec<SpecEdge>,
}

pub trait SpecLang: Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;          // shape sniff lives in extract, see note
    fn extract(&self, file: &str, content: &str) -> SpecFacts;
}

pub fn spec_langs() -> &'static [&'static dyn SpecLang] { &[&OpenApiSpec] }

struct OpenApiSpec;
impl SpecLang for OpenApiSpec { /* ... */ }
```

```rust
// src/engine.rs additions
const SPEC_RELS: [&str; 3] = ["spec_op", "spec_schema", "spec_edge"];
fn spec_rels_used(prog: &Program) -> bool { rels_used(prog, &SPEC_RELS) }
impl Engine {
    fn refresh_spec_rels(&self) -> Result<()> { /* clone of refresh_doc_rels */ }
}
```

LSP overlay seam (generic, not OpenAPI-specific):

```rust
// engine.rs — extend the existing hover()
// After resolving `text` -> syms, for each sym append a "flows" block IF the
// program materialized the convention relation `flow_member(member, op)`.
fn flow_overlay(&self, sym: &str) -> Option<String>;
//   reads rel_flow_member JOIN rel_spec_op? No — keep generic:
//   flow_member(member: text, op: text) where op is a spec_op sym; join spec_op
//   for the (method, path) label. Guard on table-exists so non-spec programs no-op.
```

## 2. Pseudo-code bodies

```rust
// OpenApiSpec::matches — extension prefilter only; real dispatch in extract.
fn matches(&self, p: &str) -> bool {
    p.ends_with(".yaml") || p.ends_with(".yml") || p.ends_with(".json")
}

// OpenApiSpec::extract
fn extract(&self, file, content) -> SpecFacts {
    // 1. Parse to serde_json::Value (YAML via serde_yaml::from_str -> Value,
    //    JSON via serde_json::from_str). serde_yaml round-trips into the json
    //    Value model, so one walker handles both.
    // 2. SHAPE SNIFF: require top-level "openapi" (3.x) or "swagger" (2.0) key;
    //    else return SpecFacts::default() (matches() is broad, this gates).
    //    -> keeps a plain config.yaml from minting bogus ops.
    // 3. components/schemas/{Name}: one SpecSchema each. For each property whose
    //    type is a $ref or inline object, push SpecEdge{schema, prop_schema, "field"}.
    //    allOf/oneOf/anyOf members -> SpecEdge{schema, member, "ref"}.
    // 4. paths/{path}/{method}: one SpecOp.
    //    parameters[].schema  -> edge(op, schema, "param")
    //    requestBody.content[*].schema (follow $ref) -> edge(op, schema, "request")
    //    responses[*].content[*].schema (follow $ref) -> edge(op, schema, "response")
    //    v1 collapses status codes into one "response" kind; v1.1 may keep status.
    // 5. $ref "#/components/schemas/Name" resolves to that component's sym
    //    WITHIN THE SAME FILE (no cross-file $ref in v1; external $ref -> bare name dst).
    //    Inline (non-$ref) request/response schemas synthesize a sym
    //    "{file}::schema::{opId}.{role}" and still emit a SpecSchema so they're nodes.
    // line numbers: serde_yaml 0.9 does not expose spans on Value. Parse the
    //    file a SECOND time with tree_sitter_yaml (already a dep) and walk for
    //    the block_mapping_pair whose key text == operationId / component name;
    //    its start row + 1 is the line. Two views of one file joined by key text;
    //    line is display-only (not identity), so an ambiguous key is harmless.
}
```

```rust
// refresh_spec_rels — clone of refresh_doc_rels (engine.rs:4033)
//   SELECT repo, path, rev FROM _file WHERE path LIKE '%.yaml' OR '%.yml' OR '%.json'
//   par_iter: pick spec_langs().find(matches), read_content, lang.extract(...)
//   flatten ops -> spec_op rows; schemas -> spec_schema rows; edges -> spec_edge rows.
//   dedup each by sym (ops/schemas) / (src,dst,kind) (edges).
//   self.refresh_rel("spec_op", &["sym","method","path","opname","file","line"], rows)
//   self.refresh_rel("spec_schema", &["sym","name","file","line"], rows)
//   self.refresh_rel("spec_edge", &["src","dst","kind"], rows)
//   NO corpus-global resolution pass ($ref is within-file), NO repo-qualified sym
//   rewrite needed for v1 (single-spec assumption; revisit if two specs collide).
```

```rust
// flow_overlay(sym) — generic spec-flow membership for hover
//   if !table_exists("rel_flow_member") { return None }
//   SELECT o.method, o.path, o.opname
//     FROM rel_flow_member m JOIN rel_spec_op o ON o.sym = m.op
//     WHERE m.member = ?sym
//   -> "Participates in flows:\n- GET /users/{id}\n- POST /orders"
//   (dedup, sort). None when empty.
```

DSL the user writes to wire it (NOT engine code — this is the "we say" tie):

```
rel flow(src: text, dst: text, kind: text).
flow(s, d, k)        <- spec_edge(s, d, k).         // request/response/field/ref
flow(s, d, k)        <- type_link(s, d, k).         // code type graph
flow(s, d, "import") <- module_edge(s, d).          // import tree (src, dst)
flow(s, d, "call")   <- call_edge(s, d, _).         // call graph (caller, callee, kind)
flow(s, d, "df")     <- df_edge(s, d).              // intra-proc dataflow (from, to)
// the tie: operationId == handler function name
flow(op, fn, "handler") <- spec_op(op, _, _, name, _, _),
                           type_entity(fn, name, "function", _, _, _).
// schema <-> code data type by name
flow(sch, ty, "binds")  <- spec_schema(sch, name, _, _),
                           type_entity(ty, name, _, _, _, _).
reaches(a, b) <- closure(flow).
// flow membership for the LSP overlay: member reachable from an op
rel flow_member(member: text, op: text).
flow_member(x, op) <- spec_op(op, _, _, _, _, _), reaches(op, x).
```

### 2a. Node-space caveat (the integration risk)

The unioned `flow` mixes FOUR id spaces. `reaches()` only crosses between them if
bridged, else the default recipe ships a closure with disconnected islands (a
silent no-op for "follow the import tree / data flow from a handler"):

| edge rel     | node id space                          |
|--------------|----------------------------------------|
| `spec_edge`  | spec syms `{file}::op|schema::{name}`   |
| `type_link`  | code syms, repo-qualified `{repo}::{sym}` |
| `call_edge`  | code syms (verify: SAME repo-qualification as type_link?) |
| `module_edge`| FILE PATHS                             |
| `df_edge`    | df-node ids `file:line:col`            |

Required bridge rules in the shipped recipe (without these the union is islands):
```
flow(fn, file,  "in_file") <- type_entity(fn, _, _, _, file, _).   // sym -> path, lets module_edge continue
flow(fn, dfid,  "owns")    <- df_node(dfid, _, _, fn, _, _).        // fn sym -> its df nodes, lets df_edge continue
```
Open verification for step 2: confirm `call_def`/`call_edge` syms carry the SAME
`{repo}::` qualification as `type_entity`/`type_link` (refresh_call_rels vs
refresh_type_rels). If they diverge, the handler bridge connects spec->type but
NOT spec->call; add a normalization or a `call_def`<->`type_entity` sym bridge.
The step-4 e2e MUST assert a cross-space reach (op -> handler fn -> a callee in
another fn, or op -> request schema -> code struct -> a field type) actually
returns rows — that test is the guard against shipping islands.

## 3. Instance lifetimes (types that hold state)

- `OpenApiSpec` — zero-size unit struct, `'static` in the `spec_langs()` registry
  slice. Same lifetime story as `MarkdownDoc`/`RustTypes`. No per-file state; the
  serde `Value` tree is a local in `extract`, dropped at return.
- `SpecFacts`/`SpecOp`/`SpecSchema`/`SpecEdge` — short-lived, one per file inside
  `refresh_spec_rels`'s `par_iter` collect; flattened into `Vec<Vec<Value>>` and
  dropped before the `refresh_rel` write.
- `spec_op`/`spec_schema`/`spec_edge` rows — persist in SQLite (`rel_*` tables),
  rebuilt each tick by `refresh_spec_rels`; lifetime = the db. Same as `doc_node`.
- `flow`/`reaches`/`flow_member` — derived rel tables, materialized by the
  fixpoint each tick, read by the daemon's `hover` between ticks.

## 4. Storage layout, read/write sequence, uniqueness

Storage (new built-in source rels, declared in the `RelDecl` block ~engine.rs:264,
guarded against user redefinition like `doc_node`):

| rel          | cols                                          | key (uniqueness)        |
|--------------|-----------------------------------------------|-------------------------|
| `spec_op`    | sym, method, path, opname, file, line         | `sym`                   |
| `spec_schema`| sym, name, file, line                         | `sym`                   |
| `spec_edge`  | src, dst, kind                                | `(src, dst, kind)`      |

Write sequence (per tick, mirrors type/doc rels):
1. Cold build path: in the same block as `refresh_type_rels`/`refresh_doc_rels`
   (engine.rs ~1822), add `if spec_rels_used(prog) { refresh_spec_rels()? }`.
2. `--changed` path: in the `files_changed` block (engine.rs ~2087), add the
   same call + `for r in SPEC_RELS { changed_source_rels.insert(...) }` so
   derived `flow`/`reaches` re-fire when a spec file edits.
3. `refresh_spec_rels`: SELECT spec files from `_file`, parallel extract, dedup,
   three `refresh_rel` calls (one batched write each — no N+1).
4. Fixpoint then evaluates the user's `flow`/`reaches`/`flow_member` rules over
   the fresh source rows (existing machinery, no change).

Read sequence (LSP hover, daemon, between ticks):
1. `handle_hover` -> `eng.hover(file, text)` (lsp.rs:296) — unchanged entry.
2. `hover` resolves `text` -> syms via `type_entity`/`call_def` (unchanged), then
   for each sym calls `flow_overlay(sym)` and appends its block when `Some`.
3. `flow_overlay` reads `rel_flow_member ⋈ rel_spec_op`; table-exists guard makes
   it a no-op for non-spec programs (zero overhead, like `type_profile_overlay`).

Uniqueness / correctness conditions:
- `spec_op.sym` collision: two ops with no operationId on the same (method, path)
  cannot exist in a valid spec; the `{METHOD}_{path}` fallback is unique per file.
  Two SPEC FILES sharing a component name DO collide on schema sym in v1 — note
  as the single-spec assumption; the multi-spec fix is repo/file-qualified syms
  exactly like `refresh_type_rels`'s `{repo}::{sym}` (deferred).
- `spec_edge` external `$ref` -> bare name dst (unresolved leaf), same stance as
  `type_link`'s unresolved `edge.to` fallback, so the node still appears in
  `reaches`.
- One rel = one rule kind: `spec_op`/`spec_schema`/`spec_edge` are SOURCE rels
  (engine-populated). The user's `flow`/`flow_member`/`reaches` are DERIVED. Never
  head a spec_* rel with a DSL rule (the mixed-source/derived bail catches it).
- No N+1: three `refresh_rel` batched writes, parallel extract. The tick counter
  must stay quiet.

---

## Step list (sequenced, chill)

1. `src/spec/mod.rs`: `SpecLang` trait + `SpecFacts`/`SpecOp`/`SpecSchema`/`SpecEdge`
   + `OpenApiSpec::{matches,extract}` (serde_yaml/json -> Value walker, shape
   sniff, components + paths). Unit tests in-module (fixture spec string), mirror
   `ingest::tests`.
2. `engine.rs`: declare the 3 source rels (`RelDecl` + reserved-name guard +
   `SPEC_RELS`/`spec_rels_used`), add `refresh_spec_rels` (clone `refresh_doc_rels`),
   wire both refresh call-sites (cold + `--changed`).
3. `engine.rs`: `flow_overlay` + append into `hover`; table-exists guard.
4. e2e test `tests/it/openapi_flows.rs`: a tiny spec + a 2-file code sandbox; the
   DSL wiring above; assert `flow_member` ties a handler fn to its op and that a
   schema field type is `reaches`-able from the op. Optional: assert hover markdown
   contains "Participates in flows".
5. `examples/openapi-flows.dl`: the DSL wiring as a shipped recipe.

Deferred (explicitly out of v1): role split (request vs response) in the overlay;
status-code-keyed responses; cross-file/external `$ref`; multi-spec sym
qualification; Avro/SQL `SpecLang` impls (the trait already admits them).

## Decisions (locked 2026-06-28)
- **Overlay seam = convention rel.** Engine reads a fixed-name DSL rel
  `flow_member(member, op)` the program materializes; engine knows nothing about
  OpenAPI binding semantics. `flow_overlay` joins `rel_flow_member ⋈ rel_spec_op`,
  table-exists-guarded. Smallest engine change, most flexible.
- **Spans = tree-sitter-yaml.** `OpenApiSpec::extract` parses with
  `tree_sitter_yaml` (already a dep) for real node positions on ops/schemas, not
  a key-text line scan. More code, accurate `line`, and opens a later path to
  feeding op/schema byte-spans into the `_where_bytes`/`ref` spine (so an op could
  itself be a `--move`/edit coordinate). NOTE: still parse to `serde_*` Value for
  the structural walk; use the tree-sitter tree ONLY to recover the line of a
  given key (operationId / component name) — two views of the same file, joined
  by key text. (Revisit if double-parse cost matters; specs are small.)
- **Default `flow` recipe includes import/call/dataflow.** The shipped
  `examples/openapi-flows.dl` unions `spec_edge ∪ type_link ∪ module_edge ∪
  call_edge ∪ df_edge` plus the handler/binds bridges, so `reaches()` follows the
  full import tree and data flow from a handler, not just the type graph. Accept
  the broader closure; the overlay can later filter by edge kind if noisy.
