# Fable possibility-mining session: what can dl do that nothing else can

## Mission

A dedicated Fable session whose whole job is to mine the possibility space of
dl: take the primitives that now exist (ref spine, type/call/df graphs,
rev-aware twins, hook_event, demand sinks, gen, edit sink, closure, argmax,
json aggs, hover_note, lattices, ports) and systematically find the
highest-value things they compose into. Not a feature sprint. The output is
proven possibilities, ranked, with the evidence attached.

## Method: the proving-query discipline

Every claimed possibility must be one runnable .dl plus a measured result on
a real corpus (this repo, or ~/orgs for scale claims). No possibility enters
the ranked list on vibes. The template per candidate:

1. One paragraph: the capability, who wants it, what tool it replaces.
2. The .dl (or the 2-3 rules that are its heart), run for real.
3. Measured rows: counts, one or two concrete hits pasted, tick cost from
   `stmt_ms`/`rel_count` if nontrivial.
4. Verdict: KILLER / USEFUL / PARLOR TRICK / NEEDS-ENGINE-GAP(name it).

The exemplar-pair trick is the standard miner: pick two known instances of a
suspected pattern, compute both footprints, intersect (anti-unify). Where the
intersection is dense, there is a pattern worth automating; where it is
empty, the hunch dies cheaply. This is the same shape as flow_common in the
goto-recorder plan and the shotgun-surgery scatter below; reuse the idiom.

Session protocol: Fable designs the queries and verdicts; when a candidate
graduates into engine or extension work, it becomes a plan file + Sonnet/
Opus briefs (the standing staffing rule; Fable does not bulk-implement).
Policy stays in .dl per the wasm-generality law; an engine feature is only
proposed when a proving query names the exact missing seam.

## Seed directions (ranked by suspected value)

### 1. Shotgun-surgery automation (the seed conversation, 2026-07-10)

When one conceptual change (new RPC method, new builtin rel, new TypeLang,
new CLI verb) fans out over N sites: match arms, const registries, catalog,
docs, test files, fs layout.

- **Tier 1, mine**: `scatter(name, file, kind)` from the ref spine
  (`ref` x `string.norm` on an exemplar command name), kind-tagged by CST
  containment (`node`/`child`); intersect two exemplars' scatters
  normalized-by-name = the derived checklist. Also `co_change(a, b, n)` from
  `git log --name-only` via a cmd effect: files that historically move
  together catch prose/doc sites name-scatter misses.
- **Tier 2, rail**: registration-completeness diags. Authoritative member
  set from the real registry (sg over `handle_request` arms, `rel_catalog`,
  `type_langs()`), one anti-join rule per satellite site, each diag naming
  the fix. Precedents already shipped: magic-rel-audit, gen-doc-indexes
  drift rail, lang_matrix. First target: daemon RPC methods (the thin-client
  plan is about to add refs/saved/repo_roots/diag_mute, so the rail pays
  immediately).
- **Tier 3, scaffold**: gen rules for table-driven satellites (doc rows,
  test skeletons as new files) work today; in-place arm insertion = the
  --move edit-sink pointed at "after the last sibling arm" (a span the CST
  rels locate). Grade 2 is real work; the near version is the Tier-2 diag
  carrying paste-ready text.

### 2. Navigation-derived knowledge (rides the flow-marks epic)

hook_event + the goto recorder turn EDITOR BEHAVIOR into a queryable rel.
Beyond flow recording: which files/syms does a debugging session orbit
(argmax over dwell), what does onboarding-Chris touch vs maintaining-Chris,
which jump chains repeat weekly (= missing abstraction or missing doc).
Anti-unify across sessions is the primitive throughout.

### 3. Review/PR intelligence

pr-diff.dl + type/call/df rev twins already diff graphs across revs. Mine:
blast-radius-of-this-PR as a hover/diag (touched syms -> run_reaches_pair
fan), "this PR adds a call into a deprecated/flow_sanitizer-modeled fn",
test-coverage anti-join (changed syms with no test file naming them),
CHANGELOG-drift (public decl moved, Unreleased silent).

### 4. Living architecture docs

gen + json aggs + doc_comment: generate the architecture doc FROM the graph
and diff it in CI (the doc-index drift-rail shape, scaled up). Module/crate
cards, BOM rollups per subsystem, "what changed structurally this month"
from rev twins. The deck/ presenterm pipeline is the render tier.

### 5. Cross-repo org queries at scale

~/orgs corpus + pin-skew/scip_want precedent. Mine: org-wide dead exports,
version-skew blast radius (who breaks if hub repo X changes signature Y),
convention drift across repos (same-name fn, diverging arity), the
manifest-seam family beyond go.mod.

### 6. Runtime/temporal joins

clock, every, @async, hook_event, ports: join code graphs against LIVE
streams (CI results via gh effect, test failures, deploy markers). "Which
recently-flaky tests touch syms changed in the last 3 revs" is one rule if
the streams land as rels. The mcp server surface means any agent can ask.

### 7. Agent-facing surfaces (dl as the code-intel API for LLM sessions)

dl what/summary/q verbs + MCP tools + hover_note as an agent WRITE channel
(an agent annotates spans with findings; the human sees them on hover).
Mine which query shapes agents actually need by instrumenting this very
session pattern (query_log is already a rel).

## Deliverables of the mining session

1. `docs/possibilities.md` (or chat_log + ledger entries per repo
   convention): the ranked list with verdicts and evidence links.
2. Each KILLER/USEFUL verdict either lands as an example .dl in-tree or
   becomes a plan file with staffed briefs.
3. At least one Tier-2 rail shipped end to end during the session (RPC
   registration completeness is the pre-picked candidate) so the session
   produces a working artifact, not only a survey.
4. Explicit graveyard section: hunches tested and killed, with the query
   that killed them (prevents re-pitching; precedent: shape-iso detectors).

## Guardrails

- Proving-query discipline above; no unmeasured claims in the ranked list.
- --no-daemon for ad-hoc runs (daemon hijack precedent).
- Engine gaps get NAMED and ledgered, not built mid-mining (unless S-sized
  and blocking a proving query, then a Sonnet brief).
- Repo style laws apply to every .dl written: descriptive vars, no
  source+derived mixed heads, recompute guards on global ops.

## Context to load in that session

- This plan + plans/2026-07-10-flow-marks-goto-recorder.md (anti-unify
  idiom) + plans/2026-07-10-lsp-thin-client-daemon.md (RPC list for the
  rail) + plans/2026-07-10-vscode-ext-review.md (panel surfaces).
- README.md (DSL surface), docs/reference/relations.md (the rel inventory
  the queries draw from), examples/ (the idiom library).
- CLAUDE.md ledger for what already exists (avoid re-mining shipped ground).
