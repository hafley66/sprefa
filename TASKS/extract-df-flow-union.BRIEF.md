# Lane brief: FlowF, the interprocedural value-flow family (issue extract-df-flow-union)

First action: `git merge --ff-only 497b108ba7882bc2db5b393680be8c1f2535d9d6`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Do not ask a question before starting. Everything you need is below with a
`path:line`. If a receipt does not match what you read, say which one and stop.

## The decision, already made by Chris. You implement it, you do not re-open it.

`issues/extract-df-flow-union/item.md`, Decisions section:

> Fork B: cross-function value edges are a SEPARATE family (FlowF), own plane,
> own closure. Existing df rules keep their intra-function meaning; cross-wall
> walks opt in by naming the new family. Implementation dispatchable: glue =
> DfArg x resolved_edge x DfParam.

Fork A (a `DfEdgeKind::Flow` variant) is REJECTED: it would silently change the
meaning of every existing `closure(df_edge)` rule. Do NOT add a variant to
`DfEdgeKind` (`src/types.rs:625-634`). Do NOT delete its `// Flow(FlowEdgeKind),
// PENDING epic 5` comment line by replacing it with a variant; delete the
comment and point the doc at the new family instead.

## The gap, with receipts

| fact | receipt |
|---|---|
| `DfEdgeKind` has one variant, the union is a comment | `v6/sprefa-extract/src/types.rs:625-634` |
| the plane note calls Flow pending | `v6/sprefa-extract/src/types.rs:16-19` |
| the raw material exists on all langs | `DfArg` `src/types.rs:548-554` (`call`, signed `pos`, `arg`), `DfParam` `src/types.rs:536-542` (`node`, `pos`), both on `DfFAux` `src/types.rs:557-562` |
| the missing hop | nothing joins a caller `DfArg` to the callee's `DfParam`, though the resolved caller-site-to-callee edge already exists (`ProjectEdge<CallF>`, `src/types.rs:743-761`) |

## Scope, fixed. Do not widen it.

You land: the `FlowF` family types, the PURE join that produces its edges, the
wire flatten, the schema block, and a new test. You do NOT wire it into
`resolve_project`.

**`src/project.rs` is FORBIDDEN.** The dispatch that would call your join inside
`resolve_project` (`project.rs:134`) and stream the rows from the CLI is the
named follow-up: state it under a `Follow-up` heading in your PR body, naming
`project.rs:134`, `project.rs:456-497` (`RESOLVE_ARMS`) and
`src/bin/extract.rs`, with the exact call you would have written.

## Three deviations a previous run made. Do not repeat them.

1. It gave `FlowEdgeKind` FOUR variants (`ArgToParam`, `RetToCallRes`,
   `LambdaElem`, `LambdaRet`), citing a plan that does not exist. The decided
   union is THREE, exactly as spelled in §2 below: `ArgToParam`,
   `RetToCallRes`, `HigherOrder`. Adding or renaming a variant is language
   design, which belongs to Chris, not to this lane.
2. It wrote a doc comment saying FlowF is "DERIVED, never extracted ... the
   engine closes the plane". False for this card. The join is a PURE FUNCTION IN
   THIS CRATE (`src/flow.rs`, §3 below), computed over two already-extracted
   `ExtractOutput`s. What is deferred is only the `resolve_project` dispatch.
3. It left a 12-line comment run with no waiver, which the pre-commit rail
   rejects. See the rail law at the bottom.

## The pinned design. Every judgment call below is already made.

### 1. `FamilyTag::Flow`, in `src/types.rs` (the enum is at `:92-98`)

Add one variant, `Flow`. Nothing matches `FamilyTag` exhaustively in `src/**`
(it is compared with `==`), and outside the crate only `if let` reads exist
(`v6/sprefa-engine-rs/src/dep_resolve.rs:560`,
`src/source_bind/_1_runtime.rs:397`).

**ONE exhaustive match exists, in a test, and editing that ONE LINE is
AUTHORIZED** (a previous run of this lane found it and correctly stopped).
`tests/golden_parity.rs:196` reads `FamilyTag::Cst | FamilyTag::Module => {}`
inside the `FlatFact::Node` arm. Change that line to:

```rust
                FamilyTag::Cst | FamilyTag::Module | FamilyTag::Flow => {}
```

Nothing else in `golden_parity.rs` may change: no new test, no assertion edit,
no `PORTED` change, no comment rewrite. Say in the PR body that the arm was
widened under an explicit exception, because FlowF is phase-2 only and never
appears in a phase-1 flatten. The `FlatFact::Edge` match at `:200-206` already
carries `_ => {}` and needs nothing.

Any OTHER exhaustive match you find is still a report-and-stop.

### 2. `FlowF`, in `src/types.rs`, in the VALUE-FLOW plane section right after the `impl Family for DfF` block

```rust
pub struct FlowF;

/// Cross-function value edge kind.
pub enum FlowEdgeKind {
    /// A caller argument reaches the callee parameter in the same slot.
    ArgToParam,
    /// A callee return value reaches the caller's call-result node. The edge
    /// row is caller-local like every other one, so the VALUE travels dst to
    /// src for this kind alone.
    RetToCallRes,
    /// An argument that names a callable: the value reaching the parameter is
    /// the callable's definition.
    HigherOrder,
}
```

`as_str`: `arg_to_param`, `ret_to_call_res`, `higher_order`, matching the
`DfEdgeKind::as_str` shape at `src/types.rs:636-642`.

`impl Family for FlowF` with `type NodeKind = DfNodeKind` (FlowF declares NO
nodes of its own; both endpoints are DfF nodes), `type EdgeKind = FlowEdgeKind`,
`type Aux = ()` (the `CstF::Aux = ()` precedent, `src/types.rs:180-186`),
`const TAG = FamilyTag::Flow`. Derive exactly what `DfF` / `DfEdgeKind` derive.

FlowF gets **no `FamilyMask` bit and no `ExtractOutput` field**. It is phase-2
only, computed by a join over two already-extracted outputs. Adding a field to
`ExtractOutput` would break every lang file's exhaustive struct literal
(`src/types.rs:511-514` states that constraint). Say this in the family's doc
comment, one line.

### 3. The join, in a NEW module `src/flow.rs`, `pub mod flow;` in `src/lib.rs`

`deps.rs` is the precedent for a pure module beside `types.rs`. The types stay
in `types.rs`; only functions live here.

```rust
/// One resolved call: the caller-side site and the callee it reached.
pub struct CallCrossing<'a> {
    pub caller: &'a ExtractOutput,
    pub callee: &'a ExtractOutput,
    pub callee_blob: BlobHash,
    /// The caller's phase-1 call site span (`CallSite.span`).
    pub call_site: Span,
    /// The callee declaration's span in `callee_blob` (a CallF def node span).
    pub callee_def: Span,
}

/// `ArgToParam` + `RetToCallRes` edges for ONE resolved call. Pure; no index,
/// no IO.
pub fn call_flow_edges(crossing: &CallCrossing) -> Vec<ProjectEdge<FlowF>>;

/// `HigherOrder` edges for one caller blob: an argument whose name resolves to
/// exactly one callable definition in the corpus index.
pub fn higher_order_flow_edges(
    caller: &ExtractOutput,
    index: &DefIndex,
) -> Vec<ProjectEdge<FlowF>>;
```

Every edge is `ProjectEdge::new(src_noderef_in_caller, callee_blob,
dst_span_in_callee, kind)`, plus `.with_call_site(crossing.call_site)` on the
two kinds that have one. `src` is a `NodeRef` into the CALLER's `DfF` node vec;
say that in the doc comment, because `ProjectEdge.src`'s own doc
(`src/types.rs:750-751`) says only "local node in this file".

**The three join rules, pinned. Implement exactly these.**

1. `ArgToParam`. Caller side: every `DfArg` in `caller.df.aux.args` whose
   `call` node's span is the call node for `crossing.call_site` (rule 4 below)
   and whose `pos >= 0`. Callee side: every `DfParam` in `callee.df.aux.params`
   whose node span is CONTAINED in `crossing.callee_def` and whose
   `pos == arg.pos as u32`. Edge: src = the arg's `NodeRef`, dst = the param
   node's span. A receiver argument (`pos == -1`) produces NO edge in v1; say so
   in one comment line.
2. `RetToCallRes`. Callee side: every `DfF` node of kind `DfNodeKind::Ret` whose
   span is contained in `crossing.callee_def`. Caller side: the call node from
   rule 4. Edge: src = the caller's call node `NodeRef`, dst = the ret node's
   span, kind `RetToCallRes`.
3. `HigherOrder`. For every `DfArg` in `caller.df.aux.args` with `pos >= 0`,
   take the arg node's interned `name`; if `index.map` holds that exact name
   with EXACTLY ONE site whose `family == CallF::TAG`, emit src = the arg's
   `NodeRef`, dst = that site's span, `dst_blob` = that site's blob. Two or more
   sites is ambiguous: skip it, never guess. Zero sites: skip. Read
   `DefIndex` / `DefSite` (`src/types.rs`, search `pub struct DefIndex`) and
   `containing_def_site` (`:1063`) before writing this; a `Resolve<CallF>` arm
   doing the same name lookup is at `src/lang/prolog/_0_source.rs:795`.
4. The caller's call node, used by rules 1 and 2: the `DfF` node whose kind is
   `CallRes` or `New` and whose span EQUALS `crossing.call_site`; if none is
   equal, the SMALLEST node of those kinds whose span contains the call site; if
   none contains it, the crossing produces no edges at all and returns empty.

Output order is deterministic: caller node order, then callee node order. No
`HashMap` iteration reaches the returned Vec.

### 4. Wire, in `src/wire.rs`

FlowF edges flatten through the EXISTING generic `FlatFact::ProjectEdge` arm
(`src/types.rs`, search `ProjectEdge {`) with `family: FamilyTag::Flow`. Add NO
new `FlatFact` variant. Write `flatten_project_flow`, mirroring
`flatten_project_type` at `src/wire.rs:204-234` exactly (same signature shape,
same `BlobHash` hex spelling).

### 5. Schema, in `src/schema.rs`

The generic project-edge row is currently UNDOCUMENTED in `SCHEMA`. Add its
record shape line in the RECORD SHAPES block, next to `record=resolved_edge`
(`schema.rs:35`), using the EXACT record tag serde emits. Determine that tag by
reading the `#[serde(tag = "record", rename_all = "lowercase")]` attribute on
`FlatFact` and confirming it in your new test with one `serde_json::to_string`
assertion. Do NOT rename the variant or add a `#[serde(rename)]`: the tag on the
wire is what existing consumers already read.

Then add to the `family` field description (`schema.rs:61-62`) and to the KIND
VOCABULARIES block (`schema.rs:110-122`):

```
  flow edge   arg_to_param ret_to_call_res higher_order
```

### 6. Status table, `src/types.rs:1861-1870`

The DEFERRED table's Flow row: state that the family landed and that the
`resolve_project` dispatch is the follow-up. No dates, no arc names.

## The test, a NEW file `v6/sprefa-extract/tests/17_flow_union.rs`

New fixtures under `v6/sprefa-extract/tests/fixtures/flow/` (new directory):
`caller.ts` and `callee.ts`, each under 25 lines. The callee exports one
function with two annotated parameters and one return; the caller imports it,
calls it with two arguments, assigns the result, and separately passes a local
function by name as an argument.

The test:
1. extracts both files with `FamilyMask::ALL`;
2. asserts, as literals, the call site span and the callee def span it feeds the
   crossing (derive them by hand from the fixture bytes, never from the output);
3. asserts the EXACT `Vec<ProjectEdge<FlowF>>` from `call_flow_edges`: one
   `ArgToParam` per slot, one `RetToCallRes`, each `(src span, dst span, kind)`
   spelled out;
4. asserts `higher_order_flow_edges` finds the function-valued argument and
   emits exactly one edge;
5. asserts the flattened JSONL line of one edge byte-for-byte, which pins the
   record tag and the `family: "flow"` spelling;
6. asserts an ambiguous name (add a second same-named callable to `caller.ts`)
   produces NO `HigherOrder` edge.

**Fail-first receipt, required.** Write the test first against the unimplemented
join, run it, paste the red output into the commit body. Then land the join and
paste the green.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 17_flow_union; echo "FLOW rc=$?"
cargo test --features cli --test 12_df_identity; echo "DFID rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. A leg that differs between runs is a report-and-stop.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/types.rs`
- `v6/sprefa-extract/src/wire.rs`
- `v6/sprefa-extract/src/schema.rs`
- `v6/sprefa-extract/src/flow.rs` (new)
- `v6/sprefa-extract/src/lib.rs` (module + re-export lines only)
- `v6/sprefa-extract/tests/17_flow_union.rs` (new)
- `v6/sprefa-extract/tests/fixtures/flow/**` (new)

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`
- every `src/lang/**` file (READ them freely; edit none)
- `v6/sprefa-extract/src/bin/extract.rs`
- every EXISTING file under `v6/sprefa-extract/tests/` (new test files only), with
  the ONE authorized line in `tests/golden_parity.rs:196` as the sole exception
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- `v6/sprefa-engine-rs/src/hosts.rs`, `v6/tsv2/goldens/scip_combo/**`
- everything outside `v6/sprefa-extract/`

## Laws that bind you

- Never spawn a subagent.
- Every public type lives in `types.rs`, the canonical type module. Functions in
  `flow.rs`.
- Comment budget: comments state only constraints the code cannot show. No
  change-log narrative, no dates, no arc or commit references.
- Identifiers are descriptive, never single-letter.
- No em dashes. Banned in prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" is banned in prose; say TODO or not built yet.
- No `eprintln!` under `src/**`; `tracing` only.
- N+1 law: collect the set, one push loop. No per-row allocation of an index.
- **The pre-commit rail.** `.githooks/pre-commit` runs
  `v6/tsv2/scripts/comment-budget-rail.sh`: MORE THAN 2 CONSECUTIVE COMMENT
  LINES in a staged hunk FAILS the commit. The waiver is one line
  `// @comment-ok: <one-line reason>` inside the offending run; two examples are
  in `v6/sprefa-extract/src/types.rs`. Prefer shrinking the comment to 2 lines
  over waiving. Never `git commit -n`, never edit the hook or the rail script.
- Commit in slices (types, join, wire+schema, test). Use
  `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `feature/extract-df-flow-union` in your worktree.
2. Commit in slices.
3. `git push -u origin feature/extract-df-flow-union`
4. `gh pr create` with a body carrying: the decision citation, the three join
   rules as written, the fail-first red and the green, both gate runs with
   per-binary counts, the exact record tag you documented, and a `Follow-up`
   heading naming the `resolve_project` dispatch.
5. The PR body ends with the trailer line: `Refs-Issue: extract-df-flow-union`
6. NEVER merge the PR. Report the URL and stop.
