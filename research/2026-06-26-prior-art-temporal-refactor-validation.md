# Prior-Art Survey: sprefa — Datalog refactor recipes calibrated against git history

Date: 2026-06-26. Positions sprefa's three recipes (`missing-type.dl`,
`context-object.dl`, `param-fan-out.dl`) and the proposed **git-time-as-oracle**
calibration layer against the literature. Companion to
`2026-06-26-variable-name-signal-extraction.md` and
`2026-06-26-cross-domain-decomposition-techniques.md`.

## A. Software clustering / decomposition-quality objective functions (TurboMQ, Bunch)

**Closest work**
- **Bunch / Modularization Quality (MQ)** — B. Mitchell & S. Mancoridis, "On the Automatic Modularization of Software Systems Using the Bunch Tool," *IEEE TSE* 32(3):193–208, **2006**. https://www.cs.drexel.edu/~mancors/papers/Mancoridis-TSE-0035-0304.pdf — Defines **MQ** (intra-cluster cohesion vs inter-cluster coupling over a Module Dependency Graph) and optimizes partitions via hill-climbing/GA.
- **Original automatic-clustering paper (MQ origin)** — S. Mancoridis, B. Mitchell, Y. Chen, E. Gansner, "Using Automatic Clustering to Produce High-Level System Organizations of Source Code," *ICSM* **1998**. https://ieeexplore.ieee.org/document/693283
- **EAS / search-based modularization surveys** — Praditwong, Harman, Yao, "Software Module Clustering as a Multi-Objective Search Problem," *IEEE TSE* 37(2), **2011** (ECS/ECA — extends MQ to multi-objective); SBSE literature uses MQ as the go-to decomposition-quality oracle.

**Overlap with sprefa** — `context-object.dl`'s SCC-over-co-occurrence clustering is the same family (group entities that belong together) but operates on a **lexical co-occurrence graph of local-variable names across functions**, not a structural dependency graph, and uses connected-component/SCC rather than MQ optimization.

**Gap sprefa could claim** — Nobody validates that MQ rises after *known* refactor commits in a before/after fashion at scale; MQ is optimized at a single snapshot, and its longitudinal behavior across real refactors is unstudied. sprefa can make the direction-of-movement claim explicit because it keys on revs.

## B. Git-history "behavioral" analysis (Tornhill / CodeScene)

**Closest work**
- **CodeScene (product)** — CodeScene AB, **2015–present**, https://codescene.com/product/ — Commercial "behavioral code analysis": hotspot analysis (churn × complexity), CodeHealth, knowledge maps, temporal trend dashboards. Mines git history to prioritize refactor targets.
- **"Your Code as a Crime Scene" / "Software Design X-Rays"** — A. Tornhill, Pragmatic Bookshelf, **2015** / **2018**, https://pragprog.com/titles/atcrime/your-code-as-a-crime-scene/ — frequency of change × complexity to find hotspots, predict defects, target refactors.
- **CodeScene validates against:** defect/issue-tracker integration (Jira/GitHub Issues) and lead-time/PR-density ("Code Red" whitepaper, https://codescene.com/hubfs/web_docs/Business-impact-of-code-quality.pdf, **2022**). Its claim is *predictive validity* (hotspots → future defects), **not** "refactor commits move metric X up."

**Overlap with sprefa** — **Adjacent, and this is the single biggest novelty threat.** CodeScene already mines git history for refactor prioritization and has longitudinal trend tracking. sprefa's `scan(...)/match(...,rev,...)/module_edge_rev` text axis is conceptually a CodeScene-style behavioral layer.

**Gap sprefa could claim** — CodeScene never uses **known refactor commits as labeled before/after pairs** to *calibrate threshold values* of structural recipes; it correlates churn-complexity with defects, a different (downstream) oracle. CodeScene's signals are churn + complexity + team dynamics — it does **not** consume a SCIP-quality typed symbol index, nor does it do name-co-occurrence clustering or identifier-repetition-as-missing-type detection. CodeScene is rev-aware on **text/churn**; sprefa aims to be rev-aware on **structural SCIP facts** — that bridge is genuinely open.

## C. Refactoring mining as labeled ground truth

**Closest work**
- **RefactoringMiner** — N. Tsantalis, D. Mazinanian, S. Rostami et al.; **2.0**: Tsantalis, Ketkar, Dig, *IEEE TSE* **2020**, https://ieeexplore.ieee.org/document/9136878 (PDF: https://users.encs.concordia.ca/~nikolaos/publications/TSE_2020.pdf). Repo: https://github.com/tsantalis/RefactoringMiner — AST-diff-based detector for **106 refactoring types** (Java/Python/Kotlin/TS/JS), **99.9% precision / 98.2% recall**. The de-facto ground-truth miner. Now ships an MCP server.
- **RefDiff** — D. Silva et al. / "RefDiff 2.0" (Wei, Foucault, et al.), **MSR ~2020**, https://github.com/GumTreeDiff/RefDiff — GumTree-based AST differencing for refactoring detection; main non-RMiner alternative for cross-validation.
- **Refactorings-at-scale empirical datasets** — M. Tufano, F. Palomba, G. Bavota, M. Di Penta, R. Oliveto, A. De Lucia; Palomba et al., "Mining Version Histories for Detecting Code Smells," *IEEE TSE* **2015**. These supply the before/after commit corpora researchers treat as labels.

**How empirical-SE uses them** — Researchers run a smell detector on *rev-before*, then check whether the flagged entity disappears or changes at the mined-refactor *rev-after*; the mined refactor is the implicit label. Public oracles: https://github.com/ameyaKetkar/RMinerEvaluationTools, http://refactoring.encs.concordia.ca/oracle/.

**Overlap with sprefa** — **This is exactly sprefa's proposed methodology.** "Use RefactoringMiner-mined refactor commits as before/after labels, re-run the recipe at rev-before vs rev-after, measure metric movement" is the **standard empirical-SE evaluation protocol** — not novel as a method.

**Gap sprefa could claim** — (1) the **threshold-calibration framing** — using the signed magnitude of metric movement across many known-good refactors to *derive* recipe cutoffs, replacing N-rater human/model labeling; (2) doing it inside a **user-writable Datalog** engine rather than a one-off Java study. The calibration-as-oracle angle is the defensible slice; "mined refactors as labels" itself is DONE.

## D. Automated refactoring *suggestion* tools

**Closest work**
- **JDeodorant** — Tsantalis, Chatzigeorgiou, Fokaefs et al., Concordia/U. Macedonia, Eclipse plugin, https://github.com/tsantalis/JDeodorant — Detects Feature Envy, God Class, Type/State Checking, Long Method, Duplicated Code and suggests Move Method / Extract Class / Extract Method / Replace-Conditional-with-Polymorphism. God Class → **Extract Class via agglomerative clustering** of field/method dependencies (Fokaefs et al., *JSS* 85(10), **2012**, http://users.encs.concordia.ca/~nikolaos/publications/JSS_2012.pdf). **No identifier-name signal; no git history.**
- **Marinescu — "Detection Strategies: Metrics-based rules for detecting design flaws"** — R. Marinescu, *ICSME* **2004** (DOI 10.1109/ICSM.2004.1357798). The metrics-strategy basis underlying inFusion/Designite God-Class/Brain-Class detectors.
- **Designite / DesigniteJava** — T. Sharma et al., https://designite.net/ — Smell/metric detection (God Class, Feature Envy, Brain Class) for C#/Java; static-metric based.
- **inFusion / Structure101 / SonarQube (temporal "new code" rules)** — inFusion (Marinescu lineage, commercial); Structure101 (structural dependency smells); SonarQube temporal rules key on *when* an issue was introduced (revision/PR), not on before/after metric-movement calibration.

**Signals used** — AST + dependency graph + metrics (size, coupling, cohesion, complexity). **No tool in this set uses identifier-NAME repetition across functions as a signal, and none uses git time as a calibration oracle.**

**Overlap with sprefa** — JDeodorant's God-Class → Extract-Class *clustering* directly contests `context-object.dl`'s novelty. sprefa's `param-fan-out.dl` (`>=25` locals → god-fn) is squarely in Marinescu/Designite territory (Long-Method/God-Function metric threshold).

**Gap sprefa could claim** — None of these read **SCIP-local relation facts** in a Datalog recipe language, none use **identifier repetition as a missing-abstraction signal**, and none auto-tune thresholds from git-mined refactors. The recipes are not novel in spirit; the substrate (datalog-over-SCIP, user-writable) and the calibration are.

## E. Code-smell temporal / decay studies; venues

**Closest work**
- **Smell co-occurrence lifecycle** — F. Palomba, G. Bavota, M. Di Penta, F. Fasano, R. Oliveto, A. De Lucia, "A Large-Scale Empirical Study on the Lifecycle of Code Smell Co-occurrences," *IST* 99:1–13, **2018**. https://fpalomba.github.io/pdf/Journals/J11.pdf — 13 smell types × 395 releases × 30 systems; shows smells **persist** and that their removal tends to improve complexity/cohesion.
- **Smell decay/persistence** — Peters & Zaidman, "Measuring the lifecycle of a code smell," *ICSM* **2012**; Tufano et al., "When and Why Your Code Starts to Smell Bad," *ICSE* **2015**.
- **MSR is the right venue** — Mining Software Repositories, https://conf.researchr.org/series/msr. Core venues ranked: **MSR** (best fit), **ICSE/FSE** (full papers), **ICSME/SANER** (maintainability, refactor tooling — JDeodorant/RefactoringMiner home turf), **EMSE/TSE** (long empirical validation), **SCAM** (tooling).

**Overlap with sprefa** — The "smells have a lifecycle across releases" finding is the *precondition* for sprefa's time-as-oracle; sprefa operationalizes it as a calibration procedure rather than reporting it as a finding.

**Gap sprefa could claim** — These studies measure smell presence/absence over time descriptively. None frame the **signed movement of a recipe metric across a known refactor commit as a unit-level calibration signal**, and none compound multiple recipe movements through the commit as a join key.

## F. Lexical / identifier-name signals (sprefa's most distinctive axis)

**Closest work**
- **Identifier normalization / semantics** — D. Lawrie & D. Binkley, "Expanding Identifiers to Normalize Source Code Vocabulary," *ICSM* **2011**, https://doi.org/10.1109/ICSM.2011.6080778. Earlier: Lawrie, Binkley, Morrell, "Normalizing Source Code Vocabulary," *SCAM* **2007**.
- **Linguistic antipatterns** — B. Abebe & P. Tonella, "Linguistic Antipatterns..." (*ICSE/WCRE* **2011**, *SCAM* **2014**); Abebe, Tonella, Ricca, *EMSE* **2014** — inconsistencies between identifier *names* and *behavior*. Closest prior art to "names carry meaning; mine them." Targets name↔semantics mismatch, **not name repetition as a missing-type smell**.
- **Name-based concept location / feature location** — Hill et al. (*MSR* **2008**); Marcus & Maletic, LSI over identifiers (*ICSM* **2003**) — use identifier text to locate concepts/features. Framed as *retrieval*, not *refactor smell*.

**Overlap with sprefa** — **`missing-type.dl` is sprefa's strongest, most distinctive recipe.** Found **no prior work that uses cross-function/cross-file frequency of a repeated local identifier as a missing-type/missing-abstraction detector.** Linguistic antipatterns, concept location, and identifier normalization all touch identifier text, but none frame "this name is a de-facto type that was never extracted" as a repetition-frequency smell.

**Gap sprefa could claim** — **Likely NOVEL** (as a signal), modulo a careful related-work search in EMSE/SCAM. The closest structural cousin is the classic **"Data Clump"** smell (Fowler) and its detectors — *parameter-list* clumps (multiple methods sharing the same parameter set → extract a parameter object). sprefa's `context-object.dl` (co-occurring *locals* → candidate struct) is essentially **Data Clump detection extended from parameter lists to function-local variable names via a co-occurrence SCC**. That extension is the novelty; must be framed against existing Data-Clump/Data-Class detectors to survive review.

## G. Datalog / logic-programming for static analysis & refactor

**Closest work**
- **CodeQL / .ql (Semmle)** — GitHub, https://codeql.github.com ; de Moor, Sereni, Verbaere, Distefano, ".QL," *CASCON* **2007**; Avgustinov et al., "QL: Object-oriented Queries on Relational Data," *OOPSLA* **2016**. Datalog-style queries over a per-snapshot extracted database; huge library of security (and some quality) queries. **One CodeQL database = one snapshot; not natively revision/diff-aware.**
- **Doop** — Bravenboer & Smaragdakis, *OOPSLA* **2009**; https://github.com/plast-lab/doop — Datalog (Soufflé / LogiQL / **DDlog**) pointer analysis. Doop supports **Differential Datalog (DDlog)** for *incremental* analysis — the closest analogue to "rev-aware incremental facts."
- **Soufflé** — Scholz et al., https://souffle-lang.github.io — Datalog-to-C++ synthesis.
- **CrocoPat** — D. Beyer & C. Lewerentz (FSE/tech-report ~**2003–2005**) — relational calculus (RDL) over code graphs; architecture/architecture-smell queries. Pre-CodeQL logic-over-code lineage.

**CodeQL for smells?** Community packs detect some code-smell patterns, but standard packs are security-dominated. **Is CodeQL rev/temporal-aware? No** — a CodeQL DB is snapshot-bound; "temporal" queries are approximated by running two DBs and diffing results externally (no first-class `rev` join like sprefa's `module_edge_rev`). **Datalog-over-git** essentially does not exist as a first-class construct; the closest is Doop+DDlog *incremental* analysis (incremental over changes, but to a single program, not mining history as labeled data).

**Overlap with sprefa** — sprefa *is* datalog-over-SCIP. This axis is **DONE as a substrate** (CodeQL/Semmle own "declarative queries over code"). `scip_local(fn,name)` is the CodeQL-equivalent fact extraction; the `.dl` recipe layer is `.ql`-equivalent.

**Gap sprefa could claim** — (1) **first-class revision as a Datalog relation** (`scan(rev,...)`, `match(path,rev,...)`) — CodeQL has no such join; making the **SCIP structural axis** equally rev-aware would be a genuine Datalog-over-git contribution. (2) **Recipes as user-writable refactor *suggestions*** vs CodeQL's detector-only posture. Don't claim the datalog substrate; claim the rev-keyed join and the suggestion framing.

## H. "Time as join key for recipe compounding"

**Closest work**
- **CodeScene temporal trend dashboards** (section B) — tracks metric *trajectories* over time and flags decline. Single-metric trend, business-impact validated.
- **Quality-evolution / technical-debt trend tools** — Code Climate (https://codeclimate.com), Codeac (https://codeac.io), Codecov (https://about.codecov.io), SonarQube "New Code" / Quality Gate history — track aggregate quality over commits; **no cross-recipe joint confirmation**.
- **Multi-metric joint smell validation across history** — Palomba et al. (*IST* **2018**) on *co-occurrence* of smells and their joint removal; Bavota et al. on relational/clustered smells. These study co-occurrence **at a snapshot**, not "metric-A moves AND metric-B moves together across the same commit."

**Overlap with sprefa** — Trend/evolution tracking is **DONE** (commercially and academically). The specific framing — *a smell is confirmed when, across one commit, metric-A moves (local-count drops) WHILE metric-B's cluster disappears, with git commit as the join key* — is not found in the literature or the products. Multi-signal *agreement at a single commit* is the unclaimed slice.

**Gap sprefa could claim** — "Commit as the join key that compounds otherwise-isolated recipes" is a **plausibly novel composition operator**, but a methodological/contribution novelty on top of tools that already track trends individually. Frame it as a *precision booster* (recipe agreement reduces false positives), validated against RefactoringMiner-mined refactors.

## I. Novelty assessment

| # | Axis | Verdict | Closest prior art that contests it |
|---|------|---------|------------------------------------|
| 1 | **Lexical name-repetition as missing-type signal** (`missing-type.dl`) | **NOVEL** (as a signal) | Fowler *Data Clump* + detectors (parameter-clump → parameter object); Linguistic antipatterns (Abebe & Tonella, *ICSE* **2011**); identifier normalization (Lawrie & Binkley, *ICSM* **2011**). None use cross-fn **local-variable name frequency** as a missing-abstraction smell. `context-object.dl` is Data-Clump-extended-to-locals — also novel-but-adjacent. |
| 2 | **Datalog-over-SCIP recipes as user-writable refactor *suggestions*** | **ADJACENT** (substrate DONE) | **CodeQL/.ql (Semmle)**, de Moor *CASCON* **2007** / Avgustinov *OOPSLA* **2016** — owns "declarative queries over code." Doop + Soufflé own the datalog engine layer. **Doop + DDlog** owns "incremental datalog over changes." Novel residue is *not* the datalog layer — it's (a) first-class **`rev` as a relation** on the *structural* axis (CodeQL has none), and (b) recipes that *propose* refactor structure, not just detect. |
| 3 | **Git-time as threshold-calibration oracle replacing human raters** | **DONE** as protocol, **NOVEL** as framing | Empirical-SE's "mined-refactor-as-label" evaluation (Palomba *TSE* **2015**; RefactoringMiner 2.0, Tsantalis *TSE* **2020**). Using refactors as before/after labels is established methodology. Unclaimed slice is *threshold derivation* (fitting cutoffs to the distribution of signed metric movements) and *commit-as-join-key* compounding (axis H). CodeScene already mines git for refactor prioritization — so "git as oracle" broadly is DONE; "git as a calibration oracle for N-rater replacement" is a thin, defensible slice. |

**Honest summary** — The only axis clearly **NOVEL as a signal** is #1 (identifier-repetition-as-missing-type). #2's substrate is squarely DONE (CodeQL/Semmle) — defensible contribution is the rev-keyed structural join + suggestion framing. #3's methodology is DONE — defensible contribution is calibration-from-movement-distributions and commit-as-join-key, not "use git as labels."

**Venues to target (ranked)**
1. **MSR** — best fit; "mining git for labeled before/after refactor pairs + datalog recipes" is MSR-shaped. RefactoringMiner/RefDiff lineage lives here.
2. **ICSME** (tool + research tracks) — refactor-suggestion tooling home (JDeodorant, Designite). The `.dl` engine + recipes are an ICSME-tool paper.
3. **SCAM** — identifier-signal novelty and the datalog recipe layer fit SCAM's tooling/signal focus.
4. **SANER** — architecture/structural smells, decomposition-quality (Bunch/MQ lineage), rev-aware analysis.
5. **FSE / ICSE** — if the calibration methodology (threshold-from-metric-movement, commit-as-join-key) is the headline; harder bar.
6. **EMSE / TSE** — long empirical validation once a recipe set + mined-refactor corpus is mature.

**Datasets / tools to reuse**
- **RefactoringMiner 2.0/3.x** — N. Tsantalis et al., https://github.com/tsantalis/RefactoringMiner. *The* label generator; also its MCP server. Oracles: http://refactoring.encs.concordia.ca/oracle/ and https://github.com/ameyaKetkar/RMinerEvaluationTools.
- **RefDiff** — https://github.com/GumTreeDiff/RefDiff — cross-validate RMiner output.
- **CodeQL** — https://codeql.github.com / https://github.com/github/codeql — baseline structural-query comparison and to demonstrate the rev-join gap.
- **Doop (DDlog mode)** — https://github.com/plast-lab/doop — reference for incremental/rev-aware datalog design.
- **JDeodorant** — https://github.com/tsantalis/JDeodorant — baseline for God-Class/Extract-Class clustering comparison against `context-object.dl`.
- **Palomba et al. 2018 smell-lifecycle corpus** (395 releases × 30 systems) — replication package for longitudinal validation.
- **rust-analyzer / SCIP** — https://github.com/sourcegraph/scip — the index format sprefa consumes.

**Suggested positioning (3 sentences)**
Lead with the **identifier-repetition-as-missing-type signal** (`missing-type.dl`) — sprefa's only clearly novel *signal*, defensible against Data-Clump/linguistic-antipattern work if framed as "missing *type* abstraction detected via cross-function local-name frequency over a SCIP index." Use **RefactoringMiner-mined Extract-Class / Split-Method / Extract-Struct commits as labeled before/after pairs** to *show* recipe metrics move correctly (established protocol), but pitch the **contribution as the calibration procedure** — fitting recipe thresholds to the distribution of signed metric movements, and using **the commit as a join key to compound isolated recipes** (local-count-drop ∧ cluster-disappearance = confirmed refactor) for precision. Explicitly concede the datalog substrate (CodeQL/Semmle) and the git-as-oracle methodology (empirical-SE standard) as prior art; claim only (a) the name-frequency signal, (b) rev-awareness on the *structural* SCIP axis, and (c) commit-keyed recipe compounding.
