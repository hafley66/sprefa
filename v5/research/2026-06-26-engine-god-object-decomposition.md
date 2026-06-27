# Engine god-object diagnosis: coupling scores + field-coverage decomposition

Date: 2026-06-26. Method: prebuilt `v5/target/debug/dl`,
`SPREFA_SCIP_INDEX=.../index.scip`, `--root v5`, throwaway `--db /tmp/*.db`,
`--no-daemon`. All measurements on the RA-resolved oracle (`scip_fn_edge`,
`scip_callee_type`): 100% recall, no heuristic noise. Pure dl; no new Rust
extraction was needed for any finding below (the `scip_*` relations from B1
sufficed).

## Headline

Engine is a **god-session, not a bag of unrelated concerns.** Of its 110
methods, **44 touch only `self.db`** and nothing else on Engine — a clean
extractable data-access layer. The remaining 48 stateful methods share central
fields (`rels`, `root`, `repos`, `closure_cache`) and form a genuine mesh with
no structural seam. Splitting the 44 db-only methods out (behind a `Db`
collaborator) drops Engine 110 -> 66 and isolates the storage concern without
touching the core. The core itself will not split further without surgery.

The technique that found this was **field-coverage partitioning** (sort methods
by how much self-state they touch; the ones at the floor fall out). It is NOT
LCOM-clustering — clustering blobbed at every threshold. Documented below
because the negative results are as instructive as the positive one.

## The techniques tried, in order

Each subsection names the literature technique, the dl that computes it, and
what it revealed on Engine. All dl is reproducible from the `scip_*` relations.

### 1. Feature envy (Marinescu / Fowler)

A method that calls more features on another type than on its own belongs on
that other type. Computed via `scip_callee_type` (the `impl#[Type]` segment of
each method moniker): for each fn, group callees by receiver type; flag (fn,
type) pairs where the fn is not itself on that type.

**Result: noisy.** Top hits were `watcher_loop -> Daemon(7)`, `run_daemon ->
Engine(6)`, `tick -> Rule(4)`. But most were false positives — orchestration of
hubs, not misplaced methods. Feature envy conflates four regimes and cannot
separate them alone (see #2-#3 for the filters that do).

### 2. Type coupling scorecard — Ca / Ce / Instability (Chidamber-Kemerer / Robert Martin)

- **Ca** (afferent): distinct types that depend on T.
- **Ce** (efferent): distinct types T depends on.
- **Instability I = Ce / (Ca + Ce).** I -> 0 = stable (depended-on, reaches out
  little); I -> 1 = leaf (disposable). Good architectures depend toward
  stability.

```
rel tc_edge(a, b) <- scip_fn_edge(fa, fb), own_type(fa, a), scip_callee_type(fb, b), a != b.
rel ca(ty, count(a)) <- tc_edge(a, ty).
rel ce(ty, count(b)) <- tc_edge(ty, b).
```

**Result: Engine is the sole god-object.** Ca=13, Ce=13, **I=0.50** — maximally
central in BOTH directions. Every other type is a leaf (I ~ 0) or minor node.
A healthy stable core sits at I -> 0 (depended-on, reaches outward little);
Engine reaches outward exactly as much as it is reached into. This is the
quantitative god-object signature, not a judgment call.

### 3. Type reciprocity — real cycle detection

For each type pair (A, B): does A call B AND B call A? Bidirectional coupling is
a hard refactor blocker; one-way is severable behind an interface.

```
rel tc_cycle(a, b) <- tc_edge(a, b), tc_edge(b, a), a < b.
```

**Result: 0 reciprocal type pairs.** The method-level type graph is a DAG. So
every feature-envy hit is one-way severable, none is a true mutual cycle. The
file-level daemon<->engine cycle (8 mutual edges) is sustained by free-fn and
type references, NOT by method-to-method calls — it is breakable by relocation
or one interface extraction, not by restructuring the type hierarchy.

### 4. Collaborator-based LCOM4 — threshold sweep

Two methods are "connected" if they share a foreign collaborator type. Connected
components = proposed clusters.

```
rel eng_collab(fn, ty) <- scip_fn_edge(fn, fb), eng_method(fn), scip_callee_type(fb, ty), ty != "Engine".
rel shares(a, b) <- eng_collab(a, ty), eng_collab(b, ty), a != b.
rel cl(rep, m) <- scc(shares).
```

**Result: blob at k=1, shatter at k>=2.** Threshold "shares >= 1 collaborator"
yields ONE component of 75 methods. Requiring >= 2 shared collaborators
**shatters into 79 singletons** — the blob was held together by chains of
single-shared-collaborators (A shares Db with B, B shares Rule with C), not by
dense sub-communities. Only two real multi-method clusters survived at k>=2:
`eval_scc_rule` (4 methods) and `insert_spine_files` (3). Real but tiny.

**Conclusion: Engine has no collaborator-based sub-structure.** Each method has
its own collaborator footprint; they do not group.

### 5. Field-based LCOM4

Same as #4 but the edge is "two methods reference the same Engine FIELD."
Computable in pure dl because RA encodes field monikers as `engine/Engine#<name>`
(separator `#`, no `impl#[`, no `()`), so fields are isolable by a positive
regex filter on `scip_fn_edge` callees:

```
rel eng_field_ref(fn, field) <- scip_fn_edge(fn, field), eng_method(fn), field =~ /Engine#/.
```

**Result: also a blob.** One component of 92 methods. Reason: `Engine#db` is
touched by **71 of ~75 methods** — the DB connection is universal shared state
and single-handedly connects everything.

### 6. Field-coverage partitioning (peel the hub field) — THE WINNER

The standard god-object move: remove the universal field and see what is left.
Mark `db`, cluster on the non-db fields.

```
rel is_db(field) <- eng_field_ref(fn, field), field =~ /Engine#db/.
rel non_db(fn, field) <- eng_field_ref(fn, field), !is_db(field).
```

**Result: the seam was in the complement, not the cluster.** Even after peeling
`db`, the 48 methods that touch non-db fields stay connected (via `rels`/`root`).
But **only 48 methods touch any non-db field** — the other 44 touch ONLY `db`.
Those 44 are the data-access surface: `count_rows`, `column_exists`,
`insert_spine_files`, `insert_spine_strings`, `insert_source_rows_for_paths`,
`load_edges`, `load_file_meta`, `load_rel_digest`, `query_sql`,
`rebuild_legacy_*_rels`, `refresh_module_rels_*`, `ensure_meta`,
`flush_node_spine`, `observe_ref`. Every one takes `self` only to deref
`self.db`.

```
rel db_only(fn) <- touches_db(fn), !has_nondb(fn).
```

A further 18 methods touch neither `db` nor any field — pure functions
masquerading as methods (could be free fns).

## What did NOT work and why

- **LCOM-clustering (both collaborator and field)** assumes sub-communities
  exist. Engine has none; a few hub fields (`db`, `rels`, `root`) each touch
  dozens of methods and transitively connect everything. Raising the similarity
  threshold shatters to singletons rather than revealing groups. Clustering is
  the wrong frame for a session object.
- **Feature envy alone** conflates orchestration-of-a-hub (fine), co-located
  free fns (cosmetic), one-way dependency (severable), and true mutual coupling
  (a smell). Without the Ca/Ce and reciprocity filters it is mostly noise. The
  Engine envy rows were ~90% orchestration of the Engine/Db hubs (fan-in 110 and
  80 respectively).
- **Betweenness / PageRank** would need all-pairs shortest paths — out of dl's
  reach without a new builtin. Degree centrality (fan-in/out) is the available
  proxy and is what #2 uses.

## The technique that worked

**Field-coverage partitioning.** Not clustering (which asks "do methods form
groups"), but coverage (which asks "how much state does each method touch").
Sort methods by self-state width; the floor set (touches only the universal
dependency) is the free surface that extracts without cohesion loss. The method
names confirmed the data-access character without manual inspection.

This generalizes: any god-object with a universal dependency field (`db`,
`config`, `logger`) will show the same pattern — peel it, and the methods that
touch nothing else are the extractable layer. The cluster count stays 1; the
*partition* by coverage is the signal.

## Validation: cold-read decomposition vs field-coverage

A separate decomposition was produced cold: an agent read `engine.rs` source
only, with no access to this note or the dl analyses, and proposed a 7-cluster
split (Setup, RepoSync, SourceReconcile, Indexers, Eval, Digest, LspQueries)
plus a 2-method leftover. All 110 methods covered, each in exactly one cluster.
Both decompositions were then scored on the same per-cluster metrics.

### Per-cluster scores

| Decomposition | Cluster | N | F_total | F_shared (>=2 methods) | F_ext (outside) | F_unique = total - ext |
|---|---|---:|---:|---:|---:|---:|
| dl | db_only | 44 | 1 | 1 | 1 | 0 |
| dl | stateful | 48 | 16 | 16 | 1 | **15** |
| dl | pure_fn | 18 | 0 | - | - | - |
| cold-read | setup | 8 | 16 | 5 | 16 | 0 |
| cold-read | reposync | 10 | 6 | 4 | 6 | 0 |
| cold-read | reconcile | 10 | 5 | 1 | 5 | 0 |
| cold-read | indexers | 31 | 4 | 3 | 4 | 0 |
| cold-read | eval | 27 | 13 | 9 | 13 | 0 |
| cold-read | digest | 6 | 1 | 1 | 1 | 0 |
| cold-read | lsp | 16 | 3 | 3 | 3 | 0 |
| cold-read | leftover | 2 | 2 | 1 | 2 | 0 |

`F_unique` = fields touched only by methods in this cluster. **Every cold-read
cluster owns 0 unique fields.** Only dl/stateful isolates any state (15 fields:
`rels`, `root`, `repos`, `closure_cache`, etc.). If extracted, only dl/stateful
would carry exclusive ownership of any field; every cold-read cluster would
share all its state with the rest of Engine.

### Cross-tab (cold-read cluster x dl bucket, method counts)

| cold-read \ dl | db_only | stateful | pure_fn | total |
|---|---:|---:|---:|---:|
| setup | 0 | 8 | 0 | 8 |
| reposync | 2 | 5 | 3 | 10 |
| reconcile | 7 | 1 | 2 | 10 |
| indexers | 13 | 13 | 5 | 31 |
| eval | 7 | 15 | 5 | 27 |
| digest | 3 | 0 | 3 | 6 |
| lsp | 11 | 5 | 0 | 16 |
| leftover | 1 | 1 | 0 | 2 |
| **total** | **44** | **48** | **18** | **110** |

Only one cold-read cluster (Setup, 8 methods) is bucket-pure. The other 7
straddle the field seam. Indexers (31 methods, labelled "most cohesive unit"
in the cold-read rationale) splits 13/13/5 across all three buckets. Digest
contains 3 pure fns the cold read grouped as bookkeeping.

### Verdict

The cold-read decomposition does not separate on field access. Do not ratchet
dl toward reproducing it: every cold-read cluster has F_ext = F_total, so
reproducing them would teach dl to ignore the only signal that produces
extractable units.

The two decompositions compose because they answer different questions:

- **dl = floor (what can move).** db_only (44) and pure_fn (18) are guaranteed
  extractions: extract `Db`; move the 18 pure fns to a helpers module. The
  48-method stateful core stays on Engine.
- **cold-read = labels (what to name).** The 44 db_only methods break down by
  cold-read cluster as lsp(11), indexers(13), reconcile(7), eval(7), digest(3),
  reposync(2), leftover(1). Use these to group methods on the new Db type.
- **The 48 stateful methods do not subdivide on field access.** Only one
  cold-read cluster sits fully inside stateful (Setup, 8). The other ~40
  stateful methods can only be sub-grouped by naming or call topology, and any
  such split produces zero field isolation.

Field-based techniques will all collapse to "stateful = one block" because
that is what the data says. Name clustering, topological layering, or
co-change are the only signals that could further cut the core.

## N-rater card sort: psychometric validation of the cold read

The N=1 cold read in the previous section could be anecdotal: maybe another
reader produces a totally different 7-cluster split. Settled by repeating the
experiment with N=6 independent cold-readers (5 new subagents + the original)
under the same prompt. This section teaches the methodology, then reports the
numbers.

### The question

Do independent readers, given only the source of `engine.rs`, converge on the
same decomposition? If yes, the cold-read output reflects a shared mental model
of the code rather than one reader's idiosyncratic take. If no, "decomposition"
is in the eye of the beholder and no automated technique can be benchmarked
against it.

This is a standard psychometric question: **inter-rater reliability.** The
methodology is borrowed from UX card-sort studies and clinical-coding agreement
studies. Adapted here by replacing human raters with cold-read LLM subagents
(see Caveats for why this matters).

### The metric: Adjusted Rand Index (ARI)

A partition is an assignment of 110 methods to K clusters. Comparing two
partitions is not as simple as counting matches because cluster labels are
arbitrary: rater A's "Cluster 1" may be rater B's "Cluster 3" but represent
the same group.

ARI sidesteps labels by working on method pairs:

1. There are `C(110, 2) = 5995` unordered pairs of methods.
2. For each pair, each partition either puts them in the same cluster or in
   different clusters.
3. A pair is "agreed on" if both partitions make the same call (both together
   or both apart).

Raw agreement (agreed / 5995) is inflated because two random partitions will
agree most of the time on "apart" (most pairs are in different clusters in any
sparse partition). ARI corrects for chance:

```
ARI = (observed_agreement - expected_agreement_by_chance)
      ----------------------------------------------------
      (max_possible_agreement - expected_agreement_by_chance)
```

Interpretation:

| ARI | Meaning |
|---|---|
| 1.00 | partitions identical (modulo label permutation) |
| 0.00 | no better than random label assignment |
| < 0 | worse than chance (rare) |
| > 0.6 | strong agreement |
| 0.4 - 0.6 | moderate |
| < 0.2 | weak |

Reference: Hubert & Arabie (1985), "Comparing partitions," *Journal of
Classification* 2: 193-218.

### The experimental design

- **Stimulus:** `engine.rs` (5928 lines), no other files.
- **Raters:** 6 cold-read subagents (5 `explore`, 1 `general` from the prior
  session). Each got the same prompt: "decompose `impl Engine` into 3-7
  clusters; cover all 110 methods; each method in exactly one cluster."
- **Blinding:** raters were instructed not to read `v5/research/`,
  `chat_log/`, `.agents/`, or `skills/`, and not to run `dl`. The dl analyses
  and this note were hidden.
- **Output check:** every rater covered all 110 methods, no duplicates, no
  missing, no extra. Cluster counts: r1=7, r2=7, r3=8, r4=7, r5=7, r0=8.
- **Scoring:** pairwise ARI across all `C(6,2)=15` rater pairs. Mean and
  spread describe the typical level of agreement.

### Results: inter-rater agreement

Mean pairwise ARI **0.650**, std 0.100, range 0.503-0.877.

| | r1 | r2 | r3 | r4 | r5 | r0 |
|---|---:|---:|---:|---:|---:|---:|
| r1 | 1.000 | 0.684 | 0.507 | 0.877 | 0.706 | 0.714 |
| r2 | 0.684 | 1.000 | 0.503 | 0.713 | 0.610 | 0.637 |
| r3 | 0.507 | 0.503 | 1.000 | 0.550 | 0.640 | 0.528 |
| r4 | 0.877 | 0.713 | 0.550 | 1.000 | 0.734 | 0.715 |
| r5 | 0.706 | 0.610 | 0.640 | 0.734 | 1.000 | 0.638 |
| r0 | 0.714 | 0.637 | 0.528 | 0.715 | 0.638 | 1.000 |

By the >0.6 = strong convention, the 6 raters agree strongly on a shared
decomposition. r3 is the outlier (lowest row/col means, ~0.55), the others sit
at ~0.70. The original cold read (r0, the `general` agent) is not
distinguishable from the explore agents (mean 0.65 either way), so model
"strength" did not affect the output for this task.

### Pair-agreement distribution

For each of the 5995 method pairs, count how many of the 6 raters put them in
the same cluster (0 through 6):

| Raters agreeing | # pairs |
|---:|---:|
| 6/6 (unanimous together) | 572 |
| 5/6 | 179 |
| 4/6 | 185 |
| 3/6 | 99 |
| 2/6 | 349 |
| 1/6 | 717 |
| 0/6 (always apart) | 3894 |

4466 of 5995 pairs (75%) get a unanimous either-together-or-apart vote. The
remaining 25% are the contested boundary cases.

### Consensus clusters (unanimous pairs only)

Take the 572 pairs that all 6 raters co-clustered. Build a graph where methods
are nodes and unanimous pairs are edges. Connected components of this graph are
groups where every member pair is unanimous: the rock-solid clusters.

18 components emerge. The 10 with size >= 4:

| Size | Consensus cluster | Members |
|---:|---|---|
| 29 | **Indexers** | refresh_* (builtin/spine/module/type/call/dataflow/doc/scip/changed/node/daemon), rebuild_legacy_* (module/type/call), node_*/flush_node_spine, scip_name_defs, collect_manifests, hunk_new_range, module_files_by_rev, module_rows_for_rev, insert_module_rows, insert_module_spans |
| 10 | **Datalog eval** | rebuild_derived, rebuild_closures, eval_closure_seed_rule, eval_scc_rule, run_query, query_one_sql, refresh_cond_cache, run_reaches_point, any_closure_empty, load_edges |
| 10 | **LSP read API** | count_rows, query_sql, rel_rows, repo_relation, definition_targets, hover, diags, module_imports, same_package_uses, source_paths |
| 6 | **Repo/git** | resolve_rev, resolve_repo, resolve_scan_repos, resolve_scan_bindings, ensure_cloned, ensure_cloned_or_missing |
| 6 | **Digest** | rel_digest, load_rel_digest, save_rel_digest, source_rule_digests, prune_unchanged_by_digest, seed_rel_digests |
| 5 | **Source reconcile** | load_file_meta, save_file_meta, reconcile_sources, retract_path, retract_paths |
| 5 | **Setup/flags** | new, set_query_json, set_prime_tick, set_root_implicit, set_repos |
| 4 | **Gen sink** | run_gen, run_gens, apply_splices, apply_cursors |
| 4 | **Schema declaration** | declare, declare_all, declare_builtins, create_auto_indexes |
| 4 | **Span queries** | located_spans, span_at, string_spans, work_file_id |

The remaining 8 components are size 2-3 pairs (tick+tick_paths,
parse_github_org+run_repo_pulls, etc.). 25 methods appear in no unanimous
component (singletons under the agreement graph); these are the boundary cases
where raters disagreed on placement.

### Orthogonality check: dl field-coverage vs every rater

Add the dl field-coverage partition (db_only=44 / stateful=48 / pure_fn=18)
as a 7th "rater" and compute ARI against each of the 6 humans:

| | r1 | r2 | r3 | r4 | r5 | r0 |
|---|---:|---:|---:|---:|---:|---:|
| r_dl_fieldcoverage | 0.051 | 0.032 | 0.023 | 0.033 | 0.044 | 0.032 |

Essentially zero. The structural partition and every human rater's semantic
partition are uncorrelated. This is empirical confirmation of what the
cross-tab in the prior section hinted at: the cold-read clusters do not
separate on field access, because field access and perceived concern are
different axes.

### What this experiment does and does not establish

**Does:**
- Validates the cold-read methodology as a measurement tool. Mean ARI 0.65 is
  well above the strong-agreement threshold; the cold read is reproducible.
- Produces a "subjective ground truth" for Engine: the 10 unanimous consensus
  clusters above. Any automated decomposition technique can now be benchmarked
  against this ground truth by computing its ARI vs the 6 raters.
- Empirically confirms that structural and semantic decomposition are
  orthogonal axes (dl ARI ~0.04 vs every human rater).

**Does not:**
- Establish that the consensus clusters are *correct*. They are agreed-upon,
  not optimal. Six raters could converge on the wrong answer.
- Establish that human raters would behave the same way. LLM subagents are a
  narrow population; they may share biases that real Rust developers do not.
  Treating ARI 0.65 as a population estimate requires running the same
  experiment with N>=6 human Rust developers.
- Pick a single best decomposition. It gives a distribution over partitions;
  picking one requires an objective function (see
  `2026-06-26-cross-domain-decomposition-techniques.md`).

### Caveats

1. **LLM raters, not human raters.** Generalization to human developer
   populations is unverified. The methodology is sound; the population is
   narrow.
2. **No time constraint.** Raters could page through engine.rs as much as
   they wanted. A 10-minute time limit (more like a real maintenance task)
   might compress agreement.
3. **Single codebase.** Engine may have unusually strong naming conventions
   (`refresh_*`, `insert_*`, `rebuild_legacy_*`) that anchor raters
   artificially. Replicating on a codebase with weaker naming would test
   whether the high ARI is signal or naming-driven.
4. **Rater independence.** All 6 raters are instances of the same model
   family. They may share systematic biases. Independent model families would
   be a stronger test.

### Reproducibility

Scoring script and rater data at `/tmp/dl-score/agreement.py`. Pure-Python
ARI implementation (no sklearn/numpy dependency). Re-running the 5 cold-read
subagents requires the prompt template used above; results will vary run-to-
run because subagent sampling is stochastic, but the mean ARI should land in
the 0.5-0.7 range based on this run.

## Reproducibility

All queries above run against the existing `scip_fn_edge` + `scip_callee_type`
relations (populated by `v5/src/scip_import.rs` from the RA index). No field-
access Rust extraction was needed — field references are already in
`scip_fn_edge` because that relation emits every referenced symbol, and RA's
`Engine#<field>` moniker shape makes them isolable by regex in dl.

The limiting factor for deeper analysis (true community detection / modularity
optimization / betweenness) is matrix math, which dl does not have. For those,
export the type-edge or method-edge relation and run an external tool
(NetworkX, igraph). The dl layer's job is to produce the graph; the scoring can
happen elsewhere.
