# sprefa v3 — language spec (parse-facing)

Sibling to `parse.rs` (now in `sprefa_parse`), `op.rs`, `ops/*.rs`. Complements
`v3/docs/v3-unified-language-locks.md` (runtime / semantic invariants) and
`v2/docs/_b_v3-unified-language.md` (teaching doc) by pinning the parse-layer
surface in one document with the Rust types that back each concept.

Source-of-truth ordering when the three disagree: locks > this file > teaching
doc. Teaching doc has drifted; the locks file and this spec are current.

Sessions captured: 2026-04-19, 2026-04-20, 2026-04-21.

---

## Table of Contents

1. [Scope and layering](#1-scope-and-layering)
2. [Three tiers](#2-three-tiers)
3. [Cursor — the flow unit](#3-cursor--the-flow-unit)
4. [SprfPath and ParseSite](#4-sprfpath-and-parsesite)
5. [Name tier: EntityRef](#5-name-tier-entityref)
6. [Pipeline, Op, and the AST seam](#6-pipeline-op-and-the-ast-seam)
   1. [6.1 OpInvocation (AST)](#61-opinvocation-ast)
   2. [6.2 Pipeline (lowered)](#62-pipeline-lowered)
   3. [6.3 Operator trait](#63-operator-trait)
7. [Casing as syntax](#7-casing-as-syntax)
8. [The `$` op](#8-the--op)
   1. [8.1 Unified shape](#81-unified-shape)
   2. [8.2 Lex rule](#82-lex-rule)
   3. [8.3 Balanced-brace pre-pass](#83-balanced-brace-pre-pass)
   4. [8.4 Shell-brace escape `${{...}}`](#84-shell-brace-escape-)
   5. [8.5 `$$sigil` retirement, `$$` recycle](#85-sigil-retirement--recycle)
   6. [8.6 Parser re-entry on carveouts](#86-parser-re-entry-on-carveouts)
9. [Cursor narrowing at carveout](#9-cursor-narrowing-at-carveout)
10. [Dotted access and xrefs](#10-dotted-access-and-xrefs)
11. [`> $X` capture-write](#11--x-capture-write)
12. [Fork and void](#12-fork-and-void)
13. [Last-bound Ans slot](#13-last-bound-ans-slot)
14. [Scan-pointers as tag-ops](#14-scan-pointers-as-tag-ops)
15. [Phase ordering: parse, lower, run](#15-phase-ordering-parse-lower-run)
16. [Binding graph and mode dispatch](#16-binding-graph-and-mode-dispatch)
17. [Diagnostics surface](#17-diagnostics-surface)
18. [First-pass implementation scope](#18-first-pass-implementation-scope)
19. [Open items](#19-open-items)

---

## 1. Scope and layering

1.1 This spec governs everything the parser, lexer, and lowerer must decide.

1.2 `sprefa_parse` is a leaf crate holding the AST types and parse functions.
The `sprefa` runtime crate consumes it. The boundary is enforced by crate
separation, not convention.

```text
sprefa_parse  ──────────────▶  sprefa
  site::ParseSite                   op.rs, ops/*, runner, store, LSP
  ast::OpInvocation                 (lowers OpInvocation -> Pipeline)
  parse::host_parse()
```

1.3 When locks, teaching doc, and this spec disagree, locks win on semantic
invariants; this file wins on syntactic mechanics; teaching doc is historical
context.

---

## 2. Three tiers

2.1 Information in a running sprf program lives in one of three tiers.

2.2 **Stream tier.** Cursors flow through operators. Runtime evaluation model.

2.3 **Name tier.** Identifiers bind to values in a single lexically-scoped
environment. Resolved kind: op, rule, capture, or scalar.

2.4 **Value tier.** Scalar literals: string, atom, number, bool, null.

2.5 Values flow as fields inside cursors. Names reference streams of cursors.
Ops transform streams. One uniform rule per tier.

---

## 3. Cursor — the flow unit

3.1 A cursor is a closure over file content, threaded through a pipeline. It
is the only first-class value that flows through the runtime. Scalars, rules,
ops are either embedded in cursor payload (captures, slots) or live in the
static environment (EntityRef).

3.2 Current Rust type (`sprefa/src/types.rs`):

```rust
#[derive(Debug, Clone)]
pub struct Cursor {
    pub run_id:     RunId,
    pub repo:       Arc<str>,
    pub rev:        Arc<str>,
    pub fs:         Option<FilePath>,
    pub captures:   HashMap<Arc<str>, Capture>,
    pub fks:        HashMap<Arc<str>, RowId>,
    pub path:       SprfPath,
    pub evidence:   Vec<OpEvidence>,
    pub content:    Option<Arc<bytes::Bytes>>,
    pub byte_range: Option<Range<usize>>,
    pub slots:      Slots,
}
```

3.3 v3 additions pending (this spec):

```rust
// §13 Ans slot:
pub last_bound: Option<SlotKey<()>>,   // or Option<Arc<str>>; see §13.3

// v3 also flattens repo/rev/fs into slots eventually (locks §2, Cursor
// Shape C). First pass keeps them as fields because every byte-reading op
// touches them; flatten once the slot API absorbs access.
```

3.4 `Capture` is the per-name payload:

```rust
#[derive(Debug, Clone)]
pub struct Capture {
    pub value:        Arc<str>,
    pub kind:         CaptureKind,
    pub ref_id:       Option<RefId>,
    pub scan_pointer: Option<Arc<str>>,
    pub verified:     Tri,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureKind {
    SpanBacked { span: Range<usize> },
    Synthesized,
}
```

3.5 `SpanBacked` captures point into `cursor.content`; zero-copy references.
`Synthesized` captures carry materialized strings (computed values, JSON
strings, scalar literals). `&.$X` rebase chooses narrow vs replace per kind.

3.6 `Slots` is the typed type-erased per-cursor payload store:

```rust
pub struct SlotKey<T: 'static + Send + Sync> { _marker: PhantomData<fn() -> T> }

#[derive(Debug, Default, Clone)]
pub struct Slots {
    map: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}
```

Ops declare `pub const FOO: SlotKey<FooTree> = SlotKey::new();` and write/read
parsed trees, tokens, or derived state without widening `Cursor`.

---

## 4. SprfPath and ParseSite

4.1 Two coordinate systems coexist: `ParseSite` names a location in the .sprf
source (compile-time stable); `SprfPath` names the per-cursor runtime trail
through pipeline stages.

4.2 `ParseSite` (now in `sprefa_parse::site`):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSite {
    pub file:       Arc<Path>,
    pub path:       Arc<[ParseSeg]>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseSeg {
    Top         { index: u16 },
    BraceChild  { index: u16 },
    ParenChild  { index: u16 },
    PatternLeaf { key: Arc<str> },
}
```

4.3 Every AST node that matters gets a `ParseSite`. Diagnostics anchor here.
Hover resolves cursor positions to `ParseSite` and back to a source range.

4.4 `SprfPath` is runtime provenance:

```rust
#[derive(Debug, Clone)]
pub struct SprfPath(pub Arc<[PathSeg]>);

#[derive(Debug, Clone)]
pub enum PathSeg {
    Op        { name: Arc<str>, parse_site: Arc<ParseSite>, step: u16 },
    Named     { name: Arc<str>, key: Arc<str>, parse_site: Arc<ParseSite> },
    ForkArm   { index: u16, parse_site: Arc<ParseSite> },
    SwitchArm { pat: Arc<str>, parse_site: Arc<ParseSite> },
    LeafArm   { key: Arc<str>, parse_site: Arc<ParseSite> },
    Iter      { index: u64 },
}
```

4.5 Carveout entry appends a new variant (to be added):

```rust
// extension for §9
PathSeg::Carveout { source_range: Range<usize>, parse_site: Arc<ParseSite> }
```

---

## 5. Name tier: EntityRef

5.1 Every name resolves to one of five entity kinds (locks §2):

```rust
pub enum EntityRef {
    Scalar(Value),
    Op(Arc<dyn Operator>),
    Pipeline(Arc<Pipeline>),   // anonymous, content-hash-named
    Rule(Arc<Rule>),           // named; has params: Vec<TermPath>
    Capture(SlotKey<()>),      // erased; lookup by name string
}
```

5.2 `Value` is the scalar tier:

```rust
pub enum Value {
    String(Arc<str>),
    Atom(Arc<str>),
    Number(f64),
    Bool(bool),
    Null,
}
```

5.3 `Rule` (locks §4):

```rust
pub struct Rule {
    pub name:   Arc<str>,      // atom
    pub path:   SprfPath,
    pub params: Vec<TermPath>,
    pub body:   Arc<Pipeline>,
    pub schema: RowSchema,
}

pub struct TermPath {
    pub scope:   SprfPath,
    pub name:    Arc<str>,
}
```

5.4 Resolution is one environment, lexically scoped, lookup walks from
innermost to outermost scope. Resolved `EntityRef` tells the runtime how to
treat the reference (apply op, subscribe rule, project capture, pass scalar).

---

## 6. Pipeline, Op, and the AST seam

### 6.1 OpInvocation (AST)

6.1.1 Host-parse output, pre-lower. Lives in `sprefa_parse::ast`:

```rust
#[derive(Debug, Clone)]
pub struct OpInvocation {
    pub name:       Arc<str>,
    pub brackets:   Vec<BracketSlot>,
    pub paren_src:  Option<ParenSlot>,
    pub brace_src:  Option<BraceSlot>,
    pub parse_site: Arc<ParseSite>,
    pub crossrefs:  Vec<CrossRefOccurrence>,
}

#[derive(Debug, Clone)]
pub struct BracketSlot { pub src: Arc<str>, pub byte_range: Range<usize> }
#[derive(Debug, Clone)]
pub struct ParenSlot   { pub src: Arc<str>, pub byte_range: Range<usize> }
#[derive(Debug, Clone)]
pub struct BraceSlot   { pub src: Arc<str>, pub byte_range: Range<usize> }

#[derive(Debug, Clone)]
pub struct CrossRefOccurrence {
    pub rule:       Arc<str>,
    pub var:        Arc<str>,
    pub byte_range: Range<usize>,
}
```

6.1.2 Each slot's body is raw source bytes; the op's `parse` hook owns
sub-grammar parsing. `crossrefs` are the balanced-brace-scanned `${rule.$V}`
tokens found in this invocation's slots.

6.1.3 Parser emits one `OpInvocation` per op-call in the source. Chain
structure is captured by the surrounding `Pipe` enum produced by `host_parse`.

### 6.2 Pipeline (lowered)

6.2.1 Lowered representation (locks §2):

```rust
#[derive(Clone)]
pub enum Pipeline {
    Op     (LoweredOp),
    Seq    (Vec<Pipeline>),          // A > B > C
    Fork   (Vec<ForkBranch>),        // { > A ; > B ; }
    Switch { on: ChannelSelector, arms: Vec<(Arc<str>, Pipeline)> },
}

pub struct LoweredOp {
    pub op:         Arc<dyn Op>,
    pub xrefs:      Arc<[CrossRefOccurrence]>,
    pub parse_site: Arc<ParseSite>,
}

pub struct ForkBranch {
    pub pipeline:   Pipeline,
    pub parse_site: Arc<ParseSite>,
}
```

6.2.2 Lower rewrites `Pipe` trees into `Pipeline` by resolving op names,
collapsing chains into `Seq`, and attaching `xrefs` from `OpInvocation` for
rule-DAG construction.

6.2.3 New cases added by this spec:

```rust
// §8.6 — carveout lowering:
Pipeline::Op(CarveoutOp { inner: Arc<Pipeline>, source_range: Range<usize> })

// §11 — capture-write:
Pipeline::Op(CaptureWriteOp { slot: Arc<str> })

// §12 — void sink:
Pipeline::Op(VoidOp)
```

### 6.3 Operator trait

6.3.1 Runtime contract (locks §15):

```rust
#[async_trait]
pub trait Op: Send + Sync + 'static {
    const NAME: &'static str;

    fn parse(
        &self,
        inv:  &OpInvocation,
        pctx: &mut ProgramCtx,
    ) -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>;

    fn pipe(
        &self,
        ctx:    OpCtx,
        stream: BoxStream<'static, Arc<[Cursor]>>,
    ) -> BoxStream<'static, Arc<[Cursor]>>;

    fn signature(&self) -> Vec<ArgSpec> { Vec::new() }
    fn capture_kinds(&self) -> &[&str] { &[] }

    // optional slots: hover, completion, effect, ...
}
```

6.3.2 `parse` is the per-op lowering hook: takes syntactic `OpInvocation`,
returns lowered `Pipeline`. Invoked by the registry during the lower phase.

6.3.3 `pipe` is the runtime transform: stream in, stream out.

---

## 7. Casing as syntax

7.1 First character of an identifier determines its category:

| first char       | category                           |
|------------------|------------------------------------|
| UPPERCASE letter | term (capture decl or ref)         |
| lowercase letter | op or rule name                    |
| punctuation      | sigil op (`$`, `&`, carveouts)     |
| digit            | number literal                     |

7.2 Casing-as-syntax is locked as of 2026-04-19 (locks §17 "Prolog convention"),
but first-pass implementation does not enforce it — convention only.

7.3 Enforcement (future): at classify time, bare ident starting uppercase is
rejected unless preceded by `$`. Bare ident starting lowercase in term
position is rejected. Two symmetric diagnostics, reserved for a later pass.

---

## 8. The `$` op

### 8.1 Unified shape

8.1.1 `$` is a single op with two argument shapes: bare uppercase ident, or
balanced-brace expression body.

```rust
// Conceptually:
//   $NAME        ≡   ${NAME}
//   ${expr}      →   CarveoutOp around host_expr(expr)
//   ${op($X)}    →   normal application; TermRef for $X propagates
//   $NAME in     →   TermRef { name: "NAME" }
//   op arg
```

8.1.2 `$NAME` is shorthand for `${NAME}`. Same AST node in both cases:

```rust
// Proposed AST node emitted by the lexer-level pre-pass:
pub enum CarveoutNode {
    TermRef { name: Arc<str>, parse_site: Arc<ParseSite> },
    Expr    { raw_range: Range<usize>, parse_site: Arc<ParseSite> },
}
```

The parser re-enters `Expr.raw_range` as host_expr when lowering a carveout.

### 8.2 Lex rule

8.2.1 On byte `$`:

| follow-up    | action                                                         |
|--------------|----------------------------------------------------------------|
| `{{`         | shell-brace escape (§8.4)                                      |
| `{`          | enter carveout, balanced-brace scan (§8.3)                     |
| `$`          | `$$` Ans sigil (§8.5); reserved for ans-slot reference         |
| `[A-Z]`      | term-ref shorthand; consume `[A-Z0-9_]+`                       |
| other        | parse error `bare-dollar-without-target`                       |

8.2.2 Deprecation path: scan-pointer forms `$$repo`, `$$rev`, `$$fs` are
accepted for migration with `deprecated-scan-sigil` diagnostic pointing at
the tag-op replacement. Remove once fixtures are migrated.

### 8.3 Balanced-brace pre-pass

8.3.1 Runs at lex time. Produces a `Vec<Carveout>` indexed by byte position
used by every sub-grammar's included-ranges computation.

```rust
pub struct Carveout {
    pub outer_range: Range<usize>,   // includes `${` and `}`
    pub inner_range: Range<usize>,   // strictly between braces
    pub kind:        CarveoutKind,
}

pub enum CarveoutKind {
    HostExpr,      // ${...}
    Address,       // &{...}
    ShellLiteral,  // ${{...}}
}
```

8.3.2 Scanner maintains a brace-depth counter with these rules:

- Inside `"..."` and `'...'`: skip contents (respecting escape sequences).
- Inside `r"..."` / `r#"..."#`: skip with matching hash count.
- Inside `#` comment to end-of-line: skip.
- Nested `${` and `&{`: push a new frame; track both kinds independently.
- Shell escape `${{` ... `}}`: atomic; braces don't affect the counter.

8.3.3 Sub-grammar consumers (ast-grep walker body, regex, json/yaml/toml,
shell) must call:

```rust
pub fn strip_carveouts(
    range:     Range<usize>,
    carveouts: &[Carveout],
) -> Vec<Range<usize>>
```

to produce the included-ranges multi-range their parser sees, with carveout
bytes removed.

### 8.4 Shell-brace escape `${{...}}`

8.4.1 Opens a `CarveoutKind::ShellLiteral` whose inner bytes pass through
verbatim to `sh` op bodies. Allows writing `${VAR}` literally in a shell
command.

8.4.2 Lexer scans to matching `}}`; single-brace counts are ignored inside.

8.4.3 Only meaningful inside a `sh(...)` body. Elsewhere treated as a
parse error or as a literal `${...}` pair by the host grammar (choice pinned
at implementation).

### 8.5 `$$sigil` retirement, `$$` recycle

8.5.1 The v2 `ScanPointerRef { sigil: Arc<str> }` token class is retired
(locks §16). Scan-pointer tracking is relocated to tag-ops (§14).

8.5.2 `$$` is recycled as the Ans-slot sigil (§13). No ambiguity with the
retired form because the retired form required a named suffix (`$$repo`,
`$$fs`); bare `$$` was never valid in v2.

### 8.6 Parser re-entry on carveouts

8.6.1 When host parser reaches a `Carveout` token, it recursively parses
`inner_range` using the host-expr grammar. Re-entry is eager: errors inside
surface at parse time, not at lower time.

8.6.2 Lowered form:

```rust
Pipeline::Op(CarveoutOp {
    inner:        Arc<Pipeline>,
    source_range: Range<usize>,   // outer_range from §8.3
})
```

8.6.3 `source_range` is the lexical `${...}` span in the outer .sprf source.
It is the byte-range the cursor narrows to at runtime, not the extent of
the inner expression.

---

## 9. Cursor narrowing at carveout

9.1 `CarveoutOp::pipe` runs per incoming cursor. For each cursor:

```rust
fn pipe(&self, ctx: OpCtx, stream: BoxStream<Arc<[Cursor]>>) -> BoxStream<Arc<[Cursor]>> {
    stream
        .map(|batch| batch.iter().map(|c| narrow(c, self.source_range.clone())).collect())
        .flat_map(|narrowed| self.inner.pipe(ctx.clone(), narrowed))
        .map(|batch| batch.iter().map(|c| rebase(c, outer_byte_range)).collect())
        .boxed()
}
```

9.2 Field-by-field inheritance at narrow (entry):

| cursor field   | narrow policy                                     |
|----------------|---------------------------------------------------|
| run_id         | inherited                                          |
| repo / rev     | inherited                                          |
| fs             | inherited                                          |
| content        | inherited (same Arc)                               |
| byte_range     | replaced with `source_range`                       |
| captures       | inherited                                          |
| fks            | inherited                                          |
| slots          | inherited                                          |
| last_bound     | inherited (§13)                                    |
| evidence       | inherited                                          |
| path           | appended with `PathSeg::Carveout { source_range }` |

9.3 At rebase (exit), `byte_range` is restored to the outer cursor's. All
other fields carry whatever the inner pipeline wrote. Pattern sugar stays
isomorphic to inline form — the only thing carveout *owns* is range
narrowing.

9.4 The narrow/rebase helpers reuse `cursor_ref` machinery; do not build a
parallel path.

---

## 10. Dotted access and xrefs

10.1 `rule.$V` is an ordinary host_expr. Left of `.` resolves to an
`EntityRef::Rule`; right of `.` is a capture projection.

```rust
// Lowered AST for `rule.$V`:
pub struct Xref {
    pub rule:    Arc<str>,
    pub capture: Arc<str>,
    pub parse_site: Arc<ParseSite>,
}
// Becomes an ArgValue::TermRef at op-call lowering:
ArgValue::TermRef {
    term_path: TermPath { scope: rule_scope, name: capture },
    slot_key:  runtime_key,
}
```

10.2 At runtime: the op containing the xref subscribes to `rule`'s output
stream and performs a semijoin on `capture`. Parked cursors wait for
`rule` to emit a matching row; drop silently on upstream close.

10.3 `${rule.$V}` is a carveout whose body is the host_expr `rule.$V`. No
special-case lexer form. The v2 `parse_cross_ref` function is deleted;
`${rule.$V > $TARGET}` parses as chain of xref + capture-write inside a
carveout.

10.4 Casing rule: `rule.name` (lowercase right of dot) is a path continuation;
`rule.$V` (capture sigil) is a capture projection. Resolver disambiguates.

---

## 11. `> $X` capture-write

11.1 At chain-step position, a bare `$IDENT` (not followed by `(` or `{`)
lowers to a capture-write:

```rust
pub struct CaptureWriteOp {
    pub slot:       Arc<str>,
    pub parse_site: Arc<ParseSite>,
}

impl Op for CaptureWriteOp {
    fn pipe(&self, ctx: OpCtx, stream: BoxStream<Arc<[Cursor]>>) -> BoxStream<Arc<[Cursor]>> {
        let slot = self.slot.clone();
        stream.map(move |batch| {
            let slot = slot.clone();
            Arc::from(
                batch.iter().map(|c| write(c, &slot)).collect::<Vec<_>>().into_boxed_slice()
            )
        }).boxed()
    }
}

fn write(c: &Cursor, slot: &Arc<str>) -> Cursor {
    let mut out = c.clone();
    let range = c.byte_range.clone().unwrap_or(0..c.content.as_ref().map_or(0, |b| b.len()));
    let value = c.active_bytes_arc_str();   // Arc<str> of the active slice
    out.captures.insert(
        slot.clone(),
        Capture::span_backed(value, range),
    );
    out.last_bound = Some(SlotKey::from_name(slot.clone()));   // §13
    out
}
```

11.2 Semantics:

- Write `captures[slot] = SpanBacked { span: cursor.byte_range }`.
- Set `last_bound = Some(slot)`.
- Emit the cursor; `content` and `byte_range` are untouched.

11.3 Annotate-only. The narrowing variant (`&>` sigil) was considered and
dropped in favor of fork-to-void (§12) for the rare case where a side
computation should transform its own content without polluting the main
cursor.

11.4 Storage type: `captures[slot]` is a `Capture` with `SpanBacked` kind
holding a `Range<usize>` into `cursor.content`. Zero-copy — the content Arc is
shared. Downstream `&.$X` rebase narrows to the stored span without copying.

11.5 Binding-graph contribution: lower phase records
`BindingSource::ChainStageEmit(stage_id)` for the slot. Downstream `$X` refs
resolve to this source; resolver rejects term refs with an empty source
vector (locks §6).

```rust
pub enum BindingSource {
    Param(RuleId, usize),
    WalkerBody(OpCallId),
    ChainStageEmit(StageId),
    ForkArmAncestor(ArmId),
    ParametricCallProducer(CallSiteId),
}

pub struct BindingGraph {
    pub sources: HashMap<TermPath, Vec<BindingSource>>,
}
```

---

## 12. Fork and void

12.1 Fork syntax: `{ > A ; > B ; }`. Each arm is a pipeline starting with
`>` or bare chain. Arms are separated by `;`.

12.2 Lowered form:

```rust
Pipeline::Fork(vec![
    ForkBranch { pipeline: arm0, parse_site: site0 },
    ForkBranch { pipeline: arm1, parse_site: site1 },
])
```

12.3 Runtime: fork duplicates each incoming cursor to every arm via
`Arc::clone`. Arms run concurrently. Merge is stream interleave — no join,
no combine, no key matching. Each emitted cursor carries
`PathSeg::ForkArm(i)` appended at fork entry.

12.4 Multiplicity: if upstream emits K cursors and there are N arms with
per-arm multiplicities `m_i`, the fork emits `K * sum(m_i)` cursors.
Arms ending in `void` contribute 0.

12.5 `void` is a regular op:

```rust
pub struct VoidOp;

impl Op for VoidOp {
    const NAME: &'static str = "void";
    fn pipe(&self, _ctx: OpCtx, stream: BoxStream<Arc<[Cursor]>>)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        // drain to /dev/null; emit nothing
        stream.for_each(|_| async {}).map(|_| Arc::from(Vec::new().into_boxed_slice())).boxed()
        // in practice: futures::stream::empty() after polling upstream to completion
    }
}
```

12.6 Fork-to-void pattern for side-effect taps that transform:

```sprf
A > $VAR > {
  > norm > scan_pointer > void;    # side-chain; sink via void
  > main_rest;                      # main flow continues
}
```

Arm 0: inherits `$VAR` and `last_bound`, runs `norm` (may rewrite its own
content/byte_range; arm-local because each arm holds its own cursor clone),
records via `scan_pointer`, drops via `void`. Arm 1: untouched main flow.
Merge output = arm 1 only.

12.7 Pure pass-through tag-ops (tag-op family contract, locks §10) do not
need fork-to-void. Inline is sufficient:

```sprf
> foo > $VAR > scan_pointer($VAR)
```

---

## 13. Last-bound Ans slot

13.1 Cursor gains one field:

```rust
pub struct Cursor {
    // ... existing fields ...
    pub last_bound: Option<Arc<str>>,   // name of the most recently written slot
}
```

13.2 Updated only by `CaptureWriteOp` (§11.1). Pass-through ops (tag-ops,
link-ops, void, filter ops) leave it untouched — bash `$_` semantics.

13.3 Spelling for reading it: `$$`. At arg or chain-ref position, `$$`
resolves to a `TermRef` pointing at `cursor.last_bound`.

```rust
// Lowered form of `$$` reference at use site:
ArgValue::AnsRef    // runtime reads cursor.last_bound then captures[that]
```

13.4 If `cursor.last_bound` is `None` at use: treated as an unbound
`TermRef`. Mode dispatch proceeds per op `ArgSpec` — if the op requires
`BoundOnly`, lower emits `ans-slot-empty` diagnostic. (Choice for first
pass; §19 open item for whether to diag at lower or defer to runtime drop.)

13.5 Fork per-arm isolation: falls out of cursor-is-cloned semantics. Each
arm evolves its own `last_bound`.

13.6 Carveout inheritance: `last_bound` flows into carveout body unchanged
(§9.2). Pattern sugar stays isomorphic to inline form.

13.7 Rule call boundary: open item (§19). Likely reset at rule body entry
because rule body is its own scope.

---

## 14. Scan-pointers as tag-ops

14.1 No syntactic scan-pointer class. The v2 `$$sigil` token is retired
(§8.5).

14.2 Scan-pointer tracking is a tag-op variant (locks §10):

```rust
// Per the tag-op family contract:
//   input   — cursor with referenced captures bound
//   reads   — captures[name] per arg
//   writes  — relations row with kind + src/dst
//   cursor  — passes through unchanged
pub struct ScanPointerOp {
    pub kind: Arc<str>,   // e.g. "repo", "rev", "fs", "repo_norm"
    pub arg:  Arc<str>,   // capture name to read
}
```

14.3 Built-in variants: `is_repo($R)`, `is_rev($T)`, `is_fs($F)`,
`is_repo_norm($R)`, `is_rev_norm($T)`. Each owns its kind and relation
writer logic.

14.4 Scan-pointer rows land in the `relations` table (locks §10):

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

14.5 Inline use:

```sprf
> foo > $VAR > is_repo($VAR) > ...
```

No fork, no sigil. Cursor flows; relations table gets a row.

---

## 15. Phase ordering: parse, lower, run

15.1 Three phases, three diagnostic classes (locks §6):

| phase | input                       | output             | diag class |
|-------|-----------------------------|--------------------|------------|
| parse | source bytes                | OpInvocation tree  | parse diag |
| lower | OpInvocation + op registry  | Pipeline + graphs  | lower diag |
| run   | Pipeline + runtime ctx      | RunEvent stream    | run diag   |

15.2 Parse builds syntax; each op's `parse` hook drives sub-grammar parsing.

15.3 Lower:

- resolves every name to `EntityRef`
- builds `BindingGraph: HashMap<TermPath, Vec<BindingSource>>`
- builds `StageDeps: { reads, writes, path }` per rule stage
- checks ArgSpec vs call-site modes
- detects rule cycles via Tarjan on the call graph

15.4 Run: executes lowered `Pipeline` against the runtime. Unbound
term-refs park cursors; upstream close drops them (never throw).

---

## 16. Binding graph and mode dispatch

16.1 Every term reference must have at least one binding source. Five source
kinds (locks §6):

```rust
pub enum BindingSource {
    Param(RuleId, usize),
    WalkerBody(OpCallId),
    ChainStageEmit(StageId),       // §11
    ForkArmAncestor(ArmId),
    ParametricCallProducer(CallSiteId),
}
```

16.2 Mode dispatch at op call:

```rust
pub struct ArgSpec {
    pub name:    Arc<str>,
    pub accepts: AcceptsMode,
}

pub enum AcceptsMode {
    BoundOnly,   // error if unbound at call site
    UnboundOnly, // error if bound
    Either,      // op dispatches per mode
}

pub enum TermMode {
    Bound(Value),
    Unbound,     // with SlotKey for write-back
}

pub trait ArgModeDispatch {
    fn dispatch(
        &self,
        ctx:    OpCtx,
        modes:  &[TermMode],
        cursor: &Cursor,
    ) -> OpAction;
}

pub enum OpAction {
    EmitBound(Cursor),
    IterateRegistry(BoxStream<Arc<Cursor>>),
    Filter,
    Diagnose(Box<dyn Diagnostic>),
}
```

16.3 `Operator::signature() -> Vec<ArgSpec>` declares expected modes; resolver
checks at lower. Runtime computes `TermMode` per arg per cursor; op's
`dispatch` decides behavior.

---

## 17. Diagnostics surface

17.1 Core trait (sprefa/src/diagnostic.rs):

```rust
pub trait Diagnostic: Send + Sync {
    fn code(&self)     -> &'static str;
    fn severity(&self) -> Severity;
    fn primary(&self)  -> &ParseSite;
    fn render(&self, out: &mut dyn Renderer);
}

pub trait Renderer {
    fn primary(&mut self, site: &ParseSite);
    fn related(&mut self, site: &ParseSite, message: &str);
    fn note(&mut self, message: &str);
}
```

17.2 New codes introduced by this spec:

| code                          | phase  | where                              |
|-------------------------------|--------|------------------------------------|
| `bare-dollar-without-target`  | parse  | §8.2                               |
| `deprecated-scan-sigil`       | parse  | §8.5                               |
| `xref/empty-join`             | run    | existing; stays                    |
| `ans-slot-empty`              | lower  | §13.4 (first-pass tentative)       |
| `unresolved-term-ref`         | lower  | §16, no binding source             |
| `capture-write-bad-position`  | parse  | §11, `$X` in non-chain-step spot   |

---

## 18. First-pass implementation scope

18.1 Land together:

1. §8.2 lex rule for unified `$`, including `Carveout` token emission.
2. §8.3 balanced-brace pre-pass at lex time with carveout-kind classification.
3. §8.4 `${{...}}` shell escape.
4. §8.5 `$$sigil` deprecation diag; `$$` token claim for Ans.
5. §8.6 eager parser re-entry producing `Arc<Pipeline>`.
6. §9 `CarveoutOp` with narrow + inner + rebase runtime.
7. §10 `rule.$V` as ordinary dotted-access host_expr; delete `parse_cross_ref`.
8. §11 `CaptureWriteOp` for bare-`$IDENT` chain step (annotate-only).
9. §12 `VoidOp` registration.
10. §13 `last_bound` field on Cursor + `$$` resolution at parse/lower.

18.2 Explicit non-goals for first pass:

- parametric rule params (`rule(:name, $P1, $P2)`)
- `assert(:name) { ... }` decl
- `@recursive` attribute
- casing enforcement (§7.3)
- `&{computed}` address-grammar carveout
- higher-order control ops (retry, until, when, if_else, debounce, distinct_by)
- term annotations (locks §14)

18.3 Tests to reactivate:

- `rule_repo_rev` rewritten against `> $VAR + is_repo($VAR)`.
- `kitchen_sink.sprf` migrated to `rule(:name)` atoms + new carveout shape.
- Targeted new tests per §8, §9, §11, §12, §13.

---

## 19. Open items

19.1 `$$` at arg position when `last_bound == None`: diag at lower
(`ans-slot-empty`) vs silent drop at runtime. First pass: lower diag if
op's ArgSpec requires `BoundOnly`, silent unbound otherwise.

19.2 Multi-slot chain-step write `> $(X, Y)`. Out of first pass; record as
future sugar.

19.3 Does `CarveoutOp` always narrow to the lexical `source_range`, or can
inner ops replace the target? First pass: always lexical.

19.4 Path-seg structure for carveout: single
`PathSeg::Carveout { source_range }` vs nested op-chain. First pass: single
seg, opaque inner.

19.5 Ans-slot reset on parametric rule call. Probably reset at body entry;
confirm when parametric calls land.

19.6 Whether to store `last_bound` as `Option<Arc<str>>` (name) or
`Option<SlotKey<()>>` (erased handle). `Arc<str>` is simpler; `SlotKey` is
typed-slots-shaped. First pass: `Arc<str>`.

19.7 Casing enforcement timeline — locked as convention 2026-04-19 but
no implementation pressure yet.

19.8 Scan-pointer table schema location — today's `relations` table is
universal; if scan-pointer volume gets large enough to want its own table,
split it out.

---

## 20. Future: type transclusion (dogfood)

20.1 Every Rust snippet in this spec (Cursor, OpInvocation, Pipeline, Operator,
BindingSource, ArgSpec, Capture, ...) is a hand-pasted copy of code in the
crate. Drift between this doc and the types is a near-certainty with
manual maintenance.

20.2 Target shape: this file declares anchor-bracketed transclusion regions
whose contents are rendered from the corresponding Rust entity by a sprf
rule. The daemon keeps them in sync; changes flush through the mutation
effect pipeline with the usual approve / auto / LSP-code-action paths.

20.3 Anchor protocol (proposed):

```markdown
<!-- sprf:type Cursor path=v3/crates/sprefa/src/types.rs -->
```rust
pub struct Cursor { ... }
```
<!-- /sprf:type -->
```

20.4 Rule that drives it (sketch):

```sprf
rule(doc_type_sync) {
  > fs(r"**/parse.md")
  > marker(sprf:type, $ENTITY_NAME, $ATTRS)
  > &{ path_of($ATTRS) }                           # rebase cursor to source file
  > ast[rust] { struct $ENTITY_NAME { $$$BODY } }  # pick matching entity
  > render_into_marker                             # mutation: rewrite marker region
}
```

20.5 Mechanics that land this:

- `marker` op (already planned) extracts named comment-bounded regions.
- `render_into_marker` (new) is a `MutationEffect` implementer that produces
  a textual replacement; inherits `preview`, `reversible`, `source_range`
  slots from the effect trait.
- Approval flow rides the existing `MutationHandler` family — AutoApprove
  for CI, InteractiveCli for `-y` style confirm, LspPromptBridge for IDE
  code-actions.
- Bidirectional: editing the transcluded block in the markdown emits an
  effect back onto the Rust source, gated by the same approval surface.

20.6 Acceptance: this spec's `parse.md` is the zero-th test case. When
transclusion lands, every Rust snippet here becomes generated; drift
diagnostics fire when the source diverges from the transcluded text.

20.7 This is a downstream feature, not first-pass scope. Recorded here so
the maintenance debt on §3–§16 snippets is visible and tracked to the
tool that will eventually retire it.

---

*End of language spec. Update alongside the code; drift is worse than redundancy.*
