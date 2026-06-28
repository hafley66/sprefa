# High-fidelity call/dataflow from entry points — problem breakdown

The ask: "best fidelity call/dataflow analysis from a known entry point or a
selection of points in the dataflow of the program." This decomposes the problem
into its real axes, marks where the v5 engine already is on each, and names the
gates to fidelity. It is a breakdown, not yet an impl plan.

## The unit you're circling: a program SLICE from a seed set

Everything below serves one primitive: **given seed(s) S and a direction, return
the slice = the subgraph of program points reachable from S over a chosen edge
layer, with the witness path(s).** "Blast radius" is the forward slice; "where did
this come from / who can break this" is the backward slice. The engine already has
the *reachability boolean* (`reaches()`/`closure()`, seeded BFS over the SCC
condensation). The fidelity work is everything that makes the slice (a) over the
RIGHT edges, (b) resolved by the RIGHT oracle, (c) returned as PATHS not pairs.

## Six axes

| # | Axis | Options (low→high fidelity) | Where v5 is | The decision |
|---|------|------------------------------|-------------|--------------|
| 1 | **Seed model** | a fn sym → a call site → a variable@line → a param slot → a type → a spec op; single vs a SET of points | seed = a symbol string (any node id in an edge rel); BFS seeds off it | one uniform seed grammar across {fn, site, var, slot, type, op} |
| 2 | **Direction** | forward (influences) / backward (influenced-by) / both (chop between two frontiers) | `reaches_from` + `reached_by` both exist (engine.rs:4853) | expose both + a "chop" (S→T slice) query |
| 3 | **Edge layer** (the fidelity ladder) | call graph → type graph → intra-proc df → **inter-proc df** → field/alias-sensitive | call_edge (Rust only today), type_link, df_edge (intra-proc, all 3 langs) | which layers union into the walked graph per query |
| 4 | **Resolution** (dispatch precision) | syntactic name-unique → SCIP/compiler → virtual dispatch (iface→all impls) | name-unique; SCIP override when index present; **no is_implementation** | how a virtual call resolves: one target, all impls, or oracle-backed |
| 5 | **Answer shape** | reach (bool) → count → witness path(s) → full slice subgraph → ranked/weighted | reach + the dst set; no path witness, no weight | do we need PATHS (yes, for the "1 message" report) and WEIGHTS (cousins) |
| 6 | **Posture + seam + scale** | over-approx (never miss) vs under-approx (only definite); cross-lang seam; on-demand vs full-materialize | df is under-approx ("may miss, never invents"); cross-lang via shared string/SCIP; seeded BFS = on-demand | over- vs under-approx per use; how cross-lang edges are minted |

## The two gaps that actually gate fidelity

Everything else is plumbing; these two are the substance.

### Gap A — interprocedural dataflow stitch (turns "call reachability" into "value flow")
Today `df_edge` is **intra-procedural** (per `DataflowFacts` doc: no SSA, no
arg/return stitching). So a forward slice crosses functions via `call_edge`
(control), but the VALUE doesn't flow: caller's arg → callee's param, and callee's
return → caller's call-result node, are not edges. The stitch:
- at each `call_site`, for arg position i: `df_edge(arg_i_node, callee_param_i_node)`.
- callee `return`/tail expr node → the `call_res` node at the site.
- requires: resolved callee (axis 4) + param-slot identity (already have `type_sig`
  slot model) + the call_site→def join (the engine's containment pass).
This is the single highest-leverage build for "real dataflow blast radius."

### Gap B — dispatch resolution (interface → all impls)
A call to an interface method is, soundly, a call to EVERY implementor. Without
this, a backward slice from a concrete impl misses callers who only see the
interface, and a forward slice from an interface caller misses the impls. Inputs:
- our `type_edge(impl, iface, "impl")` (have it, syntactic, cross-lang).
- SCIP `is_implementation` relationship (gap in `scip_import.rs` — reads
  occurrences only) → `scip_impl(impl_sym, iface_sym)`, compiler-accurate, the
  right answer for Kotlin/Java cross-module soup.
- the rule: a virtual call `x.m()` where `x: IFace` → call edges to `m` of every
  `impl_of(_, IFace)`. Over-approx by default (the safe blast-radius posture).

## What I think you actually want (synthesis)

> From a **seed** (an op handler, a request-client call site, a variable, a type,
> or a set of them), get the **forward + backward slice** over a **layered,
> oracle-resolved** graph (call ⊕ type ⊕ inter-proc df, virtual calls fanned to
> all impls), returned as **ranked witness paths**, rendered as a **single focused
> report** (the "request client vs its handler in one message, links + lists") and
> a **d2/atlas visual** — and it works **across languages** because the seam edges
> (spec op-id, SCIP symbols) are first-class graph nodes.

Concretely against your three named goals:
- **OpenAPI client vs handler**: seed = a spec op; forward slice over `spec_edge ∪
  handler-bind ∪ inter-proc df` = the handler's request/response data path;
  backward slice over `client-call ∪ df` = who sends it. One report, two slices.
- **Kotlin interface soup**: Gap B (`scip_impl`) gives compiler-accurate impl fan;
  the `singleton`/`iface_count` smell already ships; the slice shows whether the
  abstraction is ever crossed by two different impls in real flows.
- **blast radius across langs**: Gap A (stitch) + axis-3 layer union + axis-4
  oracle = a value-level forward slice from any entry point, cross-lang.

## Decisions that fork the design (need your call)

1. **Posture**: over-approx (sound blast radius — "everything that COULD be
   affected", virtual calls fan to all impls) vs under-approx (only definite
   flows). Likely: over-approx for blast radius, under for "where did this exact
   value come from". Per-query flag?
2. **Path witnesses**: closure()/SCC gives reachability pairs, not paths. Do we
   add a path-reconstruction pass (BFS parent pointers on the condensation), or is
   the slice subgraph (all edges among reachable nodes) enough for the report?
3. **Resolution oracle default**: syntactic-when-no-index vs require-SCIP for the
   high-fidelity mode. (SCIP gives is_implementation + accurate dispatch but needs
   a per-repo index build.)
4. **Seed grammar surface**: a built-in query form (`slice(seed, dir, layers)`) vs
   a pure-DSL recipe the user assembles from `reaches` + the layer rels. DSL-first
   keeps the engine generic (matches the flow_member overlay decision).

## Build order implied (once decisions land)
1. Kotlin/TS `call_edge` (IN PROGRESS, subagent) — unblocks cross-lang call layer.
2. `scip_impl` from SCIP `is_implementation` (Gap B) — dispatch + Kotlin soup + oracle.
3. inter-proc df stitch (Gap A) — arg→param / return→site edges; the value-flow core.
4. slice query surface + witness paths (axes 2/5) — the report primitive.
5. the "1 message" report + d2 slice visual; SCIP oracle test that the slice hits
   are a subset of the compiler's reference graph (precision) + a recall snapshot.
