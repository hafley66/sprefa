# Change-cost friction inventory (agent-sourced, 2026-07-10)

## Context

Chris asked "how can we make changes cost less" after months of monotonic code growth.
This inventory consolidates the debriefs of ~10 worktree agents from the 2026-07-10
arcs (Go/Python TypeLangs, parity twins, per-site scorer, otel corpus measurement,
nested-repo walker fix) plus orchestrator-side pains, ranked by how many INDEPENDENT
agents hit the same wall. Repeat sightings are the strongest signal of where
change-cost actually lives: every item below was paid for at least once in real debug
time, most of them several times. The claim this ranking encodes: cost lives in
wait-time, collision surface, and undocumented contracts — not line count per se.

Two items are resolved context, listed here so nobody re-litigates them: the 36-54s
pre-commit hook cost and the served-copy divergence both collapse once the running
daemon is restarted onto the current binary (Chris-only action, ledgered in
CLAUDE.md). The walker/nested-repo hazard found during the corpus arc is FIXED
(main 3bdcce5).

## The inventory, ranked

### 1. Ambient config + daemon hijack (4+ sightings)

Every ad-hoc `dl` run at a config-registered root silently ingests the global
`~/.config/sprefa/config.toml` repos ("[config] 3 repo(s) registered") and, when a
daemon serves the root, routes writes to the daemon's db with no visible signal.
Hermetic testing is currently a folk incantation — `SPREFA_CONFIG=/nonexistent/...`
plus `--no-daemon` plus a scratch `--db` — that every agent brief must recite and
every new consumer discovers by being burned. Fix shape: a first-class `--hermetic`
flag that implies all three, plus the already-ledgered loud "daemon is serving this
root, writes went there" warning on hijack; S-sized, mostly CLI plumbing.

### 2. Cross-family hidden dependencies (3 sightings, 2 shipped as real bugs)

A family's resolver reads another family's rels but nothing forces the producer to
run: ModuleFamily shipped this bug (type/call programs got no module graph, narrowing
silently no-oped) and ScipKind shipped the identical shape (SPREFA_SCIP_INDEX was a
silent no-op for exactly the call/type programs it should improve, found only when
the corpus with-scip arm measured byte-identical to without). Both were fixed by
hand-ORing the consumer's usage into the producer's `used()`, which works but leaves
the dependency invisible to the next reader and unauditable by the magic-rel rail.
Fix shape: a declared `reads: &[family]` edge on ExtractFamily/RelKind so `used()`
composes mechanically and the rail can assert no resolver reads an undeclared
family's tables; M-sized, engine registry work.

### 3. dl query output format is undocumented lore (3 sightings)

Query output indents data rows with two leading spaces, delimits sections with
`? relname` lines, and offers no stable machine format — so `oracle_parity.rs`
carries a 34-line hand-rolled stdout parser that every new oracle consumer
re-derives, and the per-site scorer agent lost a debug round-trip to the indent
mis-keying cell 0 (the previous scorer only worked because its keyed value happened
to live in cell 3). Fix shape: `--format=json` (or tsv) for `?` query blocks and one
paragraph documenting the human format's contract; S-M sized, deletes the parser
class entirely.

### 4. engine/mod.rs monolith (3 sightings, standing epic)

At ~7,800 lines the engine file has no readable entry point for cross-cutting
contracts: "which tier answers call resolution" required source spelunking for the
corpus agent, `AST_LANG_TABLE` sits at line ~7,674 as ledgered placement debt, and
parallel language arcs collide at shared append anchors (the Go+Python double-land
needed hand-merge scripts). The trait-extraction epic (RelKind = phase 1) is already
designed with coupling measured at 93 family members / 89 dispatch sites, and has
been parked since June while the arcs that would have benefited kept landing.
Fix shape: run the epic; per-language extractor files behind the registry alone
would remove the same-anchor collision class for language work.

### 5. Resolution-tier invisibility (2 sightings + this session's cost)

There is no way to ask "which tier answered this call_def row" — scip override,
syntactic unique-def, alias hop, or import narrowing — so proving the with-scip arm
measured nothing cost the corpus agent a probe cycle and the orchestrator a
verification cycle, and the turnkey agent's ensure-families pain is the same
blindness from the demand side. Fix shape: a `resolution_source` column (values
scip|syntactic|alias|narrowed) on call_edge/type_link (rev twins included), plus a
public `eng.ensure_families(&[...])`; S-M sized, and it turns every future
"why did/didn't this resolve" investigation into one query.

### 6. Per-language dataflow coverage is invisible (3 sightings)

TS class-method bodies emit zero df nodes, `return lookup()` bodies produced no
nodes for the turnkey agent, and nothing surfaces per-language df coverage until a
df-dependent query silently under-reports. Fix shape: a generated per-language
coverage table (which node kinds each TypeLang lift emits, tested counts on a
fixture) in docs/reference, regenerated the same way the rel catalog is; S-sized
docs work with an optional rail.

### 7. Worktree agent base traps (2 landings complicated)

Agent worktrees spawn from whatever the branch tip was, not from the commits the
task brief references: the corpus agent's worktree predated both the scorer and the
corpus pins ("main just landed X" was true only of the unpushed local main, costing
~15 min of git archaeology plus a mid-task merge), and the twins agent's worktree
spawned off a foreign feature branch so its branch carried alien merge history
(landed by cherry-pick to avoid pulling that branch into main). Fix shape: process,
not code — every agent brief names the base SHA and the orchestrator verifies
branch topology (`git merge-base`) before landing; zero-cost once habitual.

### 8. Submodule/gitlink scan hang (1 sighting, new)

`dl` scanning a git-submodule root hangs silently before the first tick — no
`[tick]` line, no error — because root/rev handling wedges on the gitlink `.git`
FILE (`gitdir: ...`) form. The corpus agent only survived because its brief warned
it; an unwarned user gets an indefinite hang with zero signal. Fix shape: detect the
gitlink form at root resolution and either handle it or fail loudly ("root .git is a
gitlink; scan a copy or the superproject"); S-sized.

### 9. Rel line-base lore (2 sightings)

Positional rels disagree on line base — `comment_node` 1-based, `scip_occurrence`
0-based, df nodes 1-based (after the Kotlin normalization arc) — and the base is
stated nowhere, so consumers learn it by reading extractor source or by producing
off-by-one joins. The parity scorer does the 1→0 conversion in exactly one commented
place, which is the right pattern but currently tribal. Fix shape: state the base in
every positional RelDecl doc string and regenerate the reference; S-sized, one sweep.

### 10. Pre-commit hook noise (2 sightings)

The hook prints three `info[op-example]` lines to stdout mid-commit — benign but
alarming enough that two separate agents flagged it as "did my commit break
something". Fix shape: route info-severity rail output to stderr or suppress info
under `--check`; trivial.

### 11. call_def.sym doc drift (1 sighting)

The RelDecl doc says the sym is bare `file::kind::name` but the emitted value is
repo-qualified (`repo::file::kind::name`, matching call_edge.callee_q), so the
scorer agent only trusted the join after querying live rows. Fix shape: one-line doc
correction plus a regen; trivial, and it compounds with item 9's sweep.

### 12. S6: body-level source+derived mix silently drops the rel atom (1 sighting)

A scan+match rule whose body also joins a derived rel runs the extraction and
silently IGNORES the join — no bail, no warning — while the rel-level mixed guard
set the expectation that this case would also bail loudly. It cost a failing-test
loop during the gen-lang-skill arc and remains a sharp edge for every .dl author.
Fix shape: extend the mixed-kind guard to body-level (bail with "move the join to a
derived rel"); S-sized, parse/typecheck work.

## Sequencing

Cheapest-first: 10 + 11 + 9 in one docs/regen sweep (one sitting); 8 and 1 as one
CLI-ergonomics arc; 3 as its own S-M arc (removes a whole parser class); 5 rides the
next resolver-touching arc; 2 and 12 are engine-guard arcs; 4 is the standing epic
and the only L. Items 7 is orchestrator process, effective immediately.
