# V4 Docs

Generated file inventory: [V4 Docs TOC](./v4-docs-toc.md)

Reading order for getting back into v4 without replaying chat logs:

1. [V4 Language Vector](./v4-language-vector.md)
2. [V4 Design Guardrails](./v4-design-guardrails.md)
3. [V4 Core And Store](./v4-core-and-store.md)
4. [V4 CursorValue And WhereBytes Plan](./v4-cursor-value-where-bytes-plan.md)
5. [V4 Config And CLI](./v4-config-and-cli.md)
6. [V4 Checked Paths](./v4-checked-paths.md)
7. [V4 ghcache Integration](./v4-ghcache-integration.md)
8. [V4 rev Relation](./v4-rev-relation.md)
9. [V4 System Architecture Audit](./v4-system-architecture-audit.md)
10. [V4 Rule Query Semantics](./v4-rule-query-semantics.md)
11. [V4 SQL Rule Query Plan](./v4-sql-rule-query-plan.md)
12. [V4 Terse Relational Query Sessions](./v4-terse-relational-query-sessions.md)
13. [V4 Durable Mounted Query Plan](./v4-durable-mounted-query-plan.md)
14. [V4 Effect Output Retraction Plan](./v4-effect-output-retraction-plan.md)
15. [V4 Runtime Batching](./v4-runtime-batching.md)
16. [V4 Primitive Examples Plan](./v4-primitive-examples-plan.md)
17. [V4 V3 Parity Gaps](./v4-v3-parity-gaps.md)
18. [V4 Next Slices](./v4-next-slices.md)

## Milestone — recursion is reactive (2026-05-18 → 2026-05-19, main `294d60db`)

Plain version of what landed over the last few sessions. (The reactive
wire landed at `37bb93a5`; the cycle/self-reach surface form below
landed at `294d60db`.)

**Before:** a recursive rule (a rule that calls itself, e.g. "reach =
one edge, OR reach-then-one-more-edge") only worked from hand-written
Rust. At the `.sprf` surface it either did not close the loop or, once
it did, it never un-derived anything: delete a fact from the source and
the stale conclusions stuck around forever.

**Now:** you can write transitive closure / reachability / a reactive
state machine directly in `.sprf`, run it against a persistent
`--fact-db`, edit the source, re-run, and the answer re-converges. Rows
that lost their only reason to exist get retracted, not left behind.
Working example: [`v4/examples/reactive-reach-retraction.sprf`](../examples/reactive-reach-retraction.sprf)
(run it twice, delete a line between runs, watch `reach` shrink).

The three things that made it work, in order:

1. **Recursion runs at the surface.** A `rule(){ self?(...) > ... }`
   overload now loops to a fixed point during a normal `sprefa-run`,
   with a hard round cap so a runaway recursion fails loudly instead
   of hanging.
2. **Retraction is sound.** Every rule output is tracked back to what
   derived it. Re-running over changed sources removes conclusions
   whose support is gone, including transitively through the recursive
   loop, scoped so unrelated writes are untouched.
3. **The loop is wired into the reactor.** Each recursive rule now
   advertises itself and subscribes to the source tables it depends
   on, while structurally never subscribing to its own (or a
   co-recursive sibling's) output, so a change-notification can never
   feed itself into an infinite wake. A genuinely contradictory rule
   graph (a relation depending on its own negation) surfaces as a
   diagnostic instead of a silent wrong answer.

Follow-on (`294d60db`): cycle / self-reference is now a plain query
shape, not a workaround. `reaches?(N?, N)` means "the rows where this
rule's two columns are equal" — i.e. a node that reaches itself. No
separate `where`-equals step. This completes the term-name algebra:
the same term across two reads is a join across rows; a `?`-bound
term re-referenced in one read is equality within a row. Example:
[`v4/examples/reachability-cycle-imperative.sprf`](../examples/reachability-cycle-imperative.sprf).

### What this unlocks (graph theory in the language)

With recursion + stratified negation + retraction + cycle-as-surface,
graph-theory queries are now writable in `.sprf` itself, over real
repo corpora, not just in hand-written Rust:

- reachability / transitive closure (recursive overload),
- cycle detection and, from it, import-cycle / dependency-cycle
  (`reaches?(N?, N)`),
- mutual-reach / strongly-connected-ish membership
  (`reaches?(A?,B?) > reaches?(B?, A?)`),
- ancestor / descendant, connected-component-style grouping,
- "reachable but **not** through X", orphan / dangling-reference
  detection, version/rev drift — all via stratified antijoin.

The honest framing: recursion + stratified negation + retraction is
**stratified Datalog**, now expressible at the surface and evaluated
over a polyglot code/repo graph. sprf stopped being "match + project"
and became a recursive deductive query language.

### Where this sits next to Differential Dataflow

Same query *class*, different maintenance *cost curve*.

- Expressiveness: stratified Datalog with retraction, same class as
  Differential Dataflow (DD).
- Recursion evaluation: sprf loops the fused SQL to quiescence
  (semi-naive engine in `fixpoint.rs`; run path = `INSERT OR IGNORE`
  loop with a hard cap). DD does true differential (delta) iteration.
- Incrementality: sprf's fact-level retraction is incremental, but
  recursive closures **recompute per run by default**. DD maintains
  them differentially. The engine *can* cascade incrementally
  (`tests/retraction_ph6.rs`) — the gap is wiring, not model.
- Substrate: sprf is SQLite-backed fused SQL + effect_runtime,
  owner/demand-driven and *persisted + ad-hoc queryable + LSP-wired*.
  DD is a dedicated timely-dataflow operator graph with no notion of
  "ask the graph a new question later" without redeploying.

So today: sprf = stratified Datalog, recompute-by-default, reactive
recompute triggers. DD = stratified Datalog, differential
maintenance. The structural edge sprf keeps is that the graph is a
persisted, queryable, editor-integrated artifact, not a streaming
operator topology.

Not yet on by default: skipping the recompute entirely when nothing
changed. The code path exists behind `SPREFA_REC_INCREMENTAL=1` but
the staleness signal available today is corpus-wide, not per-rule, so
the safe default is "always recompute, never serve a stale closure."
**This is precisely the gap versus Differential Dataflow above:** a
per-rule source-attribution oracle is what turns recompute-by-default
into differential maintenance. Sharpening that signal is the next
step. Detail:
[`v4-recursion-surface-gaps.md`](./v4-recursion-surface-gaps.md).

Doc posture:

- `human-goals.md` is the current human-authored intent artifact.
- Current `v4/src`, `v4/tests`, and `v4/examples` are executable truth.
- `/Users/chrishafley/projects/sprefa-archive-20260428/README.md` and older `.sprf` files are historical target vocabulary only.
- Use SQL terms when SQLite has a 1:1 concept. Keep implementation trait-backed.
- Avoid new glossary terms unless they map immediately to existing project words.
