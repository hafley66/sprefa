# Codex queue after the engine split: agent-feedback fixes

Source: debrief sections from every 2026-07-11 agent (semi-naive, sym-emit,
file_lines, plans-index, cross-harness) plus standing ledger pains. Runs via
/codex-delegate (workspace-write sandbox, dedicated worktree, one item = one
commit unless marked). Model routing per the ledger rule: luna(medium) weak /
terra(medium) opus-class / sol(high) fable-class.

Every item ships with a red/green test. No item may touch rebuild_derived
semantics or the sym lowering without an exact-count / byte-compare gate.

## Batch 1 — luna(medium), mechanical, one worktree, one commit each

1. **Plain-string backslash warn** (plans-index debrief, 3rd sighting):
   lex.rs:159 silently drops `\` in `"..."`. Emit a parse-time warning when a
   plain string literal contains `\s|\d|\w|\b|\n`-style escapes ("use r\"...\"
   for regex text"). Test: `--parse-only` fixture.
   <!-- todo(bug): lex warn on dropped backslash escapes in plain strings -->
2. **dl query output contract doc** (scorer debrief): document the 2-space
   data-row indent + tab separation where query output is described (docs
   reference + book ch on queries); add a doc-note to `run_query`.
3. **call_target header note** (scorer debrief): std/flow.dl call_target doc
   comment states name-equality pinning (aliases excluded) + df dependency.
4. **call_def.sym doc fix** (scorer debrief): RelDecl doc says bare
   `file::kind::name`; emitted value is repo-qualified. Fix the doc string,
   regen README/reference via the generator.
5. **filesize allowlist drift rail** (file_lines debrief): scripts/
   filesize-allow.txt vs .dl/file-size.dl `big_file_ok` facts can drift
   silently. Add the check to scripts/filesize-rail.sh (diff the path sets,
   warn loud). Test: red/green in a scratch copy.
6. WONTFIX (audit 2026-07-11: codex verified cargo already surfaces "Blocking waiting for file lock" through verify.sh's tee; the skip note lived only in the codex transcript — recorded here now). **verify.sh lock echo** (semi-naive debrief): when cargo blocks on the
   target-dir flock, echo "waiting on target-dir lock". (cargo prints
   "Blocking waiting for file lock" already when attached to a tty — ensure
   verify.sh doesn't swallow it; one-line fix or wontfix with note.)

## Batch 2 — terra(medium), engine-adjacent, sequenced AFTER the split merges

7. **Reserved-name check at parse tier** (semi-naive debrief): `rel node(...)`
   dies only at tick time; `--parse-only` should catch reserved builtin
   collisions. The decl catalogs now live in engine/decls.rs — expose the
   reserved set to typecheck. Test: `--parse-only` exit 2 + named fix text.
   <!-- todo(feature): reserved-name collision at --parse-only tier -->
8. **S6 body-level source+derived mix bail** (ledger, cost two agents a
   failing-test loop): a scan/match rule whose body also joins a rel silently
   ignores the rel atom. Bail loudly like the rel-level guard.
   <!-- todo(bug): body-level extract+rel-atom mix must bail, not ignore -->
9. **rail-contract fixture** (semi-naive debrief): perf_woes.rs asserts
   .dl/perf-woes.dl severities by hand; a severity edit breaks Rust tests
   invisibly. One shared fixture (expected code/severity pairs) read by both.
10. **generic cell readers** (sym-emit debrief): assert every generic edge/id
    reader uses `cell_as_string`, not `row.get::<_, String>` (silent row drop
    on INTEGER). Grep-rail or unit test over the reader helpers.
11. **`dl --check --max-wall <secs>`** (never-again ledger guard 3): loud
    partial report, exit 0 with `check-timed-out` warning diag. Required
    before hooks re-enable.
    <!-- todo(feature): --max-wall self-deadline before hook re-enable -->

## Batch 3 — sol(high), design judgment, LAST and one at a time

12. **Ambient-config hermeticity** (friction inventory #1, 4+ sightings):
    ad-hoc `dl` runs silently ingest ~/.config/sprefa repos; daemon hijack has
    no visible signal. Design: loud one-line banner naming config source +
    served-root writes, `--hermetic` alias for the env-var pile. Needs a
    decision section before code.
13. **S3/S4 string ergonomics** (batch-2 feedback): body-level bind for pure
    fns (`x = replace(...)`) and/or `concat()`/`+` on text. Parser+lower
    change; propose grammar first, get sign-off, then implement.
14. **setup.rs split** (cross-harness debrief): 928 lines; extract
    setup/hooks.rs per the file-size law (pure move, engine-split pattern).

## Standing rules for every batch

Worktree per batch; brief points at THIS file; pure test-verified commits
(`git commit -n`); full it suite max 2 runs per batch; hermetic dl runs;
never touch ~/.local/state/sprefa or daemons; file-size law; final summary
with per-item commits + skips.

## Post-queue additions (2026-07-11 late, factoring + instrumentation debriefs)

15. **Module-graph nondeterminism** (factoring agent, EVIDENCE-BACKED, own
    arc, sol-class): same binary + same 3-repo ambient corpus, back-to-back
    cold runs produce different rel_module_edge content (213 rows both, text
    sum 5999 vs 5964; mutual pairs 38 vs 108, cascading into bom clusters).
    Suspect: parallel per-file resolver extraction feeding the repo-less
    module_edge union. Evidence dbs were /tmp/dl_rm3 vs /tmp/dl_rm4.
    <!-- todo(bug): module_edge nondeterministic across identical cold runs -->
16. **Semi-naive divergence bail** (factoring wedge, terra-class): the 100k
    iteration cap exists but a growing delta makes each iteration slower long
    before it trips (15-min wedge at ~43k statements). Add a delta-growth /
    total-row-budget bail naming the rel, plus consider a began-statement
    marker so a wedged statement is visible (DL_STMT_TRACE is the stopgap).
    <!-- todo(perf): semi-naive delta-growth bail + wedge visibility -->

## Chris-approved wart fixes (2026-07-11 final hamon)

18. **Auto-split the rule-splitting family** (sol-class, engine): when a rel
    mixes source+derived heads, or a source rule body joins a non-slot rel,
    or a term-extract feeds @next, the engine should SYNTHESIZE the
    intermediate rel + union it currently tells the user to write (keep the
    bail as a lint tier for those who want explicitness). Design first: the
    synthesized rel needs a stable name (diagnostics reference it) and must
    not collide with user names.
    <!-- todo(feature): auto-split mixed/source-join/term-extract shapes instead of bailing -->
19. **closure() readable unpinned in rule bodies** (terra): today a closure
    rel in a rule body must be pinned or it refuses (materialization guard).
    Fix shape: when a rule body reads a closure unpinned, lower that RULE
    through the semi-naive delta machinery over the closure's edge rel
    instead of materializing the view (the flow-ctor hand-pattern, generated).
    <!-- todo(feature): unpinned closure in rule bodies via generated recursive rule -->
20. **argmax/argmin aggregates** (terra): first-class `argmax(col, by)` head
    aggregate replacing the negation-count idiom hand-built in chat-marks,
    suppress, goto-flows, gh-cache. Must define tie-break (stable: lowest
    rowid of the max group) and compose with key()/merge lattice decls.
    <!-- todo(feature): argmax/argmin head aggregates -->
21. **CLI flag taxonomy redesign** (sol PROPOSAL first, no code): the flag
    surface (--load/--load-once/--check/--fix/--await-settle/--rows/--query
    /--parse-only/--no-daemon/--hermetic-pending...) grew by accretion.
    Chris's sketch: gate by effect class — read-only query mode / mutate mode
    / effects mode — so one axis says what the run may DO and the rest is
    input selection. Deliverable: plans/ design doc mapping every current
    flag onto the new axes, back-compat aliases, sign-off checklist.
    <!-- todo(decision): CLI flag taxonomy — query/mutate/effects axes -->
