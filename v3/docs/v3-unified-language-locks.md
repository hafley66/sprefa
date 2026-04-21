# v3 unified language locks — session 2026-04-20

Session outcome: sigil unification, rule = Pipeline collapse, parametric
rules as the single reuse surface, arg-mode dispatch formalized, control
flow grounded, mode-annotation grammar flagged as its own exploration
lane. This is the lock file; unchecked items below remain open.

Reading dependency: `v2/docs/_b_v3-unified-language.md` is the teaching
doc that precedes this file. This file updates it with locks the
teaching doc doesn't yet carry. `v3-plugin-author-surface.md` is
unchanged; all locks below fit within its A/B/C/D row table.

---

## 1. Core invariants (six)

1. Ops own everything — diagnostics, patterns, hover, fix, effect type, schema, registry access.
2. Cursor is the unit of flow — `BoxStream<Arc<[Cursor]>>`.
3. Content contract PATH A → B → C — slot reuse → `cursor.content[byte_range]` → `reader.bytes()`.
4. Reads are pipe, writes are deferred effects.
5. Reparse cheap, cancellation real.
6. **Every cursor carries `path: SprfPath`** — never Option, never synthesized at read time.

---

## 2. Concept model

Six concepts, four Pipeline cases, five EntityRef cases, six Cursor fields.

### Concepts

| concept | what it is |
|---|---|
| Op | Rust-implemented cursor-stream transform |
| Rule | named Pipeline; zero or N params; registered per rule-op invocation |
| Pipeline | composition of ops; the runtime unit |
| Cursor | the flow unit; carries path + content + byte_range + slots + captures + parent |
| Capture | named projection from a cursor stream; addressed by `TermPath = (scope_path, name)` |
| Scalar | value tier; string / number / atom / bool / null |

### Pipeline

```rust
enum Pipeline {
    Op(LoweredOp),
    Chain(Vec<Pipeline>),     // A > B > C
    Group(Box<Pipeline>),     // (A > B)
    Fork(Vec<Pipeline>),      // { > A; > B; }
}
```

### EntityRef

```rust
enum EntityRef {
    Scalar(Value),
    Op(Arc<dyn Operator>),
    Pipeline(Arc<Pipeline>),      // anonymous, content-hash-named
    Rule(Arc<Rule>),              // named; has params: Vec<TermPath>
    Capture(SlotKey),
}
```

### Cursor (Shape C — final)

```rust
struct Cursor {
    path: SprfPath,                    // always present
    content: Arc<[u8]>,                // flow-universal
    byte_range: Range,                 // flow-universal
    slots: SlotMap,                    // typed payload
    captures: Captures,                // named bindings
    parent: Option<Arc<Cursor>>,       // narrowing chain
}
```

`fs` / `repo` / `rev` live as slot entries, owned by their respective
ops. Content and byte_range stay as struct fields because every
byte-reading op reads both.

### SprfPath

```rust
struct SprfPath { segments: Vec<PathSeg> }

enum PathSeg {
    Named(Atom),                                // rule name
    ForkArm(usize),                             // positional fork arm
    Anon { file: Atom, hash: [u8; 8] },         // synthesized for anonymous Pipeline
}
```

Anonymous pipeline name: `file_atom(source_file) + "." + blake3(normalized_ast).short()`.

---

## 3. Sigils — three, each with one lowering

### `$TERM`

Universal term intervention. Mode dispatched by position; semantics by op.

| source position | lowered form |
|---|---|
| `$TERM` as op arg | `ArgValue::TermRef { term_path, slot_key }` |
| `> $TERM` chain station | `Pipeline::Op(CaptureWriteOp, [TermRef])` |
| `$TERM` standalone in chain | `Pipeline::Op(TermOp, [TermRef])` — filter-semijoin default |
| `$TERM` in walker pattern hole | walker-internal capture slot decl |
| `${expr}` carveout | host expr lowers to `Pipeline`, wrapped by `CarveoutOp` with narrowed cursor |
| `${op($T1, $T2)}` | application lowers normally; TermRefs propagate |

Runtime resolution:

```rust
fn resolve_term(cursor: &Cursor, r: &TermRef) -> TermMode {
    match cursor.captures.get(r.slot_key) {
        Some(v) => TermMode::Bound(v.clone()),
        None    => TermMode::Unbound,
    }
}
```

### `&`

Cursor rebase.

| source | lowered |
|---|---|
| `&.fs` / `&.repo` / `&.rev` / `&.byte_range` | `Pipeline::Op(CursorRefOp, [AddrExpr::CursorField(kind)])` |
| `&.$X` | `Pipeline::Op(CursorRefOp, [AddrExpr::Capture(ref)])` |
| `&{addr}` | addr parses under address grammar; lowers to `AddrExpr::Computed(pipeline)` |

### Carveouts `${...}` and `&{...}`

Balanced-brace pre-pass scan at lex time. Inside `${...}` parses as host
expr grammar. Inside `&{...}` parses as address grammar. Carveout
ranges get subtracted from sub-grammar's included ranges via
`set_included_ranges`.

### `&&` — retired

No double-ampersand sigil. Registry access is op-mediated:
`rule($R)` with `$R` unbound iterates the rule registry; `fs($P)` with
`$P` unbound iterates fs-op invocations. Each op owns its registry.

---

## 4. Rule = named Pipeline with params

`rule` is the single declaration form. Params are unbound terms in the
signature. Zero params is the degenerate case.

```sprf
rule(classes) > ast[rust] { class $NAME }                # zero params; runs on subscribe
rule(used_by, $CLASS) > ast[rust] { new $CLASS() }       # one param; lazy until call

rule(audit) {
  > classes
  > used_by($NAME)     # binds $CLASS = $NAME at call site
}
```

### Rule type

```rust
struct Rule {
    name: Atom,
    path: SprfPath,
    params: Vec<TermPath>,
    body: Arc<Pipeline>,
    schema: RowSchema,
}
```

- `params.is_empty()` → auto-subscribed by runner, persists to `rule_<path>`.
- `params.len() > 0` → parametric; only subscribed via call-site references. Table columns = `arg_<param>` + capture columns.
- `op` keyword removed — rules subsume reusable op definitions. User-defined ops in Rust remain the Rust op surface.

### No shadowing (for now)

Rule names may not reuse any in-scope built-in op or other rule name.
Resolver emits duplicate-declaration diagnostic. Re-open if needed.

### Recursion

Self-referencing rules require `@recursive(max_depth=N)` attribute. Without
it, resolver emits cycle diagnostic. Cycle detection via Tarjan on the
rule-call graph at lower time.

---

## 5. Arg-mode dispatch

Per-op, per-arg ArgSpec declared by op author. Resolver checks at call
site. Runtime dispatches.

```rust
struct ArgSpec {
    name: Atom,
    accepts: AcceptsMode,
}

enum AcceptsMode {
    BoundOnly,            // error if unbound at call site
    UnboundOnly,          // error if bound
    Either,               // op dispatches per mode
}

enum TermMode {
    Bound(Value),
    Unbound,              // with SlotKey for write-back
}

trait ArgModeDispatch {
    fn dispatch(&self, ctx: OpCtx, modes: &[TermMode], cursor: &Cursor) -> OpAction;
}

enum OpAction {
    EmitBound(Cursor),
    IterateRegistry(BoxStream<Arc<Cursor>>),
    Filter,
    Diagnose(Box<dyn Diagnostic>),
}
```

### Rule mode: derived from body

Resolver walks rule body, collects per-param constraints from op
ArgSpecs. Propagation produces per-param derived mode at rule
declaration.

Example: rule `r` whose body calls `tag($PARAM, :kind)` — tag requires
arg 0 bound; therefore `r`'s `$PARAM` is derived as `BoundOnly` at call
site.

### Rule mode: explicit annotation — **open exploration lane**

See Section 14. User-facing annotation syntax is a reserved exploration
surface; no lock.

---

## 6. Binding resolution — three phases

| phase | what's checked | failure mode |
|---|---|---|
| parse | syntactic well-formedness, `$TERM` / `${...}` / `&.` valid | parse diag |
| lower | binding-source DAG complete, ArgSpec vs call-site modes, rule-mode derivation, cycle detection, explicit annotation conflicts | lower diag |
| run | cursor backpressure on missing terms | drop on upstream close (silent trace), never throw |

### Binding sources (five kinds)

```rust
enum BindingSource {
    Param(RuleId, usize),
    WalkerBody(OpCallId),
    ChainStageEmit(StageId),
    ForkArmAncestor(ArmId),
    ParametricCallProducer(CallSiteId),
}
```

Resolver builds `BindingGraph: HashMap<TermPath, Vec<BindingSource>>`.
Every `$TERM` reference must have a non-empty source vector.

### Runtime wait semantics

| state at op entry | outcome |
|---|---|
| all required terms bound | op runs, emits downstream |
| term missing, upstream emitting | cursor parks (backpressure) |
| term missing, upstream closed | cursor drops |

Never throw for unbound at runtime. Static check in phase lower
prevents indefinite waits.

---

## 7. Control flow

No new syntax. Four mechanisms cover everything.

1. **Fork arms** for branching: `{ > A; > B; }`.
2. **Bare `$X` in chain position** as semijoin — drops cursors lacking `$X`. Relation ops (`eq`, `gt`, `lt`, `in`) emit zero or one cursor and compose the same way. No separate `filter(cond)` primitive.
3. **Recursive rules** for looping (with `@recursive(max_depth=N)` opt-in).
4. **Higher-order control ops** as Rust ops taking Pipeline args.

### Fork capture semantics — **intersection, not union**

When a Fork `{ A ; B }` emits, downstream stages see only captures present in
**all** arms. A cursor from arm 0 lacking arm 1's bindings cannot safely be
consumed by a downstream op that expects both. Static checker computes
`Γ_A ∩ Γ_B`; any downstream reference to a capture not in the intersection
produces a lower-phase diagnostic.

*Rationale*: bash `wait` returns the exit status of the last foreground job;
sprefa Fork is parallel composition with a meet-semilattice merge. Union would
permit unsound downstream references.

### Higher-order control op table (Rust ops, one folder each)

| op | signature | meaning |
|---|---|---|
| `retry(body, n)` | Pipeline × Number → Pipeline | up to n retries |
| `until(body, cond)` | Pipeline × Pipeline → Pipeline | repeat until cond emits |
| `while_changes(body)` | Pipeline → Pipeline | fixpoint; repeat while output changes |
| `when(cond, body)` | Pipeline × Pipeline → Pipeline | run if cond emits, else empty |
| `if_else(cond, then, else)` | Pipeline × Pipeline × Pipeline → Pipeline | per-cursor branch |
| `debounce(body, ms)` | Pipeline × Number → Pipeline | rate-limit re-emissions |
| `distinct_by(body, term)` | Pipeline × TermRef → Pipeline | dedupe by term_path value |
| `merge_by_key(stream, term)` | Pipeline × TermRef → Pipeline | rxjs mergeByKey on term_path |

---

## 8. Lazy / subscribe policy

Pipelines are cold by default. A Pipeline value is a description;
execution starts at subscribe.

```rust
enum SubscribePolicy {
    Cold,                        // default, re-run per subscribe
    Shared { store_key: Key },   // first subscriber materializes to Store; later subscribers join
    Memo { ttl: Duration },      // shared + expiry
}
```

Defaults:
- Top-level zero-param rules: `Shared` (auto-materialize to their sqlite table).
- Parametric rules: `Shared` per (rule, args-tuple) key.
- Anonymous pipelines: `Cold` unless `@memo` attribute.
- Higher-order ops: policy is op-local (retry wants Cold; cache wants Memo).

---

## 9. Mutation effects — four optional slots

```rust
trait MutationEffect: Send + Sync {
    // required
    fn kind(&self) -> &'static str;
    fn apply(&self, ctx: &ApplyCtx) -> ApplyResult;

    // optional
    fn preview(&self) -> Option<Preview> { None }
    fn reversible(&self) -> bool { false }
    fn reverse(&self, ctx: &ApplyCtx) -> ApplyResult { unreachable!() }
    fn source_range(&self) -> Option<SourceRange> { None }
}

enum Preview {
    Diff { before: String, after: String },
    Textual { summary: String },
    Custom(Box<dyn Fn() -> Markdown>),
}
```

### Approval flow

1. Op queues `Arc<dyn MutationEffect>` on ctx mpsc.
2. Runner drains; handler dispatches by `ApprovalPolicy`.
3. `LspPromptBridge` emits `RunEvent::MutationPrompt` + publishes LSP
   diagnostic at `source_range` with code actions `[Approve, Preview, Skip]`.
4. Approve → `apply()`. If `reversible`, framework records handle for
   undo; LSP shows `Undo` action post-apply.
5. Skip → drop, no apply.
6. CLI `InteractiveCli` prints `preview()` if Some; shows y/n/d prompt.
7. `AutoApprove` skips gate; `preview()` not called; `source_range` unused.

---

## 10. Relations tier — third persistence

| tier | table | written by | when |
|---|---|---|---|
| capture | `rule_<path>` | rule's Pipeline | per emitted cursor |
| evidence | `rule_<path>_evidence_<stage>` | framework | auto-tap before filter |
| relations | `relations` | tag-ops, link-ops | cursor-pass-through side effect |
| violations | `violations_<check>` | check ops | per SQL row returned |

### Relations schema

```sql
CREATE TABLE relations (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    src_rule TEXT NOT NULL, src_row INTEGER NOT NULL, src_field TEXT,
    dst_rule TEXT, dst_row INTEGER, dst_field TEXT,
    via_rule TEXT NOT NULL,
    repo TEXT NOT NULL, rev TEXT NOT NULL,
    payload BLOB
);
```

### Tag-op family contract

| input | cursor with referenced captures bound |
| reads | `captures[$NAME]` per arg |
| writes | `relations` row with kind + src/dst |
| cursor | passes through unchanged |

Tag-ops `is_repo($R)`, `is_rev($T)`, `is_fs($F)`, `is_repo_norm($R)`,
`is_rev_norm($T)`. Each owns kind, diagnostic, writer logic.

### Link-op family contract

| input | cursor with two or more captures bound |
| reads | captures[src], captures[dst] per arg |
| writes | `relations` row carrying both sides |
| cursor | passes through unchanged |

Link-ops `link(:kind, $A, :other_kind, $B)`, `depends_on($A, $B)`,
`generated_from($DST, $SRC)`.

Scan-pointer is a relation of known `kind` (repo, rev, fs, repo_norm,
rev_norm). No syntactic class. Set by tag-ops at chain positions.

---

## 11. Runtime model: mergeByKey

Every cursor emission is a keyed event. Key = `(rule_path, term_path)`.
Store row is the merged state per key. Downstream observers see
deltas.

| rxjs | sprf |
|---|---|
| Cold Observable | Pipeline (default) |
| Hot Observable | Pipeline with Shared subscribe policy |
| Subject | Relations tier |
| BehaviorSubject | Capture tier row per term_path |
| ReplaySubject | Evidence tier rows per stage |
| mergeByKey | `merge_by_key` op on term_path |
| combineLatest | merge_by_key across multiple source rules |
| debounce | `debounce(body, ms)` op |
| distinct | `distinct_by(body, term)` op |
| subscribe | runner subscribes top-level rules; wrappers subscribe arg-pipelines |
| unsubscribe | CancellationToken fires, TaskGuard drops |
| backpressure | bounded mpsc channels; term-level parking |

Differential dataflow semantics under the hood. Reparse invalidates by
term_path; retract + reinsert per delta.

---

## 12. Dagging — StageDeps

Resolver builds per-rule stage dependency graph:

```rust
struct StageDeps {
    reads:  Vec<TermPath>,
    writes: Vec<TermPath>,
    path:   SprfPath,
}
```

Used by:
- runner for scheduling (stages with satisfied reads run parallel)
- LSP for block-point diagnostics ("waiting on `classes.NAME`")
- persistence for sprfpath completion (all required terms bound → row insert)
- fork-arm parallelization when DAGs don't cross

A cursor's sprfpath is incomplete until all parameter bindings are
present. Persistence writes only complete sprfpaths. Partial flows live
as parked cursors.

---

## 13. Sub-grammar lowering — two flavors

| sub-grammar | lowers to | why |
|---|---|---|
| json, yaml, toml, md | sprf op chain (fork over field-extract ops) | structural, composable |
| ast-grep walker | walker-native rule tree + capture slot decls | opaque, engine-owned |
| regex | single `re_match(pattern)` op | opaque, leaf |
| shell | `sh` op with body as literal + carveout substitution | opaque, effect |

`${...}` and `&{...}` carveouts inside any sub-grammar body are
extracted via balanced-brace pre-pass; sub-grammar parses with those
ranges excluded. Host ranges re-enter as narrowed-cursor
sub-pipelines.

`sh` double-brace escape `${{var}}` passes literal `${var}` to shell.

---

## 14. Term annotations — **open exploration grammar lane**

This is where arbitrary linking / tagging / annotating may live. The
lane is reserved at the grammar tier; semantics are unpicked.

### Motivation

A term reference carries more than a name. It may also carry:

- **mode** — must-be-bound / must-be-unbound / either
- **kind** — the scan-pointer-like class (repo, rev, fs, norm, ...)
- **link directive** — bind this term to another term by rule
- **scope modifier** — escape to ancestor, limit to fork arm, etc.
- **persistence directive** — record / skip / annotate-only
- **arbitrary user tag** — op-local convention

Forcing all of these into one sigil (`$`) loses discriminability.
Forcing each into a new sigil grows the language. A term-annotation
grammar is the compromise: one extra character-class after `$NAME`,
parsed into a structured annotation AST.

### Candidate shapes (not locked)

```
$NAME:atom                   # atom annotation (uses existing `:` grammar)
$NAME@kind                   # @-prefixed kind sigil
$NAME!mode                   # !-prefixed mode sigil
$NAME(annotation_expr)       # paren-carved sub-expression
$NAME{annotation_body}       # brace-carved annotation block
$NAME[kind, mode, link]      # bracket annotation list
$NAME :: kind :: mode        # prolog-ish cascading annotations
```

Each has different tradeoffs:

| shape | grammar cost | readability | extensibility | collisions |
|---|---|---|---|---|
| `$NAME:atom` | zero (reuses atom) | low for chained | single-slot | none |
| `$NAME@kind` | new sigil | medium | single-slot | bash-ish |
| `$NAME!mode` | new sigil | low | single-slot | yaml-like |
| `$NAME(...)` | parse ambiguity with op call | medium | high | risky |
| `$NAME{...}` | parse ambiguity with fork | medium | high | risky |
| `$NAME[...]` | conflicts with slot bracket | low | high | collision with `ast[lang]` unless terms can't appear in op-head position |
| `$NAME :: ...` | lex-level new token | high | high | mercury/haskell lineage |

### Scope of what annotation could carry

1. **Mode** — `:bound`, `:free`, `:either`. Derived by default; explicit
   overrides derivation.
2. **Kind** — scan-pointer kinds (`:repo`, `:rev`, `:fs`, `:repo_norm`).
   Could make `is_repo($R)` tag-op redundant by letting `$R:repo` declare
   its kind at binding time.
3. **Link** — `$X linked_to $Y` or similar. Writes a relation row at
   binding time, replacing the need for a separate link-op when the
   relation is known statically.
4. **Persistence** — `:persist`, `:ephemeral`, `:annotate`. Controls
   whether the binding goes to sqlite, stays in memory, or just
   annotates an existing row.
5. **Arbitrary user** — namespaced user tags (`:user/important`). For
   op-local conventions without framework changes.

### Design questions (all open)

- Do annotations compose? (`$X:bound:repo:persist` or
  `$X[bound, repo, persist]`)
- Do annotations run at bind time or reference time?
- Are annotations write-once at declaration or mutable through the chain?
- Do annotations participate in mode derivation or override it?
- Is there a default annotation set per rule or per op?
- How do LSP hover and completion surface annotations?
- Can user-defined tag-ops and link-ops be absorbed into annotations,
  reducing the op surface?

### Relation to the rest of the system

If annotations can carry kind + persistence directives, the tag-op
family shrinks or vanishes. If annotations can carry link directives,
the link-op family shrinks or vanishes. If annotations carry only mode,
they're a narrower feature and tag/link ops stay.

This is the exploration surface. Pick a shape before writing grammar.js.

---

## 15. Op authoring surface — min-viable

Four trait slots minimum (unchanged from `v3-plugin-author-surface.md`):

| row | what |
|---|---|
| A1 | `NAME: &str` + `inventory::submit!` |
| A2 | `parse(&OpInvocation, &mut ProgramCtx) -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>` |
| C1 | `pipe(OpCtx, BoxStream<Arc<[Cursor]>>) -> BoxStream<Arc<[Cursor]>>` |
| C6 | one `CaptureKind` impl (if op emits captures) |

Everything else defaults.

### Framework affordances added this session

- `ProgramCtx::register_invocation(kind, range, path)` — op-invoked at parse time to populate its own registry.
- `ArgValue::TermRef { term_path, slot_key }` — new ArgValue variant.
- `OpCtx::resolve_term_modes(cursor, &[ArgValue]) -> Vec<TermMode>` — helper for arg-mode dispatch.
- `Operator::signature() -> Vec<ArgSpec>` — per-arg mode declaration.
- `MutationEffect::{preview, reversible, reverse, source_range}` — four optional slots.
- `SubscribePolicy` enum + `Pipeline::subscribe(policy)` wrapper.
- `StageDeps` computed per rule at lower; consumed by runner + LSP.
- `BindingGraph` computed per rule at lower; used for diag + completion.
- `Rule { params, schema with arg_<param> columns }` — parametric rule type.

---

## 16. What was discarded this session

| discarded | replaced by |
|---|---|
| `$$repo($R)` / `$$rev($T)` scan sigil class | tag-op family `is_repo($R)` / `is_rev($T)` |
| `&&.rules` registry sigil | op-mediated registry via parametric call `rule($R)` |
| Separate `op` keyword for reusable defs | `rule(name, $TERM*)` subsumes |
| `${rule.$V > $NEW}` rename via `>` inside carveout | capture-write station `> $NEW` as ordinary chain op |
| Scan-pointer as sigil class | relation rows with known `kind` |
| `$NAME` vs `${NAME}` duality | `$NAME` canonical; `${...}` is the carveout expr-hole |
| Separate `Rule` vs `Op` runtime | Rule is a named Pipeline; collapse |
| Norm as dedicated syntax | `is_repo_norm($R)` tag-op |
| Bash-style `>&N` content redirect | capture-write op + prolog mode |
| Full unification / SLD resolution as framework concern | shallow prolog only (mode dispatch); embed deep logic in `prolog(...)` op if ever needed |

---

## 17. Open items (locked open, not dropped)

1. **Term annotation grammar** — Section 14. Pick shape before grammar.js.
2. ~~**Anonymous pipeline AST normalization**~~ — **LOCKED 2026-04-20**: blake3 over canonicalized AST (whitespace strip, var rename, comment strip). Scheme details TBD at implementation time.
3. **Rule recursion annotation spelling** — `@recursive(max_depth=N)` vs `rule(..., :recursive(5))` vs attribute form.
4. **Shadowing policy across scopes** — current lock is no-shadowing same-scope. Cross-scope is undecided.
5. **Parametric rule call site subpath** — does `foo(:a).arm_0` differ from `foo(:b).arm_0` in path? Probably yes, args baked in.
6. **Fork arm path naming when unnamed** — positional `arm_N` locked; could use content hash. Minor.
7. **Evidence tables for anonymous pipelines** — default on. Opt-out via `@ephemeral`.
8. **`merge_by_key` as first-class op vs SQL-derived** — locked as both coexist; op for hot path, SQL for cold queries.
9. **Subscribe-policy per-call vs per-definition** — currently per-definition; call-site override not designed.
10. **Undo scope** — stack-of-effects vs targeted by id. Stack is simpler; targeted more powerful.

---

## 18. Reading order for re-entry

1. This file.
2. `v2/docs/_b_v3-unified-language.md` — teaching doc (some sections now stale; Appendix A needs grammar.js-direction rewrite).
3. `v3/docs/v3-plugin-author-surface.md` — op author A/B/C/D surface.
4. `v3/docs/v3-min-author-ops.md` — the metric.
5. `chat_log/20260420.1.v3-language-design-three-shapes.md` — prior session.
6. This session's chat log (filed next).

---

## 19. Invariant count summary

- 6 concepts: Op, Rule, Pipeline, Cursor, Capture, Scalar
- 4 Pipeline cases: Op, Chain, Group, Fork
- 5 EntityRef cases: Scalar, Op, Pipeline, Rule, Capture
- 6 Cursor fields: path, content, byte_range, slots, captures, parent
- 3 sigils: `$`, `&`, carveouts `${...}` / `&{...}`
- 6 invariants: ops-own-everything, cursor-is-flow, content-contract, reads-pipe/writes-deferred, reparse-cancel, cursor-has-path
- 4 persistence tiers: capture, evidence, relations, violations
- 3 diagnostic phases: parse, lower, run
- 5 binding sources: param, walker-body, chain-stage, fork-arm, parametric-call
- 4 Pipeline run policies: Cold, Shared, Memo (+ op-local)
- 4 min-viable op trait slots: name, parse, pipe, capture-kind

---

## End of lockfile 2026-04-20
