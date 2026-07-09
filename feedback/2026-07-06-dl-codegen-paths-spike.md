# dl feedback — codegen paths spike (2026-07-06)

Source: an agent-run spike in the `smash` repo that used `dl` to generate a
path-addressing surface (`TunePath` enum + `set()` + lens ZSTs) for a Rust struct
from the type graph, plus a `--check` staleness rail. The spike succeeded end-to-end
(112-leaf enum generated, `dl --check` gates drift, core builds), so every item below
comes from a working program, not a dead end. Filed as concrete doc bugs, feature
asks, and DX friction ranked by the iterations it cost.

Programs written (in the smash worktree, for repro reference):
`.dl/gen-tune-paths.dl` (generator), `.dl/lint-tune-paths-stale.dl` (rail), marker
`// sprefa:paths` above `struct Tune`.

---

## A. Doc bugs (reference is wrong or misleading — verifiable)

These actively misled the author; each is a drop-in correction to
`docs/reference/{relations,syntax}.md`.

### A1. `type_sig` does NOT carry struct fields  [relations.md]
Documented as `(sym, slot, pos, ref)` = "type signature slots (params, **fields**)".
Reality: **function** signatures only. Querying a struct sym returns **0 rows**.
- Impact: this is the obvious first tool a user reaches for to enumerate a struct's
  fields, and it silently returns nothing.
- Fix: drop "fields" from the description; add "function params/returns only" and a
  pointer to `ast (field_declaration ...)` for struct fields.

### A2. No relation enumerates a struct's fields with names
Corollary to A1. `type_edge(from, to, "field", repo)` is the other candidate, but it
**dedupes field edges to the target type and drops the field name**, and produces **no
edge at all for scalar fields** (`f32`/`i64`/`bool` are not `type_entity` nodes). So
`Tune` with ~112 scalar fields yields a handful of nameless type edges.
- Impact: "what are struct X's fields?" — the single most obvious structural query —
  has no direct answer. The author fell back to `ast (field_declaration ...)` over
  source text, which works but loses type resolution.
- Fix (doc): state on `type_edge` that field edges dedupe to target type, drop the
  field name, and omit scalar fields. See feature ask B1 for the real fix.

### A3. `split` out-of-range binds NULL, does not drop the row  [syntax.md]
Documented: "out-of-range drops the row (NULL filter)". Reality: the row **survives
with the column bound to NULL**. Consequences:
- A `!has_segment` antijoin misfires (the NULL row exists, so the antijoin sees it).
- NULL reaching a string function throws a raw engine error:
  `Invalid function parameter type Null at index 0` (from `ucfirst(NULL)`).
- The working presence idiom is `p != ""` (NULL `!=` "" is falsy, so it filters).
- Fix: correct the parenthetical; document `!= ""` as the presence/termination idiom
  and note that NULL reaching a scalar fn errors rather than filtering.

### A4. `gen` template substitution's brace rule is undocumented  [syntax.md]
A `{var}` in a `gen` template only substitutes if that var **also appears at least
once outside `{ }`** in the same template. `z { {f} }` emits the literal `z { {f} }`;
`p {f} q { {f} }` substitutes all three. It behaves like a global string replace keyed
on an out-of-brace occurrence.
- Impact: highest single-item turn cost (~6 iterations). Every existing `gen` example
  emits brace-free markdown/d2, so the trap is invisible until you generate real code
  (Rust match arms, struct/fn bodies) that contains literal `{ }`.
- Workaround the author used: keep every substituted var outside braces
  (`t.{field} = if let ... { x } else { return }`; lenses via a brace-free
  `tune_lens!(...)` macro call rather than an inline `impl { }`).
- Fix: document the rule explicitly on `gen`, and add a `dl examples --show` that
  emits real code with literal braces (see B4).

### A5. `${var}` (head/sink) vs `{var}` (gen template) are two dialects
Head/sink string columns interpolate with `${var}`; `gen` templates interpolate with
`{var}`. Same program, two syntaxes, stated nowhere in the reference (only in a user
memory note). Additionally, `${var}` works in a head/sink column but **not** as a body
binding: `c = "${a}"` errors `unbound variable c`.
- Fix: one paragraph in syntax.md contrasting the two interpolation sites and noting
  `${}` is head/sink-only, never a body binding.

### A6. `--check` uses a single global rel namespace across all `.dl/*.dl`
A bare `rel block(...)` in a new `.dl/` file collided with another file's `block`
rel (`rel block declared twice with different columns`). Nothing in the reference or
skill says check-mode shares one namespace.
- Fix: document that `dl --check` loads all `.dl/*.dl` into one rel namespace; advise
  prefixing rels per file. See C-item for the error-message improvement.

---

## B. Feature asks (would have made the spike one-shot)

### B1. A struct-field relation
`type_field(struct_sym, field_name, field_type, ordinal)` (or equivalent). "What are
X's fields, with names and types, in order?" is the central structural query for any
codegen/lens/serialization tooling and today has no direct answer (A1/A2). This is the
highest-leverage ask — it would have removed the entire `ast (field_declaration)`
fallback and its enclosing-struct antijoin.

### B2. Case-fold / concat string builtins
`functions.md` has `ucfirst`/`lcfirst`/`upper`/`lower`/`split`/`replace`/`trim` but no
boundary-aware case fold and **no concat/format**. Snake→UpperCamel required one rule
per segment count (1–6), each gated by segment-presence antijoins, assembled via head
`${a}${b}...` interpolation (~30 lines for what should be one function call).
- Ask: `upper_camel` / `snake_to_camel` (or a general `concat`/`format`).

### B3. A hash / aggregation scalar
The staleness fingerprint the rail writes is `leafcount=112` (a count) because there
is no `hash`/`digest` scalar and no `group_concat` aggregation. A content hash of the
leaf set would catch same-count edits (rename a field: count unchanged, hash changes).
- Ask: a `hash`/`digest` scalar, or `group_concat` so the fingerprint can hash the
  ordered leaf list.

### B4. A splice-mode `gen` example that emits real code
`dl examples --show` for `gen` splice mode emitting a Rust match arm or fn body (with
literal braces), so the A4 substitution rule is demonstrated, not discovered.

### A7. `scip_want` and on-demand SCIP indexing are undocumented  [relations.md — DOCS BUG, not a feature ask]
CORRECTION to a wrong first draft of this note: the capability EXISTS and works. dl
builds a SCIP index on demand from within a program — head the user-derived gate
`scip_want(".").` and `scip_setup::ensure_index` runs the detected indexer once
(`rust-analyzer scip . --output .dl/index.scip` for Rust; install
`rustup component add rust-analyzer`) unless an index already exists, then
`scip_def`/`scip_ref`/`scip_name`/`type_link` populate (one-tick data-driven-scan lag;
missing toolchain skips loudly, never fails the tick). Source: `src/rels/scip.rs:26`,
`src/scip_setup.rs:334` (`ensure_index`), `src/scip_setup.rs:54` (the rust-analyzer
indexer row).

The bug is that NONE of this is discoverable from the reference:
- `scip_want` is a USER-DERIVED relation (engine reads it, users head it), so it is
  absent from `rel_catalog` and therefore from the generated `relations.md`. A reader of
  the reference has no way to learn it exists. Two independent readers (a spike agent and
  a reviewer) concluded "no demand relation, manual step required" straight from the
  docs. That is a real, repeatable failure.
- The scip source relations' doc strings say "from an existing index.scip" with no
  mention that `scip_want` can PRODUCE one. So the empty-rows-without-an-index case reads
  as "no such edge," not "no index yet."
- Asks: (1) document `scip_want(repo)` in `relations.md` (a "user-derived demand
  relations" subsection, alongside `rev_cmp_want`), with the Rust example and the
  auto-index behavior; (2) amend the `scip_def`/`scip_ref`/etc. doc strings to point at
  `scip_want` as the way to obtain an index; (3) consider a `dl doctor`/`--check` hint
  when scip rels are queried but no index and no `scip_want` row exist.

---

## C. Error-message improvements

- `source rule X missing scan` → say "an `ast`/`comment`/`match` atom needs its
  `scan` in the same rule body, not a derived rel." The author hit this twice by
  factoring `scan` into a derived rel (`tune_src(p,rev) <- scan(...)`) then using
  `ast(p,rev,...)` against it.
- `rel block declared twice with different columns` → name the **other file** that
  declared it (A6). Cross-file collisions are undiagnosable without the other site.
- `Invalid function parameter type Null at index 0` → name the function and the
  source atom that produced the NULL (A3), rather than a raw SQLite-shaped error.

---

## D. What worked well (keep)

- `gen` splice mode (comment-marker pairs, convergent no-op on byte match) is the
  right shape for checked-in codegen; the `--check` skip is correct.
- `comment_node` (grammar-backed, `//` tokens stripped) made a comment marker
  (`// sprefa:paths`) clean to detect and strictly better than a doc marker for this
  use (a `//` is invisible to rustdoc; a per-field `// sprefa:gated` tag is a fact a
  proc-macro derive cannot see).
- The staleness rail worked exactly as intended: `dl --check` green when fresh, exit 2
  with a named `diag` after dropping a variant. This is the feature that makes the
  codegen track safe to adopt over a proc-macro.

Minor gotcha worth a skill note, not a bug: the splice `comment` source op matches its
marker regex **anywhere in the file including inside `//!` doc prose**. The author's
scaffold header documented the marker tokens literally and they were picked up as
spurious pairs; reword prose to never contain the literal marker tokens.

---

## E. Bottom line for the spike

The dl-codegen track beat a `#[derive]` proc-macro on compile time (no proc-macro in
the build; output is plain greppable source), IDE/debuggability (real lines, not an
opaque token stream), and per-field policy tags (a comment a derive can't read). The
only cost — drift — is fully covered by the `--check` rail. The friction above is all
authoring ergonomics, not a capability gap: the tool did everything asked. Landing B1
(struct-field relation) and B2 (case/concat) would turn a 90-line generator into a
~10-line one and make a second struct trivial.
