# Graph reachability, clustering, and auto-refactor

A concept sketch for turning the engine's existing graph relations into
reachability / clustering queries that drive automated refactoring. Not a
spec — a map of what's already in the tree vs. what's missing, so the idea has
somewhere to land.

## The substrate (already built-in)

The engine already materializes a rich set of graph relations per tick, lazily
(gated on use):

| relation | graph |
|---|---|
| `type_edge(from, to, kind)` / `_rev` | type graph: field/variant/impl/generic (Rust syn, Kotlin ts, TS oxc) |
| `call_edge(caller, callee, kind)` / `_rev` | resolved call graph (caller-sym → callee-sym) |
| `call_site(caller, callee, file, line)` | call occurrences (line-scoped) |
| `df_node(id, kind, var, fn, …)` / `df_edge(from, to)` | intra-procedural dataflow |
| `node(id, kind, file, lo, hi, parent)` / `child(parent, child)` | the CST as a relation (christmas #3, done) |
| `loop_over`, `nest(call_id, loop_id, depth, …)`, `allocates(fn)` | loop/nesting/alloc — raw material for symbolic Big-O |
| `module_edge(src, dst)` / `crate_edge` | file/file + workspace import graphs |
| `ref(id, string, file, lo, hi)` + `node_file_span_idx` | byte-located rewrite coordinates |

And one cross-cutting operator already on top of them:

- **`closure(edge)`** — transitive reachability over any edge relation, via the
  SCC condensation (cached across ticks, recondensed only on row churn). This is
  *the* reachability primitive: `reaches(X, Y) <- closure("call_edge"), …` already
  answers "can A reach B through the call graph." Works for `type_edge`,
  `df_edge`, `module_edge` alike.

So **transitive reachability is already expressible** for every graph the engine
builds. What's *not* there: the higher-order questions that make refactoring
automatic.

## The missing layers

### 1. Clustering / cohesion (the "who groups with whom" question)

**Most of this is free today** — the earlier draft over-stated #12's role. Concretely:

- **SCCs (mutually-recursive clusters)** are two `closure` reads, no builtin:
  ```
  reaches(a, b) <- closure("call_edge").
  scc(a, b) <- reaches(a, b), reaches(b, a).
  ```
- **Cluster id** (a canonical rep per SCC) is the `min` aggregate, which exists:
  ```
  cluster(rep, member) <- scc(member, other), rep = min(other).
  ```
  (`min`/`max`/`count` are all first-class: `fan_out(f, count(t)) <- edge(f, t).`,
  see `gen-type-table.dl:21`.)
- **Fan-in / fan-out** are `count` per group — also already in examples
  (`lint-docs.dl:37 call_refs(s, count(caller))`).
- **Boundary edges** (an edge whose endpoints sit in different clusters):
  ```
  boundary(caller, callee) <- call_edge(caller, callee, _),
      cluster(ca, caller), cluster(cb, callee), ca != cb.
  ```
- **Edge weight** (call count) is `count` over `call_site(caller, callee, file, line)`.

So an **extraction-candidate report** ("SCCs whose internal edge count dwarfs their
boundary — cohesive islands with a narrow surface") is writable as a pure `.dl`
program right now, dogfooded on sprefa itself. **#12 (rank/row_number/nth) is NOT
the blocker for clustering** — `count`/`min`/`max` cover it. #12 only matters for
fresh-name generation ("-2"/"-3" suffix collision) and "Nth occurrence / keep-first",
which are edit-side concerns (#4), not discovery.

What `closure` + aggregates genuinely can't do:
- **Community detection** (Louvain / label propagation) — non-monotonic, iterative,
  not a Datalog fold. Out of scope as a builtin; a `cmd` shell pass (dump
  `call_edge` → run a clusterer → read back `cluster(node, id)`) is the escape
  hatch, and is now reachable end-to-end via the data-driven scan + repo-sink
  (#1, done).

### 2. Scope and binding (the "what does this see / own" question)

Christmas #10 (partial): the CST gives properly-nested spans, so "innermost node
covering byte C in file F" is an indexed range scan. But **binding/visibility and
free-variable analysis are open.** That's what you need to answer:

- "Is name X in scope at coord C" (name resolution).
- "What does byte range [lo,hi) reference but not bind" (free vars — the
  extraction target for a move).
- "Which decl owns this use" (the scope tree).

Needs: #3 (CST, done) + binding rules (an external index or a per-language
decl/use walker) + the interval index (done). The free-var set is the input to
any "extract this range and fix its imports" refactor.

**MVP shortcut (avoids #10):** a move's import-fix does not strictly need name
resolution. "What does fn F reference that lives elsewhere?" is answerable from
the already-built coarse relations: `type_edge(F, T, _)` gives the types F
references, `module_edge(M, T)` (or the decl's file) gives where T lives, and the
fix is a synthesized `use M::T;` insert into the destination. This is imprecise
(unused imports may slip in; fully-qualified paths aren't handled) but it makes a
first move-and-fix loop runnable WITHOUT #10. True scope/binding retires the
imprecision and unlocks local-variable moves, not the first cut.

### 3. Edit algebra (the "act on it" question)

Christmas #4 (open): the sink is replace-one-span only. Auto-refactor needs:

- `insert_before` / `insert_after` / `delete` / `wrap` operators.
- A multi-edit transaction with **overlap detection** (two edits to the same span
  bail loudly — mirrors the gen two-writers rule).
- File-level sinks (#13): create / rename / delete, beyond what `--move`
  hardcodes.

Located rewrite coordinates already exist (`ref`/`node` byte spans + `_where_bytes`),
so the edit targets are first-class. The missing piece is the algebra of edits +
a verified transaction.

### 4. Verify-rollback (the "did it work" question)

Christmas #14 (open): apply the edit to a scratch worktree, run a checker via the
`cmd` op, keep only if it passes. The reactive tick + `changed_line` rails already
give the "what did the edit touch" half; the scratch-worktree + keep-if-pass is
the missing harness.

## The refactor loop this enables

```
discover   : scan + type_edge + call_edge + df_edge + node/scope
cluster    : comp(id, node) + cohesion metrics  (#12 + a component relation)
propose    : "these 5 fns form an island with high internal cohesion and a
              narrow boundary → extract module / move to file F"
edit       : edit algebra (#4) over the located spans, multi-edit txn
verify     : scratch worktree + checker (#14), keep-if-pass
```

Each layer is a christmas-list item; the substrate is done. **Discovery is free
today** (closure×2 + count/min, see §1) — the first useful increment is a pure
`.dl` program that emits extraction-candidate clusters from `call_edge`, no new
engine code. Acting on a candidate is where the open items land:

| step | needs | status |
|---|---|---|
| discover (SCC + fan-in/out + boundary) | `closure`, `count`, `min` | **free now** |
| propose (emit a report / TODO list) | `gen` (splice) | **free now** |
| cut + paste a fn (delete span, insert into dest) | #4 edit algebra | open |
| new file for the extract | #13 file sinks | open |
| fix imports at the destination | `type_edge` + `module_edge` (coarse) | **free now** (MVP); #10 for precision |
| keep-if-it-compiles | #14 verify-rollback | open |

So the **minimal auto-refactor that actually moves code** = discovery (now) + #4
+ #13. Import-fix leans on existing type/module edges. #10 (scope binding) and
#12 (window aggregates) are NOT on the critical path — they refine, they don't
gate.

## Ties to the christmas list

- #1 data-driven scan — **done** (8240126). Makes the `cmd` shell-pass escape
  hatch (dump edges → external clusterer → read back) reachable end-to-end.
- #3 CST-as-relation — done. #9 whole-match span — done.
- #10 scope — partial (interval + point index landed; binding/free-var open).
  **NOT on the auto-refactor critical path** — type_edge/module_edge cover the
  MVP import-fix; #10 adds precision + local-var moves.
- #12 window aggregates — open. **NOT the clustering blocker** (count/min/max
  suffice for SCC + fan-in/out); only blocks fresh-name suffixing + keep-first.
- #4 edit algebra — open. **The real gate for moving code.**
- #13 file sinks — open. #14 verify-rollback — open.

## Open questions

- SCC-pairwise (`scc(a,b) <- reaches(a,b), reaches(b,a)`) materializes O(n²) pairs
  for a big cluster — fine for sprefa-scale, maybe not for rust-analyzer-scale.
  A `comp(id, node)` builtin (Tarjan, like `closure`) may pay off later; not now.
- Rust path resolution after a move: `mod.rs` vs `mod/`, re-exports, `crate::`
  paths. The coarse type_edge/module_edge fix handles `use` insertion; deeper
  path rewriting is #10-territory or a language-specific pass.
- When two proposed edits overlap the same span, bail loudly (mirror the gen
  two-writers rule, christmas #27) — the multi-edit txn in #4.
