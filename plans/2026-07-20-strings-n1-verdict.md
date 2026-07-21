# `_strings` INSERT N+1: verdict + rail relabel (v11)

Date: 2026-07-20. Branch `v11-strings-n1` off v11 tip `4fb637d3`.
Two jobs. Job 1 (this file's spine): does the `_strings` INSERT N+1 blow up?
Job 2 (second half): kill the `format!`-hashed node identity in dataflow.

---

## JOB 1 — VERDICT: BOUNDED. It does not blow up.

The `[n+1] 'INSERT _strings' ran Nx` count is the SUM of several batched,
structural flushes that shared one bump key. Split by call site, every term is
O(program structure) or O(fixpoint depth). NONE is per-row. No coefficient on
corpus rows R anywhere.

### Scaling law (measured, release binary, scratch db in-worktree)

Let F = derived components in the program, Rels = rels materialized this tick,
D = fixpoint depth (max recursion passes of the deepest recursive component),
B = source batches (ceil of source rows / stage batch), R = corpus source rows.

    count(INSERT _strings) =
        (fixpoint/pass)      ~ D - 1      per recursive component
      + (fixpoint/component) ~ F          seed + final + non-rec + mirror drains
      + (encode/rel)         ~ Rels       one per rel materialize
      + (spine/source)       ~ B          source spine + where-bytes + call-graph
      + (plain INSERT _strings) = 0       genuine per-row leak sentinel

R appears with coefficient ZERO in every term. `spine/source` grows with B =
R / batch_size (chunked, sub-linear, never per-row).

### Receipts

Adversarial minting program (`/tmp/n1scratch/mint.dl`): a recursive rule whose
head BUILDS a new trail string each pass, so the per-pass flush is non-empty.

Sweep A — DEPTH (1 chain, vary depth D), fixed rule structure:

| depth D | trail rows | (fixpoint/pass) |
|--------:|-----------:|----------------:|
| 10      | 55         | 9               |
| 20      | 210        | 19              |
| 40      | 820        | 39              |
| 80      | 3240       | 79              |
| 160     | 12880      | 159             |

`(fixpoint/pass) = D - 1`, exactly. Linear in fixpoint depth.

Sweep B — CORPUS (vary chains K, FIXED depth 4), rule structure fixed:

| chains K | edges | trail rows | (fixpoint/pass) | (spine/source) | PLAIN |
|---------:|------:|-----------:|----------------:|---------------:|------:|
| 50       | 200   | 500        | ~D (≤1 shown)   | 0              | 0     |
| 400      | 1600  | 4000       | ~D              | 0              | 0     |
| 2000     | 8000  | 20000      | ~D              | 8              | 0     |

An 8x corpus increase (500 -> 4000 -> 20000 rows) at fixed D left the per-pass
count flat at ~D. `spine/source` moved 0 -> 8 (chunked source batches, B), never
per-row. PLAIN `INSERT _strings` = 0 in every configuration: no per-row leak.

Real corpus — sprefa `.dl/` cold build (scratch db), former single `INSERT
_strings` = 143 now attributes as:

| label                    | count |
|--------------------------|------:|
| spine/source             | 75    |
| fixpoint/component       | 37    |
| encode/rel               | 23    |
| fixpoint/pass, spine/call| < shortlist |
| **plain INSERT _strings**| **0** |

The old scream mislabeled this legitimate O(program)+O(depth) work as a per-row
loop. (Aside: the real loudest key on this corpus is `_reldigest` = 265, a
separate pre-existing N+1, out of scope.)

### The fix (durable deliverable): distinct labeled bump keys

`Db::flush_syms` / `flush_pending_syms` gained keyed variants
(`flush_syms_keyed`, `flush_pending_syms_keyed`) that thread a caller-chosen
bump key into `insert_rows_keyed("_strings", key, ...)`. Every structural flush
site now names its scaling axis:

- `INSERT _strings (fixpoint/pass)`      derive.rs seminaive + naive per-pass drains — O(D)
- `INSERT _strings (fixpoint/component)` seed / final / non-recursive / native-walk / mirror drains — O(F)
- `INSERT _strings (encode/rel)`         `encode_rel_rows` per rel materialize — O(Rels)
- `INSERT _strings (spine/source)`       source spine, module spans, where-bytes — O(B)
- `INSERT _strings (spine/call)`         SQLite call-graph owner/site interns — O(B)
- `INSERT _strings` (plain, unlabeled)   reserved: a genuine per-row flush; STILL screams

`tick_end`'s scream filter excludes `SELECT ` keys (as before) AND now
`INSERT _strings (` labeled variants. The plain `INSERT _strings` key is bumped
by no production caller (all route through keyed variants), so it is a clean
per-row-leak sentinel. New unit test
`db::tests::strings_n1_rail_screams_on_plain_key_not_labeled_variants` locks it:
a labeled flush past the threshold stays silent; a plain per-row flush screams.

Threshold was NOT raised (N1_THRESHOLD stays 64). A real leak still screams.

Also added `DL_PROFILE_TOPN` (default 5), sibling of `DL_PROFILE_SQL_MS`, to
widen the profile shortlist for attribution sweeps.

### Per-family batching from worktree-agent-aae639dd9de4689b8: DROPPED

That branch batched `encode_rel_rows` per family (O(Rels) -> O(families),
measured 140 -> 136, marginal). With the relabel, `encode/rel` is correctly
labeled and excluded from the scream, so the batching is not needed to silence a
false alarm. It is a constant-factor INSERT-count optimization only (23 flushes
on the sprefa corpus), for extra code. Redundant for the rail; not ported.

### API / storage / uniqueness (planning protocol)

    // db.rs
    pub fn flush_syms(&self, sink) -> Result<usize>              // = keyed("INSERT _strings")
    pub fn flush_syms_keyed(&self, sink, bump_key: &str) -> Result<usize>
    pub fn flush_pending_syms(&self) -> Result<usize>            // = keyed("INSERT _strings")
    pub fn flush_pending_syms_keyed(&self, bump_key: &str) -> Result<usize>

Storage layout: unchanged. `_strings(id INTEGER PRIMARY KEY, content TEXT)` is
still one batched INSERT OR IGNORE per flush. Only the N+1 COUNTER key changed;
no SQL, no schema, no row shape moved. Uniqueness of `_strings` rows is
unchanged (id = StringId, INSERT OR IGNORE dedups).

---

## JOB 2 — dataflow node identity normalization

### The defect

`src/graph/typegraph/mod.rs:723` (+ ts twin `ts/flow.rs:17`, rebuild
`mod.rs:751`, template `ts/text.rs:126`): the node identity is
`format!("{file}:{line}:{col}:{kind}")`, and the stored join key is
`StringId::of(that)` (`src/engine/extract/dataflow.rs:100`, the `nid` closure) —
a 64-bit blake3 hash of an interpolated composite string, used as identity for
all of `df_node.id`, `df_edge.from/to`, `df_arg.call/arg`, `df_field.id/value`,
`df_param.id`, `df_lit.id`, `df_node_repo.id`, `nest.call_id`,
`template_parts.node`, and the `_rev` twins. Unlike `_strings`, this hash has NO
dictionary row and NO collision check: a 64-bit collision across 282,109 live
coordinates silently aliases two nodes.

### Decision: DENSE SURROGATE via a persistent coordinate dictionary. Receipts.

Three candidates weighed against this repo's real constraints:

**(A) Dense sequence with no dictionary — REJECTED.** The cold-chunk shard
append (`src/engine/cold_stage.rs:398-407` -> `refresh_dataflow_rels_slice` ->
`append_dataflow_rows`, `INSERT OR IGNORE`, crash-resumable disjoint file
slices, comment `extract/dataflow.rs:355-359`) has no corpus-global sequence. A
bare sequence id is assignment-order-dependent, so a crash-resumed or re-run
slice would mint different ids for the same coordinate. Ruled out exactly as the
mandate anticipated.

**(B) Composite key (carry file,line,col,kind on every id column) — REJECTED.**
Correct identity, zero collision, no dictionary. But it explodes the QUERY
SURFACE: `df_edge(from,to)` -> 8 columns, `df_arg(call,pos,arg)` -> ~10, and
every `.dl` join `df_edge(a,b), df_node(id:a,...)` across std/flow.dl,
std/entry.dl, std/strings.dl, 8 `.dl` rails, ~15 examples, the vscode flow-panel
preset, and ~243 df tests would rewrite to the multi-column form. The user's own
note: "8 cols vs 2 and I think worse." Rejected on blast radius.

**(C) Dense surrogate from a persistent `_df_node_dict` — CHOSEN.** A dictionary
table keyed `UNIQUE(file, line, col, kind)` assigns a dense
`INTEGER PRIMARY KEY AUTOINCREMENT` id per distinct coordinate. This is "the same
discipline `_strings` uses, keyed on the coordinate TUPLE." It survives the
cold-chunk append because it is content-keyed (INSERT OR IGNORE on the tuple, a
re-run slice finds the same row) and consulted at WRITE time on the engine's
single serial connection (parsing is parallel, row writes are not). Zero
collision by construction (dense assignment, no hash of a concatenation).

Why (C) is CONTAINED (the receipts that make it landable, not (B)):
- The write seam is ONE line: `nid` at `extract/dataflow.rs:100`.
- Display already reconstructs by LOOKUP, not by parsing the id:
  `coord_reconstruct` (`src/lower.rs:44-51`) is
  `SELECT ... FROM rel_df_node WHERE id = <cell>` — it reads the coordinate
  COLUMNS of the looked-up row. A surrogate id reconstructs identically. No
  display change.
- Readers join by id EQUALITY and never parse the id (inventory across
  std/.dl/examples/vscode). Zero `.dl`-surface change, zero vscode change.
- Every referenced id IS a df_node (measured on the sprefa corpus: 0 orphan
  df_lit.id / df_edge.from / df_edge.to / df_arg.call/arg / template_parts.node
  out of 282,109 nodes / 259,629 edges / 13,123 lits). So the surrogate assigned
  to each node's coordinate resolves every reference; the dict is built from node
  coordinates alone (which carry structured file/line/col/kind).
- Raw df references are intra-file (extraction is per-file `DataflowFacts`;
  interproc flow is a `.dl` join, not a raw edge), so a slice is self-contained
  and the persistent dict only has to keep a coordinate's id stable across the
  cold-slice write and a later wholesale refresh — which INSERT OR IGNORE gives.

### Signatures / storage / uniqueness (planning protocol)

    // meta table (created in ensure_meta, persistent, never rev-scoped)
    CREATE TABLE IF NOT EXISTS _df_node_dict (
      id   INTEGER PRIMARY KEY AUTOINCREMENT, -- dense node surrogate
      file INTEGER NOT NULL,   -- StringId::of(path)   (== df_node.file)
      line INTEGER NOT NULL,
      col  INTEGER NOT NULL,
      kind INTEGER NOT NULL,   -- StringId::of(kind)   (== df_node.kind)
      UNIQUE(file, line, col, kind)
    );

    // extract/dataflow.rs — replaces the `nid` hash closure
    // pseudo-code:
    //   for every fact node n: components[n.id_string] =
    //       (StringId::of(n.file), n.line, n.col, StringId::of(n.kind))
    //   batch INSERT OR IGNORE the distinct tuples into _df_node_dict
    //   batch SELECT id per tuple  ->  surrogate[coord_string] = id
    //   nid(s) = surrogate[s]   (bail if absent: an id with no node = invariant break)
    fn resolve_node_surrogates(&self, facts: &[..]) -> Result<HashMap<String, i64>>

`file`/`kind` are the SAME interned StringIds `df_node` already stores, computed
purely (`StringId::of`, no `_strings` row needed at collect time; the later
`encode_rel_rows` interns them). The dict's `UNIQUE(file,line,col,kind)` is the
real key over the coordinate columns; `id` is the dense surrogate. Identity is no
longer a `format!` hash. The internal `file:line:col:kind` string in `push_node`
survives only as a transient in-batch join key (node <-> its edges), never the
persisted identity.

Batched (N+1-safe): one INSERT OR IGNORE + one SELECT per collect pass, via a
TEMP `_df_coord_batch` join.

### Status — df-id STORAGE surrogate: LANDED and GREEN

The persisted join key for `df_node` / `df_edge` / `df_arg` / `df_field` /
`df_param` / `df_lit` / `df_node_repo` / `nest` / `template_parts` and every
`_rev` twin is now the dense `_df_node_dict` surrogate, NOT a hash of a
concatenated string. Proof:
- `db.rs`/`meta.rs`: `_df_node_dict(id AUTOINCREMENT, file, line, col, kind,
  UNIQUE(file,line,col,kind))`.
- `extract/dataflow.rs`: `nid` is gone; `resolve_coord_surrogates` assigns the
  surrogate (batched INSERT OR IGNORE + SELECT). `extract/text.rs` resolves the
  template coordinate through the SAME dict, so `template_parts.node ==
  df_lit.id`.
- Display: `coord_reconstruct` (`lower.rs`) + the `_txt` views (`declare.rs`)
  read `_df_node_dict`, reconstructing `file:line:col:kind` from the columns —
  identical display text, so every text-asserting test still passes.
- Closures: `rebuild_closures` condenses over RAW ids (`load_edges_from(raw)`);
  `declare_closure` passes the stored id through (`CAST(name AS INTEGER)`)
  instead of re-hashing (`sprf_sym`), which only worked while id == hash(text).
- Join parity: `tests/it/dataflow.rs::coordinate_id_deinterned_but_joins_and_display_hold`
  rewritten to assert the id IS the dict surrogate AND is NOT the old hash.
- Validation: 629 lib / 966 it green (0 failed). Zero orphan references measured
  (every df id is a df_node).

The `resolve_coord_surrogates` + dict pattern IS the general mechanism the sym
normalization below reuses (same shape, different key columns).

### REMAINING (scope expansion 2026-07-20 — the symbol layer). NOT landed.

The rail `.dl/composite-key-string.dl` still flags the IN-MEMORY `format!`
identity strings (the stored key is fixed, the transient in-memory key is not):

BEFORE (measured, this branch): findings for the two owned files —
- `src/graph/typegraph/mod.rs`: 398, 399 (`mint_sym`), 417 (`lambda_sym`), 723
  (df-id `let id = format!`) — 4 findings.
- `src/graph/typegraph/ts/flow.rs`: 17 (df-id `let id = format!`) — 1 finding.

AFTER (this branch, df-id STORAGE surrogate landed): UNCHANGED — 4 + 1. The
storage fix did not touch the in-memory `format!`. Clearing the rail requires
eliminating the in-memory identity strings, which are three distinct reworks:

**(R1) df-id in-memory `format!`** (mod.rs:723 `push_node`, ts/flow.rs:17). The
coordinate string is the in-batch node identity that edges/args/fields/lits
reference. Eliminating it means `DfNode.id: String -> Coord{file,line,col,kind}`
or a node index, and rewriting the 81 `push_node` call sites + every
`DfEdge{from,to}`/arg/field reference across the rust/ts/kotlin/go/python
front-ends. Contained to `src/graph/typegraph/**` + the write seam; NO `.dl`
surface change (storage identity already the surrogate). Clears ts/flow.rs to
zero, mod.rs 4 -> 3.

**(R2) mint_sym** (mod.rs:398/399) — the BIG one. The sym string
`file::kind::name` (+ `parent.name`) is the join key for the WHOLE call/type
graph. Normalizing it = a `_sym_dict(id AUTOINCREMENT, file, kind, name, parent,
UNIQUE(file,kind,name,parent))` (the same mechanism as `_df_node_dict`, keyed on
the sym columns), a `resolve_sym_surrogates` batched resolver, sym columns on
`type_entity`/`call_def`/`call_edge`/`type_edge`/`type_link` + all `_rev` twins
becoming FK integers into `_sym_dict`, sym reconstruction (`file::kind::name`)
reading `_sym_dict`, and a codemod of every `.dl`/test/editor reader. Reader
inventory: see the appended section (gathered separately). This is the
multi-day arc; it is the PREREQUISITE for R3.

**(R3) lambda_sym** (mod.rs:417) — `enclosing_fn_sym::closure::coord`. It
COMPOSES on a sym (the enclosing fn's sym) + a coord, so it rides the `_sym_dict`
from R2. Blocked on R2. Once R2 lands, lambda_sym mints its dict row from
(enclosing_sym_id, "closure", coord_surrogate) — a real composite, no `format!`.

Shared-mechanism note: R1 (`_df_node_dict`, DONE) and R2 (`_sym_dict`) are ONE
mechanism — `resolve_<x>_surrogates(coords)` -> TEMP-probe -> INSERT OR IGNORE
dict -> SELECT ids -> map. R2/R3 instantiate it on the sym columns; they do not
invent a second scheme.

Honest status per the user's directive: the shared surrogate mechanism is built
and PROVEN on the df-id (storage). R1/R2/R3 (the in-memory `format!` elimination
+ the symbol dictionary + the ecosystem codemod) are NOT landed and are a scoped
multi-day arc; the mint_sym reader inventory is appended so it can be continued,
not forgotten.

### df_node / df_lit VIEW collapse — still blocked (not by the key)

The surrogate does NOT unblock the collapse: `df_node_rev` is keyed `(id, rev)`,
which still drops divergent `(var, fn)`-within-one-rev rows the base `df_node`
keeps, regardless of whether `id` is a hash or a surrogate. Widening the twin key
is the original deferred follow-up; the ~14.6MB is explicitly not the priority.


### mint_sym / lambda_sym reader inventory (for R2/R3, so it is not forgotten)

Architectural fact that reframes it: sym columns are ALREADY interned to a
`StringId` (a hash) stored as INTEGER (`spine.rs:72-131` SymSink; `lower.rs:89`
`sym_lit`). So the string<->int boundary is already transparent for equijoins.
The join key that matters is `StringId::of(the "::"-string)`. A `_sym_dict`
surrogate keyed on `(file, kind, name, parent)` changes what a sym VALUE is; the
seams that re-derive the id FROM the string are the coupling to break.

Producers (one mint pair -> ~90 call sites):
- `src/graph/typegraph/mod.rs:396-401 mint_sym`, `:416-418 lambda_sym`,
  `ts/mod.rs:1060` nested-closure prefix.
- callers: rust/mod.rs, kotlin.rs, go.rs, python.rs, ts/mod.rs, ts/flow.rs
  (line lists in the session inventory). Repo-qualification is a SEPARATE
  hand-built seam: `format!("{repo}::{sym}")` at `extract/call.rs:79,200,...`
  and `extract/type_rels.rs:204,273,290`. rev is NEVER folded into the sym (its
  own `Type::Rev` column) — the surrogate must stay rev-agnostic.

Storage (decls.rs) — sym-typed columns (all interned text, none coord):
- `type_entity(sym, parent)` base; `type_entity_rev` base.
- `type_edge(from,to)` VIEW over `type_edge_rev`.
- `type_link(src,dst)` base + `_rev`; `type_sig(sym,ref)`.
- `call_def(sym)` base + `_rev`; `call_edge(caller,callee)` base + `_rev`;
  `call_site(caller,callee)`; `call_name(sym,name)`; `call_kind(fn)`.
- also `doc_comment.sym`, `doc_tag.sym`, `const_value.sym` (VIEW over `_rev`).
- Only `type_edge`/`const_value` are view-backed; the rest are base tables with
  legacy rebuilds (`rebuild_legacy_type_rels`, the call router
  `flip_call_rels_via_router`) that must change in lockstep with the `_rev`
  writers. Built-in-name guards: `declare.rs:831` (type), `:855` (call).

Readers that RE-DERIVE the id from the string (the true coupling):
- `anchor.rs:101 split_repo_sym` (peels `repo::` prefix).
- `lower.rs:89 sym_lit` + pins at `lower.rs:455,505,509,555,916,983` (a `.dl`
  literal full-sym pin hashes the raw string at lower time).
- structural `::closure::` tests: `mod.rs:800-802`, `ts/mod.rs:1060`,
  `mod.rs:582`.
- `std/flow.dl:22-30` `replace_re`-strips `repo::` off the sym IN DATALOG — a
  format-sensitive reader.
- display: `lower.rs:26-56 sym_decode` — format-independent if the dict
  reconstructs text.

dl ecosystem: ~83 files reference a sym rel; ~25 do real joins (heavy:
`.dl/flow-panel.dl` 46, `std/measures.dl`, `std/callgraph.dl`, `std/flow.dl`,
`examples/type_profile.dl`, `goto-flows.dl`, `callable-coverage.dl`, ...).
Pure equijoins survive a consistent surrogate; format-sensitive = `std/flow.dl`
+ any literal full-sym pin.

Tests: 51 `tests/it/**` + 21 `src/` test modules touch sym; **22 files assert
the exact `::`-string form** (typegraph extractor unit tests + call/resolver
goldens: `call_golden.rs`, `resolver_*`, `go.rs:1127-1216`, `mod.rs:582`,
`ts/mod.rs`). These are the guaranteed-to-update seam.

Proposed shared design (instantiates the DONE df-id mechanism on sym columns):
- `_sym_dict(id INTEGER PRIMARY KEY AUTOINCREMENT, file, kind, name, parent,
  UNIQUE(file,kind,name,parent))` (file/kind/name/parent interned StringIds).
- `resolve_sym_surrogates(syms: &[(sym_key, file, kind, name, parent)]) ->
  HashMap<sym_key, i64>` — the exact twin of `resolve_coord_surrogates`.
- sym columns become FK integers into `_sym_dict`; `mint_sym`/`lambda_sym` stop
  returning `format!` strings and return the surrogate (or a `SymCoord` struct
  the write seam resolves), matching how df-id was done.
- sym reconstruction (`file::kind::name`) reads `_sym_dict` (a `sym_decode`
  branch), the way `coord_reconstruct` reads `_df_node_dict`.
- repo-qualification (`repo::sym`) becomes a `(repo_id, sym_id)` pair or a
  second dict tier, NOT a re-concatenation.
- codemod the 22 exact-string-asserting tests + `std/flow.dl`'s `replace_re`
  + any literal full-sym `.dl` pin.

This is the multi-day arc. It is fully specified here so it continues, not
restarts.
