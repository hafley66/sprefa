# Analysis engines + labs bake-off (native-speed lens, parse-only)

## Context

The v6 extract arc is done through phase-1 + resolve (ts/rust/go/kotlin,
parity-gold, worktree `.claude/worktrees/extract-golden-plan`, branch
`plan/extract-golden-plan`; seeds S1–S6 in
`v6/plans/2026-07-24-extract-go-closeout-and-resolve4.md`). The open
question is the ENGINE + analysis layer at hundreds-of-repos scale.

Hard constraints settled in discussion (2026-07-25):

- **Parse-only, no builds.** At hundreds of repos, build-tracing platforms
  (CodeQL compiled-lang extractors) are disqualified by rule; scip indexers
  are a sampled correctness oracle, never a runtime dependency (GitHub
  Stack Graphs is the prior art for build-less name resolution at fleet
  scale).
- **Native-speed lens.** User: "joern sounds nice but im looking for
  c/c++/zig/rust speed." Joern's SHAPE is the reference; its JVM tax is
  measured, not assumed. No candidate tool is zig-written — zig is an
  implementation-language option for OUR lab, not a candidate.
- **Budget law (the 30GB lesson).** Every lab declares its RAM budget
  BEFORE its first scale tier; exceeding it = fail, not surprise.
  See `.agents/skills/sprf-dd-memory-tiering-500/SKILL.md`.
- **Oracle law (the AI-rails lesson).** No lab is evaluated on vibes: same
  corpus, same questions, answers diffed against the control/oracle.

Method: AI iterates on labs — one lab per candidate under
`labs/bakeoff/<tool>/`, all answering the SAME question battery (below)
over the SAME corpus tiers, producing a scale matrix (tool × question ×
scale × {wall, peak RSS, LOC, cold-start}).

## Question battery (the fixed test set)

- Q1 module dep matrix + layering violations (expected output exists:
  dogfood #2 artifact in the extract plan).
- Q2 dead defs with consumer pools (verified lists exist, dogfood #2).
- Q3 feature envy + god classes (expected: zero envy; DatalogEvaluator 175,
  Store 152, Tasks 99, HostRunner 71, DlRuntime 29).
- Q4 longest flow path + max interface/trait-boundary-crossing path
  (spec: seed S5 — Tarjan-condense then DAG DP, w=hops or w=crossings).
- Q5 taint source→sink with sanitizer (S4 headline program).
- Q6 backward slice: what affects line X (needs control dependence).
- Q7 RTKQ golden: hook→op recovery + orphan-hook report (reference:
  `examples/rtkq-op-recovery.dl` + `examples/openapi-sim/` corpus).
- Q8 exact α-duplicate clusters (spec: seed S1 stage 0).
- Q9 community detection (Louvain) vs declared module structure.
- Q10 betweenness chokepoints (refactor blast-radius ranking).

<!-- todo(feature): bake-off harness — fact loader (extract JSONL) + Q1–Q10 runners + measure script (wall, peak RSS via memcap, LOC, cold-start) -->

## Corpus tiers (pinned revs, parse-only, NO builds)

- C0: `v6/sprefa-store` (rust+ts mirror — cross-language ratchet built in)
  + `examples/openapi-sim` (the RTKQ corpus).
- C1: this repo, one snapshot rev.
- C2: 10 repos from `bench/corpus/` (pinned list committed to the lab).
- C3: 100 repos (bench corpus or gh snapshot; cold + warm runs).

<!-- todo(triage): corpus tiers C0–C3 pinned revs committed to labs/bakeoff/CORPUS.md; parse-only, no builds allowed for any candidate -->

## Candidates

- **CONTROL: v5 `dl` engine (installed) + v6 extract JSONL.** The incumbent
  baseline; every candidate is judged against the control's answers AND
  its weight. If nothing beats the control, the verdict is "keep v5
  engine, park v6 store/dl forever" — a valid, cheap outcome.
- **Native shortlist (the speed lens):** Soufflé (C++), Kuzu (C++
  embedded graph db), a rust datafrog/Ascent lab over extract JSONL,
  graph-tool (C++ backend).
- **Shape reference:** Joern — evaluated for shape + scale with the JVM
  tax recorded; not an engine candidate unless it humiliates the field.
- **Read-only prior art (no bake-off):** Stack Graphs (rust, build-less
  name resolution — informs the ModuleF/specifiers ruling), Doop
  (Java-domain pointer analysis; literature).
- **Measures oracle:** NetworkX/graph-tool as the independent reference
  for Q9/Q10-style measures — same ratchet role scip plays for resolution.

Out of scope (named): CodeQL (build-tracing + license), Glean (infra
weight), Neo4j GDS (daemon), Semgrep (syntactic, not graph).

## Decisions

- **Head-to-head framing (user, 2026-07-25): "lab it out with joern and
  then see if rust can go faster."** Joern runs EARLY as the reference
  implementation of the shape (its numbers + its ergonomics are the bar);
  the rust lab (datafrog/Ascent or bespoke over extract JSONL) runs as the
  challenger whose explicit job is beating joern's wall/RSS at equal
  answers. Soufflé/Kuzu/graph-tool become OPTIONAL fillers only if the
  head-to-head is inconclusive.
- Bake-off over adoption-by-reading: rejected "just read Joern's docs" —
  shape knowledge ≠ scale knowledge.
- Parse-only rule: rejected build-tracing platforms and per-repo scip as
  runtime (hundreds-of-repos constraint; sampled-oracle only).
- Native-speed lens: rejected JVM platforms as ENGINE candidates (Joern
  demoted to shape reference) — user's c/c++/zig/rust-speed requirement.
- v5 engine as control: rejected evaluating candidates in a vacuum — the
  incumbent is the bar.
- Independent measures oracle: rejected trusting any single tool's
  numbers, including our own (NetworkX/graph-tool as reference impl).
- Agents never see each other's answers (no anchoring): rejected shared
  lab scratch space.

## Verification

- The deliverable is the scale matrix: tool × Q1–Q10 × C0–C3 ×
  {wall, peak RSS, LOC of question program, cold-start}, committed to
  `labs/bakeoff/MATRIX.md`.
- Ground truth: Q1–Q3 + Q7 (dogfood #2 artifacts + v5 rtkq). Oracle-diffed:
  Q4–Q6, Q8. Heuristic (marked, never asserted): Q9–Q10.
- Reproducibility: a fresh agent reproduces any lab from its README alone.
- Kill criteria per candidate: misses its declared RAM budget at C2;
  cannot express 3+ of Q1–Q10; or loses to the control on both axes
  (slower AND more code).
- Repo rail: `dl examples/gen-plans-index.dl --check` stays green.

<!-- todo(perf): RAM budget per lab declared BEFORE its first C2 run — exceeding = fail (30GB lesson) -->
<!-- todo(decision): per-candidate verdict at the human gate — adopt-as-engine / adopt-as-oracle / read-only / discard -->
<!-- todo(docs): scale matrix committed to labs/bakeoff/MATRIX.md (tool × Q1–Q10 × C0–C3 × wall/RSS/LOC/cold-start) -->

## Staffing

- Phase 0 — harness + corpus pinning + measure script + this battery as a
  runnable appendix: ONE agent (worktree TBD at kickoff; base = branch tip).
- Phase 1 — control: v5 `dl` answers Q1–Q10 over C0–C2: ONE agent.
- Phase 2 — REFERENCE: joern answers Q1–Q10 over C0–C2, JVM tax recorded
  (heap, cold-start, import time): ONE agent.
- Phase 3 — CHALLENGER: rust lab (datafrog/Ascent or bespoke over extract
  JSONL) answers the same battery with the explicit target of beating
  joern's wall/RSS at equal answers: ONE agent.
- Phase 3b (ONLY if 2 vs 3 is inconclusive) — Soufflé / Kuzu / graph-tool
  fillers: ONE agent each.
- Phase 4 — orchestrator assembles MATRIX.md + verdict table; HUMAN GATE
  decides adopt/retire per candidate.
- Suite budget: each agent runs its own tier once; NO agent starts C3
  without its C2 numbers reviewed by the orchestrator (RAM budget check).
- Agents do not read other candidates' labs or answers.
