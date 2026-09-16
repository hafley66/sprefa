# Refactor reward: raw LOC vs structural metrics

Date: 2026-06-26. First measured result in the auto-refactor research arc.
Companion to `2026-06-26-prior-art-temporal-refactor-validation.md` and
`2026-06-26-variable-name-signal-extraction.md`.

## Question

To bootstrap a reward signal for an auto-refactor tool (manual Q-learning:
state → action → **measured reward**), is raw net-LOC a good enough reward, or
do structural de-duplication metrics beat it?

## Method

Each refactor commit in sprefa's own git history is one pass/fail experiment:

- **intent** = direction parsed from the commit subject
  - UP   = split / extract / break out   → expect net-LOC ↑
  - DOWN = collapse / consolidate / merge / dedup → expect net-LOC ↓
- **reward** = deterministic delta at parent→commit:
  - `loc_d` = net (insertions − deletions) over touched `.rs` files
  - `dup_d` = Δ redundant within-file fn-name copies
    (`sum(count-1)` for every fn-name defined ≥2× in a file)
- **pass/fail** = does reward sign match intent?

Harnesses: `refactor-reward/loc_intent_harness.py` (v1, LOC-only, full set)
and `refactor-reward/loc_vs_structural_harness.py` (v2, hard-collapse subset).

## Result 1 — LOC alone, full set (53 experiments)

| class | LOC-Δ matches intent |
|---|---|
| UP   (split/extract → LOC↑)    | **22/23 = 96%** |
| DOWN (consolidate  → LOC↓)     | **7/30  = 23%** |

Raw LOC is an excellent reward for **splits** and a terrible one for
**consolidations.** Genuine collapses routinely *grow* code short-term
(generics, trait plumbing, wider tables) while removing duplication; LOC
counts bytes, not duplication removed. (Some DOWN fails are classifier noise
— "intern" / "route" are feature-adds the broad regex mislabels; v2
restricts to hard-collapse verbs.)

## Result 2 — LOC vs structural, hard-collapse only (11 experiments)

| reward metric | pass-rate |
|---|---|
| raw net-LOC ↓                       | **4/11 = 36%** |
| structural (within-file fn-name dup-excess ↓) | **8/11 = 73%** |

Structural beats LOC **2:1** on the consolidation class — the class LOC is
worst at. Four real consolidations that grew boilerplate are correctly seen
as de-duplicating by the structural metric while LOC mislabels them.

## Result 3 — escalating the structural metric (v3/v4)

Each lexical structural signal alone lands at 73%; the win is in **combining**
them. v3 added a type-def count (`struct|enum|trait|impl`); v4 added
cross-type-method dup (method-names implemented under ≥2 impl-targets — a
text approximation of the `scip_callee_type` semantic signal).

| reward signal | pass (11 commits) | pass (genuine refactors, n=10) |
|---|---|---|
| LOC (net LOC↓)                | 36% | — |
| fn-dup (within-file)          | 73% | — |
| type-def count                | 73% | — |
| cross-type-method             | 73% | — |
| **COMBINED lexical** (any structural↓) | **82%** | **90%** |

The type-def metric caught the store-merge (`2bda022e`, type_def_d=-3) that
fn-dup missed. Cross-type-method alone did not crack the generic case.

**Lexical ceiling ≈ 90% on genuine consolidations.** The residue is one
genuine commit (`635de156`, "collapse node/edge mirrors into a *generic*
`GraphRef<K,I>`") where parameterization *added* type-level boilerplate
(type_def_d **+22**) while unifying structurally-similar code. That is a
semantic/AST-similarity reward, not a name or type-count reward — it
motivates the SCIP-per-rev bridge.

**Methodological caveat surfaced:** commit-level granularity bundles
unrelated changes. `635de156`'s cross-type-method Δ was **+2**, not because
the collapse failed, but because the same commit added unrelated impls. A
per-symbol or per-hunk reward would be cleaner than per-commit.

Harnesses: `refactor-reward/loc_vs_structural_v3_type_def.py` and
`..._v4_cross_type.py`.

## Result 4 — cross-repo generalization (3 codebases, 3 authors)

The reward harness is pure git + Python — it runs on any repo. Pointing the
broad-verb version (`generalize_cross_repo.py`, takes a repo root as argv[1])
at two independent famous Rust codebases tests whether the result is
author-specific or general.

| repo | author | LOC reward | combined structural (genuine) |
|---|---|---|---|
| sprefa  | (mine)   | 50% (6/12)  | 92% (11/12) |
| ripgrep | BurntSushi | 29% (2/7) | 100% (7/7) |
| serde   | dtolnay    | 64% (7/11)| 100% (11/11) |
| **total (3 repos)** | | **50% (15/30)** | **97% (29/30)** |

Across **3 codebases / 3 authors / 30 consolidation refactors**, combined
structural reward agrees with the refactor **97%** of the time; raw LOC only
**50%**. The thesis generalizes beyond one author's style. LOC's pass-rate
bounces 29%–64% by repo (author-dependent); structural stays 92%–100%
throughout.

Reproducibility:

```sh
git clone https://github.com/BurntSushi/ripgrep /tmp/ripgrep
git clone https://github.com/serde-rs/serde     /tmp/serde
python3 research/refactor-reward/generalize_cross_repo.py /tmp/ripgrep
python3 research/refactor-reward/generalize_cross_repo.py /tmp/serde
python3 research/refactor-reward/generalize_cross_repo.py   # sprefa default
```

Caveat: n per repo is small (commit-message labeling is sparse for the
consolidation class). The directional finding is robust; the magnitude needs
proper labels at scale (RefactoringMiner) to publish.

| commit | net-LOC | dup-Δ | LOC | STRUCT | subject |
|---|---:|---:|---|---|---|
| `135baa97` | +61  | 0  | FAIL | PASS | collapse 58 integration tests into one binary |
| `730819a9` |   0  | 0  | FAIL | PASS | rename DocLang → IngestLang |
| `78814658` | +58  | 0  | FAIL | PASS | collapse RtCtx build |
| `44518e22` | +483 | 0  | FAIL | PASS | perf + content-hash dedup + bulk flush |

## Where both metrics fail (3/11 — the open gap)

| commit | net-LOC | dup-Δ | subject |
|---|---:|---:|---|
| `635de156` | +148 | +6 | collapse v4 node/edge **mirrors into a generic** `GraphRef<K,I>` |
| `2bda022e` |  +60 | +6 | **merge** MutationStore + FactStore |
| `e8f4aa9b` | +2862 | +8 | merge-conflict resolution (not a real refactor — exclude) |

The first two are **cross-file / cross-type** consolidations: near-duplicate
impls in separate files collapsed into one generic, or two structs merged.
Within-file fn-name repetition cannot see that. The next structural metric
must operate across files/types (struct/impl counts, type-level duplication,
AST-similarity), not within a single file.

## Caveats

- **n = 11** for the structural comparison (sprefa's hard-collapse history is
  small). The 2:1 asymmetry is suggestive, not conclusive. Confidence grows
  by widening the corpus (RefactoringMiner on famous repos) — see next steps.
- One structural metric tested (within-file fn-name dup-excess). Others
  (cross-file, type-level, call-cohesion) untested.
- sprefa-only corpus, one author's style. Generalization untested.

## Conclusion

Structural de-duplication metrics are a meaningfully better reward signal than
raw LOC for **consolidation** refactors: combined lexical structural reaches
**97% agreement across 3 independent Rust codebases / 30 refactors** (sprefa,
ripgrep, serde) vs LOC's **50%**. LOC alone suffices for **split/extract**
(96%, sprefa). The two are complementary: LOC rewards splits, structural
rewards collapses. This validates the core premise that a deterministic
structural reward is worth building — and that raw change-size (the metric
every trend dashboard defaults to) systematically mislabels the highest-value
refactor class, across authors.

The lexical ceiling is ~90–100% depending on repo; the residual (parameteri-
zation-unification like collapse-into-a-generic) requires semantic/AST-
similarity, which is the SCIP-per-rev bridge's job. Commit-level granularity
also conflates concerns (per-symbol/per-hunk reward is the cleaner unit).
Next: RefactoringMiner for proper labels at scale (grow n), and the SCIP
per-rev bridge for the semantic residue.

## Reproducibility

```sh
python3 research/refactor-reward/loc_vs_structural_harness.py
python3 research/refactor-reward/loc_intent_harness.py
```

Both are pure git + Python (no dl, no SCIP index). Run from the repo root.

## Next steps (ranked by expected value)

1. **Cross-file / type-level structural metric.** Extend the reward to catch
   the 2 "both-fail" cases: struct/impl-count Δ, and cross-file repeated
   method-name **sets** across receiver types (the parallel-trait-impl shape
   from the dup-collapse recommender work). Target: structural pass-rate
   toward 90%+ on hard-collapse.
2. **Widen the corpus via RefactoringMiner.** Mine Extract-Class / Collapse-
   Hierarchy / Merge commits from famous Rust repos (serde, tokio, ripgrep)
   for labeled before/after pairs at scale. Replaces tiny sprefa-only n with
   hundreds. This is the bridge to a publishable result.
3. **SCIP per-rev bridge.** Snapshot `scip_*` at two revs so the reward can
   be semantic (cohesion, fan-out, call-graph density) rather than lexical.
4. **Turn the policy into a recommender.** With a reward that works, detect
   the reward-positive state-shape in current code and surface the next move.
