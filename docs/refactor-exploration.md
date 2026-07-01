# Refactor exploration with dl — a formula

A repeatable recipe for pointing the dl engine at a big/"god" file and getting
*objective* refactor signal, plus the validation gauntlet that tells you which
signal to trust. Derived from a real session over `src/engine.rs` (7168 lines,
236 methods, one ~4766-line `impl Engine`). The headline lesson: **most cheap
metrics mislead; validate before you trust, and the signal that survived points
at types (traits), not at file cuts.**

## 0. The reusable extraction idioms (dl, self-contained)

Everything below is built from `match` over the target file — no SCIP, no index.
Two idioms do all the work.

**Decls + call sites + nearest-decl attribution.** A call/marker on line `cl`
belongs to the nearest `fn` decl at or above it (no other decl in between). This
is the same shape the `recompute-guard` rail uses.

```
rel m(name, line).
m(n, l) <- scan("WORK","src/engine.rs",f,rev),
  match(f,rev,/(?m)^\s*(?:pub(?:\([a-z_]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(?P<n>[A-Za-z_]\w*)/, l).

rel csite(line, callee).
csite(l,c) <- scan("WORK","src/engine.rs",f,rev), match(f,rev,/self\.(?P<c>[a-z_]\w*)\s*\(/, l).

# enclosing method = nearest decl above, expressed as above-minus-has-closer
rel above(mline, mname, cline, callee).
above(ml,mn,cl,ce) <- csite(cl,ce), m(mn,ml), ml <= cl.
rel closer(mline, cline).
closer(ml,cl) <- above(ml,_,cl,_), m(_,mid), ml < mid, mid <= cl.
rel call_raw(caller, callee).
call_raw(mn,ce) <- above(ml,mn,cl,ce), !closer(ml,cl).
```

**The call graph + community embedding.** `call_edge` is a reserved builtin —
name yours `cedge`. node2vec needs a 2-col edge rel; symmetrize for community
structure.

```
rel cedge(caller, callee).
cedge(a,b) <- call_raw(a,b), m(b,_), a != b.        # callee is a real method, no self-loop
rel g(a,b). g(a,b) <- cedge(a,b). g(b,a) <- cedge(a,b).
rel node_sim(a,b,score). node_sim(a,b,score) <- node2vec(g).
rel scc_cluster(rep,member). scc_cluster(rep,member) <- scc(cedge).
```

Run with `--no-daemon` (a running daemon hijacks ad-hoc programs) and cap
node2vec: `SPREFA_N2V_DIM=64 SPREFA_N2V_SEED=1 SPREFA_NODE_SIM_K=6`.

## 1. The signal menu (question → signal → verdict)

| you want to know | signal | how | verdict from the engine.rs run |
|---|---|---|---|
| who orchestrates / who's a shared leaf | call-graph fan-in/out | `count` over `cedge` | useful (found tick/tick_paths hubs, refresh_rel leaf) |
| which methods own which state | **field coupling (LCOM)** | `match self\.(field names)` → method↔field graph | **useful** — clean sub-struct candidates (repos, closure_cache, gen_journal) |
| soft "these belong together" | node2vec on `cedge` | components of `node_sim` ≥ 0.90 | recovers families, but groups by *role* (all call refresh_rel) |
| hard mutual-recursion knots | `scc(cedge)` | multi-member SCCs | **0 here** — the self-call graph is a DAG (cuts won't break cycles) |
| near-duplicate code | data coupling | method↔table/rel-name graph, node2vec | found the `*_rels_used` ×20, `*_rel_decls` ×21 dup families |
| is the file even consistently named | name-prefix coverage | group decls by first 1-2 name tokens | 35% here; sqlite3.c 90%, a JS god-file 20% |
| **what TRAITS are latent** | **signature-shape buckets** | group methods by `(receiver, #args, return)` | **the winner** — see §3 |
| where to cut with least call traffic | seam profile | `examples/call-seams.dl` | periphery cuts free; impl core has no cheap cut |

## 2. The validation gauntlet — run BEFORE trusting a metric

A metric that merely *varies across files* is not useful. Two cheap tests killed
the obvious metric ("name-scatter") in this session:

- **Scatter vs random placement.** Compute a family's interleave, then the
  expected interleave if the same family sizes were placed at random positions
  (shuffle ~200×). Ratio `actual/random`: ~1.0 = names ignored by layout, ~0 =
  perfectly clustered. engine.rs scored **0.32** — already 3× more clustered than
  chance, so "the layout ignores the names" was simply false.
- **Call-distance under reordering.** Median `|rank(caller) − rank(callee)|`
  over `cedge` for: current layout, name-family sort, random. engine.rs: current
  **18**, name-sort **60**, random **80**. Reordering by name *destroys* call
  locality — the file is already laid out by who-calls-what, a better principle
  than the names. The "gather by name" refactor would have hurt.

Rule of thumb: any layout/cluster metric must beat a random baseline AND not
worsen call-distance. If it fails either, it is cosmetic.

## 3. What actually survived: signature-shape → traits

The useful frame for "imperative soup on a god-struct" is not *split the file*,
it is *extract the traits*. Bucket every method by normalized signature
`(receiver, arg-count, return)`; the big buckets are trait/registry candidates.

On engine.rs this exposed three latent traits:

- **`RelKind`** — 19 rel families each repeat `X_rel_decls()` + `X_rels_used(prog)`
  + `refresh_X_rel()` (≈60 methods) dispatched by **106 hand-written
  `self.refresh_*` calls**. Collapses to one trait, 19 impls, one registry loop.
- **`BodyOp`** — `scc`/`node2vec`/`closure` evaluators share
  `fn(&mut self,&Rule)->Result<()>`; replaces the per-operator `for` loops.
- **`DeclProvider`** — 23 `fn()->Vec<_>` decl producers.

Plus sub-structs from §1 field coupling (RepoRegistry, ClosureCache, GenJournal,
RevResolver) and ~40 already-`assoc` (stateless) methods that are free functions
wearing an `impl`.

**The reframe that mattered:** node2vec's pass-1 "refresh_* supercluster" looked
like noise ("same role, low value"). Same role *is* same trait. The embedding was
pointing at `RelKind` from the start; the wrong frame (file-splitting) hid it.

## 4. Artifacts

- `examples/call-seams.dl` — the surviving *file-cut* tool: per-method-boundary
  count of straddling call edges (`seam`) + which methods bridge a cut (`bridge`).
  Low `seam.n` = a clean cut line. Edit the scan glob to retarget.
- `examples/recompute-guard.dl` — the static rail the attribution idiom came from.
- Throwaway exploration programs + python harnesses (scatter-vs-random,
  call-distance, signature-shape buckets, the cross-file god-file survey) lived in
  the session scratchpad; reconstruct from the snippets above — they are all
  `match`-extract + a short aggregation.

## 5. Cross-file calibration (so the numbers mean something)

Same measure on other on-disk god-files, with a generated control:

| file | kind | naming coverage | name-scatter |
|---|---|---|---|
| `oxc ast_builder.rs` | generated | 92% | 3.6 (control: emitted in order) |
| SillyTavern `world-info.js` | hand | 20% | 14.9 (no convention to exploit) |
| `engine.rs` | hand | 35% | 14.3 |
| `sqlite3.c` (real `sqlite3*`) | hand, amalgamation | 90% | 21.8 |
| SillyTavern `script.js` | hand god-file | 41% | 32.8 |

Reading: high coverage + high scatter → the modules are named, the layout is a
separate question (don't auto-trust "gather"); **low coverage (≤25%) is the only
regime where the expensive embedding earns its keep** — there the names carry no
structure to recover. Reserve node2vec for that quadrant.
