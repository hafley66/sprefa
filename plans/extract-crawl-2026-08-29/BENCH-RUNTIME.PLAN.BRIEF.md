# Lane `plan/bench-runtime` (opus): PLAN ONLY, a bench runtime with no language, engine or dep in its scope

First action: `git merge --ff-only 41333391a95c601dfcc8f7372bcfaa6ea72c5d76`.
Failure or missing tree = STOP, beep the coordinator, do not work around it.

**This lane writes NO implementation code.** Two documents, no `src/**` edit,
no `tests/**` edit. A lane that ships code here has failed the brief.

## Goal, one sentence

Design a measurement runtime whose scope contains no language, no engine and no
dependency: every row producer (our extractor, CodeQL, madge, joern, jelly,
PyCG, scip, a committed tsv) is one manifest row, so adding a language or a tool
is a row and never a code change.

## The user's words

"i want commonality and non language/engine/deps scope locked test runtime",
after accepting option A: the 68 committed oracle tsvs keep `cost = None`
forever; only producers that run under the runtime from here on carry real cost.

## Why (zero-context reader)

The accuracy numbers this repo quotes come from three unrelated machines:

| producer kind | how it runs today | receipt |
|---|---|---|
| our extractor | in-process `resolve_project`, 3 runs, median wall | `v6/sprefa-extract/tests/bench/mod.rs:755-813` |
| external tools | one-off scripts, hand-run, numbers hand-copied | `plans/extract-{bench,crawl}-2026-08-29/*.py`, 11 files, 1,811 lines |
| frozen artifacts | 68 committed `.tsv` across 14 tools | `ls plans/extract-bench-2026-08-29/*.tsv` |

Two seams matter and only one exists:

- **Row seam: EXISTS and is good.** `src_path \t src_name \t dst_path \t dst_name`,
  `COMMON.md:23-28`. Every tool in every pid already lands here. Do not touch it.
- **Measurement seam: does not exist.** Who ran, over what corpus, at what tier,
  what it cost, what it scored. Today: prose in REPORT.md, hand-typed percents
  (`plans/extract-crawl-2026-08-29/ts.REPORT.md:637`), and one 8-column TSV.

The pid boundary is the hard constraint. `tests/bench/mod.rs:817` calls
`getrusage(RUSAGE_SELF)`. That number is truthful only when the rows were
produced by the scoring process. For a spawned tool the mechanism is `wait4` /
`RUSAGE_CHILDREN`; for a tsv CodeQL wrote weeks ago on another machine there is
no mechanism. Cost must be stamped by the producer and carried, never sampled
by the scorer.

A sibling lane `feat/extract-tier-axis` is landing the in-process half right now
(tier as a first-class column, `RATCHET.tsv` + `RATCHET.cost.tsv`). Read its
brief at `plans/extract-crawl-2026-08-29/TIER-AXIS.BRIEF.md` in this same briefs
directory. Your design MUST subsume it, not contradict it, and must reproduce
every number it pins.

## MANDATORY FIRST SECTION: build vs buy

Repo law, non-negotiable: never assert "write our own" for a common-shaped
problem without library research and a written candidate-by-candidate analysis
first. No one-line dismissals. This problem is a declarative benchmark matrix
with subprocess timing, resource accounting and regression floors, which is
about as common-shaped as it gets.

Research at minimum these, one subsection each, with what it does, what it
cannot do here, and the exact reason it is in or out:

| candidate | angle to test |
|---|---|
| `hyperfine` | subprocess timing + JSON export + warmup; does it carry rss and a custom payload? |
| `conbench` | benchmark result store + regression detection across runs and machines |
| `asv` (airspeed velocity) | matrix over configurations, regression floors, HTML report |
| `bencher.dev` | ratchet/threshold service over CI, self-hostable? |
| `criterion` / `iai-callgrind` | in-process Rust micro-bench; state plainly why the shape is wrong |
| `pytest-benchmark` | matrix + floors, if the runner were python |
| `snakemake` / `nextflow` / plain `make` / `just` | the runner half alone, DAG of producers with cached artifacts |
| `dvc` | pipeline + artifact versioning for the 68 frozen tsvs |
| OpenTelemetry + `hafley-observe` | already linked (`v6/sprefa-extract/Cargo.toml:197`, installed `src/trace.rs:257-266`); the emission-record transport |

State the verdict as a table, then the decision. A bespoke runtime is allowed
ONLY if the analysis shows every candidate fails on a named requirement.

## The contract to design (shape it, do not code it)

Design and write down, in this order (planning protocol):

1. **Type signatures.** `SourceId`, `Emission`, `Cost`, the producer contract.
   `Cost` fields are `Option` because a frozen tsv has none and a `0` would be
   a lie the scorer cannot detect.
2. **Pseudo-code body** as a comment under each signature.
3. **Instance lifetimes** for every type that holds state.
4. **Storage layout**, then the sequence of reads and writes, then the
   uniqueness conditions.

Constraints the design must satisfy:

- **No language in scope.** The runtime must not name Rust, TypeScript, Go,
  python, node, cargo or ra_ap anywhere in its own code. Those appear only as
  manifest data.
- **No engine in scope.** No knowledge of `resolve_project`, `ScipMode`,
  `ResolveArms` or any sprefa type. Our extractor is a producer like CodeQL is.
- **No dep in scope.** Running the runtime must not require the rust-checker
  crate graph, a node install, or a CodeQL database. A producer whose tool is
  absent reports `unavailable` and its cases skip loudly, never silently.
- **A producer is a command.** Kinds: `command` (spawn, time with `wait4`, read
  4-col rows) and `file` (a frozen tsv, cost `None`). Say explicitly whether
  our extractor stays in-process as an optimisation or becomes a plain
  `command`, and price both: the in-process path is what today's
  dev-profile/missing-feature traps came from (`failure-modes.md` 101, and the
  rails at `tests/bench/mod.rs:758-768`), and a spawned binary makes those
  traps structurally impossible. Recommend one.
- **Identity is mandatory, cost is optional.** Every one of the 68 frozen tsvs
  gets a manifest row naming tool, tool version, the command that produced it,
  corpus, corpus sha, produced_at and row count. Cost stays `None`. NO
  regeneration of any oracle in this design (user decision A).
- **Adding a language is a manifest row.** Prove it: walk through adding kotlin
  (`v6/sprefa-extract/src/lang/kotlin*`, 1,845 lines, zero oracle today) and
  show which rows appear and which code does not change.
- **The tier axis is `{syntax, checker, scip}`** (`COMMON.md:75`) and applies to
  every producer, ours and theirs alike. CodeQL is a checker-tier tool.
- **Surrogate keys.** If any part of the design stores rows in SQLite, integer
  primary keys, natural keys in a dictionary table with UNIQUE, no composite
  TEXT primary key. Read `.claude/skills/sql-relational-design` and
  `.claude/skills/sqlite-costs` BEFORE writing any schema.
- **Reproduction is the acceptance test.** The design must state how it
  re-derives, byte for byte, every row `feat/extract-tier-axis` lands in
  `RATCHET.tsv`, plus the ts5 checker pair `97.33 / 70.36` and `95.02 / 76.73`
  from `ts.REPORT.md:637`.
- **The 10-second law.** No single operation over 10s in the runtime's own
  path. Producers are spawned with a per-producer cap, killed at the cap, and
  the kill is a reported outcome, never a wait.

## Deliverables (exactly two files, both new)

1. `plans/extract-eval-2026-08-31/BENCH-RUNTIME.PLAN.md`
   - opens with a TOC
   - section 1 IS the build-vs-buy analysis, candidate by candidate
   - then the four planning layers above
   - every claim carries a `path:line` or a command that prints it
   - a migration table: each of the 11 python scripts and each of the 5 ratchet
     legs, and what replaces it
   - an arc list with a dependency order, each arc small enough for one PR
2. `plans/extract-eval-2026-08-31/BENCH-RUNTIME.PLAN.visual.human.unga.md`
   - for Chris, who will not read citations
   - plain words, diagrams, ZERO citations, zero percent signs without their
     fraction
   - mermaid over ascii art; ascii only for a tick timeline mermaid cannot
     express
   - a plan without this second doc is undelivered

## Files you own

- `plans/extract-eval-2026-08-31/BENCH-RUNTIME.PLAN.md` (new)
- `plans/extract-eval-2026-08-31/BENCH-RUNTIME.PLAN.visual.human.unga.md` (new)

FORBIDDEN: every `src/**`, every `tests/**`, `v6/justfile`, `RATCHET.tsv`,
`RATCHET.cost.tsv`, `COMMON.md`, everything the sibling lane
`feat/extract-tier-axis` owns, the corpus checkouts, `v6/prolog`, `v6/tsv2`,
root `src/`.

## Laws in force

- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers. The word is "oracle", never "ground truth". The word
  "refusal" is banned in prose; an unbuilt construct is "TODO" or "not built
  yet".
- Docs open with a TOC. Output to the user is lists, tables, mermaid and TOCs;
  prose is a one-line caption under a diagram.
- Every accuracy number anywhere carries the full measure signature of
  `COMMON.md:58`: `⟨lang.family.tier⟩ vs ⟨oracle⟩ on ⟨corpus, n files⟩ :
  ⟨metric⟩ = numerator/denominator ⟨row unit⟩`. A percent with no fraction
  beside it is banned.
- Descriptive names, never single-letter.
- Doubt yourself before asserting. A refusal you find in the code is a
  hypothesis, not an edict; trace it to the throw site and cite it.
- Never `--no-verify`. A blocked command ends the approach; beep the
  coordinator.
- 10-second law, waiting posture: no foreground wait over 10s.
- You are a lane: never spawn a subagent.

## Reaching the coordinator

```
boop beep --no-wait --as plan-bench-runtime sprefa-coordinator "<one line: what, where, what you need>"
```

Blocked, done (with the PR number), or brief-is-wrong.
