# Prior art: tree-sitter code graphs and oracle-benched accuracy (2026-08-30)

Two-agent research sweep (GitHub census ~74 fetches, discourse/literature ~180
fetches, star counts verified via api.github.com 2026-08-30). Question: is
sprefa-extract a road already traveled?

## TOC
1. Verdict
2. repowise, the one direct competitor
3. Method corrections their protocol forces on our bench
4. Census verdict table (condensed)
5. Every published accuracy number found
6. Adopt / bench / read lists
7. Caveats and dead ends

## 1. Verdict

| layer | state |
|---|---|
| tree-sitter extractor building a code graph | crowded: graphify 112,704 stars in 5 months, aider 48k, dozens of MCP servers |
| syntax tier + opt-in compiler tier architecture | 2 tools (srctx dead 2024, blarify unmeasured), Graft is the closest live twin |
| measured edge recall/precision vs compiler oracles | ONE competitor: repowise (go+ts). Sourcegraph wrote the harness (scip-syntax evaluate.rs, 864 lines) and never published a number |
| rust call-graph accuracy number | NOBODY but us. repowise withdrew ("no sound call-graph tool"). Our 93.68/55.98 vs ra is the only measurement found |

Star-count caveat: graphify 112,705 stars / 376 watchers, codebase-memory-mcp
41,333 / 166, code-review-graph 31,020 / 100 — ratios far off organic norms
(repowise 6,270 / 28; stack-graphs 875 / 171 forks). Big stars are unreliable.

The field sells token reduction and measures token reduction. Nobody in the
AI/agent wave measures edges. GitHub's stack-graphs (the one technically
correct middle tier) was archived 2025-09-09, 4 languages, no Go, no Rust,
no numbers ever; GitHub docs now describe tree-sitter name search only.

## 2. repowise (repowise-dev/repowise, 6,270 stars, AGPL, pushed 2026-08-31)

Same architecture: tree-sitter parse, import resolution, 29 named resolution
origins with confidences (same_file 0.95 ... global_unique 0.50). G4 bench:
37,853 oracle edges, oracles go/callgraph/rta + tsc, precision/recall:

| cell | repowise | codebase-memory-mcp | graphify |
|---|---|---|---|
| gitleaks (no tests) | .976 / .955 | .934 / .967 | .997 / .886 |
| syft (no tests) | .943 / .513 | .635 / .542 | .771 / .447 |
| zod (no tests) | .992 / .703 | .987 / .694 | .825 / .248 |
| hono (no tests) | .977 / .731 | .949 / .686 | .980 / .688 |

repowise-bench repo has NO LICENSE: protocol copyable, code not.

## 3. Method corrections for our bench (each is a design fork for Chris)

| their protocol | ours today | consequence |
|---|---|---|
| join key = (caller_decl_file, caller_decl_line) -> (callee_decl_file, callee_decl_line); "no name is ever compared" | 4-col name-keyed tsv | name-keyed joins favor tools that spell identifiers like the oracle; our numbers not directly comparable until reconciled |
| caller = OUTERMOST enclosing function, not closure literal | enclosing (check which) | they report this rule alone moved every arm >20 pts on TypeScript |
| 3 buckets matched/contradicted/unjudged; precision = matched/(matched+contradicted) | 2 buckets | we may charge as false positives edges the oracle cannot judge (0.4-11.1% of output on their Go cells) |
| no F1, deliberately | we do not report F1 either | keep |
| resolution-origin column per edge | absent | a precision regression becomes traceable to one resolver class |
| fuzzy (Jaccard) symbol mapping, fractional TP/FP | exact string match | rust 55.98% precision is the number most likely to move under fuzzy mapping |
| state oracle + scope with every number | RATCHET.tsv names oracle; scope implicit | PyCG 99.2% precision collapses to 0.19 when scope includes deps (Jarvis); ISSTA 2024: compiler-tier Java CGs score 0.7-21.6% edge precision vs RUNTIME. Every number names its oracle and scope or it is meaningless |
| VTA seed must be stated | check ours | vta soundness is conditional on the initial graph; nil = VTA-over-CHA |

## 4. Census condensed (full tables in the agent transcripts)

MEASURE-AGAINST: repowise (AGPL, bench-only), codebase-memory-mcp (MIT, C,
"Hybrid LSP" = native checker tier for 12 langs incl rust, Go recall leader in
4/5 repowise cells), joern (only tool with go+ts+py+rust call-edge frontends
in one binary; natural 4th oracle), Graft (--lsp twin), tree-sitter-analyzer
(miswire-audit runnable FP probe), serena (LSP), CodeGraphContext, crabviz.

ADOPT-CANDIDATE (pieces): scip-syntax evaluate.rs scoring (Apache-2.0, ~300
lines to port), PyCG micro-suite as python oracle (Apache-2.0, 119 cases,
frozen), dependency-cruiser as ts module oracle (MIT, real node/tsconfig
resolution), cargo-modules as ra_ap pinning reference (MPL-2.0, active).

BELOW-OUR-TIER: graphify (label.lower() matching, 93% of edges tagged
INFERRED), aider repomap (file-granular name match, no scope), ast-grep (own
FAQ disclaims scope/types), pyan, depends, madge, doxygen, ctags lineage.

DIFFERENT-PROBLEM: Glean (fact store downstream of us), semgrep OSS
(single-file), kythe (call edge exists but requires hooking the build),
cocoindex/llama-index (chunkers), potpie, sem, sourcebot.

DEAD: stack-graphs (archived 2025-09), github/semantic (archived),
sourcetrail upstream (archived 2021, live forks petermost/OpenSource…),
lsif-go (archived), srctx (2024). SCIP moved to neutral scip-code org
2026-01; scip.proto has NO call edge (SymbolRole lines 524-547) — ingest
format only.

## 5. Published accuracy numbers, the whole field

| tool/paper | lang | oracle | recall/precision % | year |
|---|---|---|---|---|
| repowise G4 | go/ts | rta / tsc | see section 2 | 2026 |
| PyCG (ICSE) | python | hand-built | 69.9 / 99.2 | 2021 |
| Pyan | python | same | 20.6 / 74.4 | 2021 |
| Depends | python | same | 14.4 / 98.7 (sound 0/112) | 2021 |
| Jarvis | python | 135-case, dep scope | PyCG precision -> 0.19 | 2024 |
| ACG Feldthaus | js | dynamic | >=85 / >=66 | 2013 |
| Jam | js/node | dynamic | 98.6 / 84.4 | 2021 |
| WALA ACG web | js | dynamic | >80 / <5 | 2022 |
| Java static CGs | java | runtime (ISSTA) | median 88.4 recall; CHA 93.1 / 0.9 | 2020-24 |
| ours | go | codeql | 98.96 / 90.78 | 2026 |
| ours | rust | rust-analyzer | 93.68 / 55.98 | 2026 |

Everything else (stack-graphs, Glean, Kythe, CodeQL-as-extractor, Sourcegraph,
go/callgraph docs, rust-analyzer): no numbers, ever.

## 6. Reads for Chris

1. Total Recall? (Helm et al., ISSTA 2024) — what oracle parity even means.
   opal-project.de/articles/TotalRecall@ISSTA24.pdf
2. Code Isn't Memory (arxiv 2606.22417) — best downstream result, tree-sitter
   tier, +40.2 pp localization; gain over agentic grep p=0.087.
3. Codebase-Memory (arxiv 2603.27277) — the only published resolution cascade
   with confidences; names its holes (macros 0.58, no dynamic dispatch).

## 7. Notable stray facts

- blarify claims SCIP == LSP accuracy at ~330x speed (claimed, unverified);
  if true on our corpus, the scip tier and the checker tier may collapse.
- The syntax-vs-checker ablation (same corpus/oracle, tier the only variable)
  does not exist in the literature; PR #598's table is that experiment.
- Search-snippet hallucinations caught during research: 2 (fake 47.4k-star
  claim for CodeGraph, fake "37,000 oracle edges" benchmark).
- Reddit was fetch-blocked (HTTP 400); zero reddit evidence here.

## 8. Papers deep-dive addendum (third research lane)

Industrial line (GitHub stack-graphs paper, Sourcegraph SCIP docs, Meta Glean
blog) publishes scale and latency, never correctness — each source fetched and
grepped; the stack-graphs EVCS 2023 paper has NO evaluation section. Every
precision/recall figure in the field is academic.

Literature gaps stated plainly:
- NO published Go call-graph accuracy measurement exists (go/callgraph docs
  give an ordering only).
- NO TypeScript-specific accuracy study exists (all JS-only).
- Rust: RUPTA (CC 2024) reports only relative numbers (+29% edges vs Ruscg),
  no oracle P/R.
So our go, ts AND rust rows are each firsts of their oracle shape.

Oracle-kind calibration (why our numbers cannot be compared across columns):
| oracle kind | precision reads | recall reads | examples |
|---|---|---|---|
| dynamic traces | crushed (WALA 0-CFA 23.8%, CHA 0.7%) | high 0.88-0.95 | ISSTA 2024, ICSE 2020/2022 |
| hand-built micro | both meaningful, tiny n | PyCG 103/112 sound | PyCG/JCG/JARVIS |
| another static tool (OUR shape) | measures agreement | same | nobody publishes this at scale but repowise |

Reusable oracle suites worth importing:
| suite | lang | oracle | license |
|---|---|---|---|
| PyCG micro-benchmark | python | callgraph.json per snippet, 119 cases | Apache-2.0 |
| JCG/CATS | java+js+py | expected/forbidden targets, adapters incl. PyCG/Jelly/TAJS | NO LICENSE |
| DyPyBench | python | dynamic traces, 50 projects | MIT |
| NJR-1 | java | dynamic traces, 293 programs | Zenodo |

Key context row: PyCG captures only 49% of real dynamic edges on 39 projects
(DyPyBench); JARVIS whole-program precision 0.35. Our rust 55.98% and ts
71.15% precision sit above every published real-world whole-program static-CG
precision found, with the oracle-mismatch caveat attached.

## 9. Four-pass deep hunt (2026-08-31, user-ordered second sweep)

### Pass 2 (registries + paper artifacts) NEW-FINDS
- Jelly `--compare-callgraphs` (npm @cs-au-dk/jelly): prints per-call
  precision/recall between two CG JSONs, dynamic oracle via NodeProf;
  verified at src/output/compare.ts:236. JS/TS. The one shipped measuring
  TOOL outside repowise.
- SWARM-CG (github.com/secure-software-engineering/SWARM-CG, 11 stars):
  cross-language micro-benchmark, java+python+js, per-edge annotations,
  metrics.py computes per-callsite precision/recall. SWARM-JS half: 126
  snippets, 596 edges. Paper arXiv 2410.00603: PyCG 84.9/87.3
  complete/sound; TAJS 94.4 complete / 11.1 sound; best LLM 60.3/62.6 py.
  (percentages from HTML extraction, verify vs PDF before external quoting)
- NYXCorpus (arXiv 2402.07294): java dynamic-trace oracle; ML pruning +25%
  precision at -9% recall.
- Dead veins: PyPI unscrapeable (anti-bot), Zenodo search useless,
  paperswithcode defunct, sourcegraph/codeintel-qa 404s (their internal
  accuracy harness is gone from the public web).

### Pass 4 (adversarial raid) verdicts
- Vendors (Cursor/Cody/Semgrep/...): downstream QA or alert-reduction only.
  Cursor Context Bench +12.5% answer accuracy, private. EMPTY for edges.
- Chinese ecosystem (CodeFuse-CGM NeurIPS 2025, RepoFuse, MarsCode): graphs
  assumed correct, only SWE-bench-style scores. Ablations never grade the
  graph. EMPTY.
- IDE lineage (JetBrains resolve tests): per-fixture unit API, no corpus, no
  published rate. EMPTY.
- Compiler-adjacent FOUND: rust-analyzer `analysis-stats` + the
  rust-analyzer/metrics repo = daily unresolved-type/type-mismatch counts on
  a pinned 5-crate corpus (self 133 unknown-type exprs, ripgrep 0,
  webrender-2022 65; 2026-08-30 record). Grades TYPES not edges,
  self-referential, but the only continuously published resolve-quality
  series in production tooling.
- scip lint = dangling-edge boolean checks; scip stats = counts; no
  index-vs-index comparator exists anywhere in scip tooling.
- SCA reachability vendors (Endor, Coana): claimed alert-reduction ratios
  (80-99%), no oracle ever shipped. EMPTY of measured numbers.

### Pass 1 (GitHub crawl: 1,035 repos touched, 795 READMEs grepped, 38 drilled)
Verdict revision: FIVE repos publish edge precision/recall vs an independent
oracle (the first sweep found only repowise):
- Eilodon/CALM (15 stars): tree-sitter multi-lang, oracle = rust-analyzer
  scip decoded to decl coordinates. Self-repo: precision 0.795, recall 0.193
  (1,568/8,117 oracle edges). Precision stratified BY CONFIDENCE TIER:
  inferred 0.967, resolved 0.935, textual 0.514. Second harness: 12 fixtures
  with decoys, false-confidence-rate metric.
- vitali87/code-graph-rag (4,858 stars): 14+ langs, per-language native AST
  oracles (python ast, go_ast.go, Oracle.java...) + sys.settrace execution
  trace for recall. Committed CSVs (direct-call TP 4,434 at P/R/F1 1.0 —
  oracle is its own parse family, so partially self-referential). Retrieval
  on django: graph F1 0.957 vs grep 0.789.
- ktrianta/rust-callgraph-benchmark (9 stars): 6 rust call-kind packages
  with declared edges; LLVM opt resolves static 100%, dyn dispatch 0%,
  fn pointer 0%.
- bartolli/codanna (731 stars): rust indexer with per-edge verdict
  classifier; three.js 468/1,731 added edges class-wrong.
- mengshi02/codetrip (0 stars): java/kotlin/go/ts, human-reviewed facts,
  impact edges 100% precision / 89.9% recall, small n.
Harness-without-numbers: lvyahui8/CallGraphBench (13 languages x 3 tiers,
39 manifest cases, CI cross-checks vs runtime tracing — closest protocol
twin), SWARM-CG, linghuiluo/CGBench (spring), llmpa/callgraph-benchmark,
thusloop/InferCG. Negative-edge fixtures idea: nwyin/pycg-rs asserts
present:false edges.
Still true after 1,035 repos: nobody benches at OUR shape (multi-language,
corpus-scale, compiler oracle, committed ratchet floors) except repowise;
rust corpus-scale accuracy remains ours alone (CALM's rust row is
self-repo-scale, recall 0.193).

### Pass 3 (discourse trawl: HN Algolia full-text, GitHub issue trackers, reddit via curl lane)
Public discussion is vibes; the measurements hide in issue trackers of small
tools. NEW-FINDS (single-fetch verified, several look agent-maintained, star
counts mostly unchecked):
- optave/ops-codegraph-tool: the only 3-way head-to-head found anywhere —
  their tool vs Jelly 0.13.0 vs ACG on a 54-edge hand oracle tagged by mode.
  JS: P 100%/R 83% vs Jelly 94/94 vs ACG 92/67.
- cq27-dev/rag-rat: edge_oracle table of rust-analyzer verdicts over its own
  index; 225,083 edges examined, 4,248 contradicted, heuristic precision
  ~92.2%; precision-only by design.
- Ataraxy-Labs/sem: scores PyCG vs sem vs tree-sitter-stack-graphs on a
  19-edge category manifest — the only stack-graphs scoring found anywhere.
- vitali87/code-graph-rag PR #554: C calls vs libclang with abstention:
  P 0.98 / R 0.93 on jq; all 11 FPs inactive #ifdef branches.
- synaptixs/spine: fabricated-edge audit (shadowed names) across 5
  front-ends; cpp 0.47% invented bare-call edges; gate ratcheted to zero.
- ConflictHQ/navegador #196: measured that scope-aware resolution would move
  2 of 3,482 edges on their corpus, killed the planned work — measurement
  preventing code.
- reddit (curl lane, unverified): Graft author: naive name-matching tripled
  edge count and dropped precision 73% -> 37%; +rust-analyzer call hierarchy
  on anyhow: 253 -> 359 edges, orphan rate 47% -> 38%.
- Recurring shape across finds: oracle = a real toolchain (ra scip, go rta,
  tsc, libclang), and the shared discovered failure mode is the wrong-target
  edge that passes every dangling-edge/tier-distribution check.
HN full-text: 615 "call graph" comments, zero measured edge accuracy.

### Final verdict after 4 passes (revises section 1)
"Nobody measures" was too strong. Revised: a long tail (~a dozen small,
mostly 2026-era, partly agent-run repos) measures edge accuracy, usually
against a compiler-toolchain oracle, usually micro or single-repo scale,
usually precision-only or recall-only. Still standing after ~1,000+ repos,
4 passes: (a) multi-language corpus-scale compiler-oracle bench with
committed ratchet floors = us + repowise only; (b) rust at corpus scale =
us only (CALM's rust row is self-repo, recall 0.193); (c) the syntax-vs-
checker tier ablation = us only; (d) go accuracy vs codeql = us only.

### Pass 3 addendum (reddit lane final)
Graft numbers upgraded from unverified to verified (lane read the threads
live before reddit soft-blocked the IP): Go call-edge precision 73% -> 37%
when naive name-matching tripled edge count; rust +ra call hierarchy on
anyhow: 253 -> 359 edges, orphan rate 47% -> 38%. NOTE two distinct Grafts
exist: trailhq/Graft (census) and NanoNets/Graft (reddit author u/shhdwi).
14 threads remain unread (IP block); arbor + r/ClaudeAI codegraph posts are
the retry candidates.
