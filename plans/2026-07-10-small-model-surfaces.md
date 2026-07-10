# Small-model surfaces: the build plan for "dl as a steroid for Haiku"

## Scope split (read this first)

Three documents, three jobs, no overlap:

- **This plan**: the BUILD list — surfaces that make a Haiku-class model
  effective on codebases. Ranked, sized, with the existing seam each rides.
- plans/2026-07-10-agent-eval-harness.md: the MEASUREMENT instrument that
  says whether any of this works (falsifiable, preregistered, scriptable).
  Build item 1 here is that plan's S0 prerequisite; nothing else from this
  list may leak into the harness's task design.
- plans/2026-07-10-fable-possibility-mining.md: the open-ended idea mine.
  Its direction 7 (agent-facing surfaces) is superseded by this plan for
  the small-model slice; the mining session should not re-derive these.

The thesis being built (and separately tested): a small model's weakness is
multi-hop synthesis over large context; dl moves exactly that into the
engine, so the model only picks a question and reads a small structured
answer.

## Build items (ranked)

### 1. MCP tools: dl.what / dl.verb / dl.rows (S)

The already-ledgered decided-unbuilt item; this plan is its forcing
function. Three tools in the `--mcp` adapter (src/mcp.rs), schemas strict
enough that misuse is hard: `dl.what {anchor}` -> the anchor resolver
(src/anchor.rs); `dl.verb {verb, target}` -> the `dl q` runner (turnkey
plan item 3); `dl.rows {rel, limit}` -> query_rel with hard caps.
Count-first discipline in the contract: every rows response carries the
unpaged total; descriptions tell the model to check counts before paging.
Prereq for the eval harness's +dl cells AND for every item below being
reachable from a hosted agent.

### 2. Context packs: `dl q pack <sym> [--budget N]` (M)

One command returns the minimal graph slice around a symbol as ONE nested
JSON blob: definition + signature (type_entity/type_sig), direct
callers/callees (call_edge, 1-hop), the types it touches (type_link),
flow membership (flow_member if goto-flows/taint programs are loaded),
doc_comment, and the file:line of everything. Assembled by joins, packed
with the query-head json aggs (shipped 2026-07-10), truncated
deepest-first to the byte budget with an explicit `"truncated": [...]`
field so the model knows what it did not get. Small models are bad at
re-joining rows across calls; the pack pre-joins. This is the single
highest-leverage surface and the flagship exhibit for the eval harness's
+dl cells once it exists (harness runs with and without pack access are
the natural ablation).

### 3. Candidate-loop convention + one reference harness (S-M)

The ruled architecture ("LLM = candidate brain, dl = deterministic
executor") operationalized for cheap models: dl enumerates candidates
(`lint_candidate` rows, the std/suppress convention), a driver script
calls one Haiku per candidate with that candidate's context pack (item 2)
and a bounded yes/no/why contract, accepted fixes route through the edit
sink / --fix. Deliverable: `examples/candidate-loop/` with the driver +
one real rail (unwrap-safety or dead-export triage) run on this repo,
results in docs/dogfood/. The small model never sees the repo, only
bounded judgments.

### 4. Paste-ready errors, completed (S, ongoing standard)

The agent-sharp-edges arc started it; finish the standard: every engine
bail and every lint diag carries the exact fix text (the mixed-rel bail,
the reserved-name guards, and the lattice wedge messages already do; audit
the rest with a grep for bail!/diag emits lacking a "fix:" shape). Small
models recover from errors that ARE the correction and flail on vague
ones. Cheap, permanent leverage.

### 5. hover_note as the agent write channel (S, rides F1)

The sink exists (shipped 2026-07-10). What is missing is the convention +
one worked example: an agent session writes findings as hover_note rows
(via dl/hookEvent-style insertion or a .dl the agent generates), a human
sees them on hover, a `? hover_note(...)` triages them. Deliverable: an
example .dl + a docs section defining the note format (severity prefix,
session tag in the md) so multiple agents can annotate without colliding.

### 6. Ambient context injection via hooks (S)

UserPromptSubmit hook (seam exists: dl --hook + hook_event) injects a few
dozen tokens of map per prompt: "file X is in flows: checkout-path; BOM
fan-in 324; 2 open diags". Policy lives in a .dl (wasm-generality law);
the engine ships nothing new. Orientation is what small models lack most
and this is nearly free.

## Explicit non-goals

- Making small models WRITE .dl. Verbs, tools, packs are the surface;
  authoring stays with bigger models; the sharp-edge lints exist for when
  it happens anyway.
- Any claim about effectiveness. That is the eval harness's job; this plan
  only builds the surfaces the harness ablates.

## Sequencing

1 -> 2 (pack rides the verb runner) -> harness S1 can run its +dl cells ->
3 (loop uses packs) in parallel with 4/5/6 (independent, S each). Items
4-6 need no prerequisite and can fill agent idle time.

## Critical files

- src/mcp.rs (tool registration), src/cli/query.rs + src/anchor.rs (verb
  runner the tools wrap), src/rels/ (pack reads), examples/ +
  docs/dogfood/ (deliverables 3/5), assets/*.skill.md (conventions travel
  via dl setup --project)
