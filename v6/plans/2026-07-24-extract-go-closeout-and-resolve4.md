# sprefa-extract: Go closeout → Resolve<\*> (commit 4) → fixture broadening

Execution plan for **delegated subagents**. The orchestrator audits; the human
approves pushes and the 4a design. Executors: treat this file + the spec docs
as the whole truth. If anything here conflicts with the code or the spec,
STOP and report — do not improvise.

## Context

All work happens in ONE place:

- Worktree: `/Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan`
- Branch: `plan/extract-golden-plan` · Base SHA: `74a12940`
- Crate: `v6/sprefa-extract` (its own isolated workspace; all cargo commands
  run with cwd `v6/sprefa-extract` unless stated)

Landed so far: commits 1–3c (TS families + Epic U uniform surface), Tier-2
parity gold (TS + const facet), the Rust second language (`de94cceb`…`d28f43f8`
+ `f501e5bd`), and the Go third language (A–E: `8abdc38e` skeleton, `aa3c782e`
TypeF, `aab204d1` CallF, `d6427eab` DfF, `16bc0855` parity gold) merged as
`74a12940`. Four golden cases green, zero divergence.

The spec (do NOT re-derive decisions from v5; read these first, every brief):
- `v6/plans/2026-07-23-sprefa-extract-golden-plan.md` — the arc plan
- `v6/sprefa-seed/src/_3_extract/_7_tasks.rs` — the canonical ledger/BUILD STATUS
- `v6/sprefa-seed/src/_3_extract/_2_traits.rs` — the trait model (ProjectCx at
  lines 35–51; `Resolve<F>` signature + which families resolve at lines 80–97)

Interrupted state (what the previous session died inside of): three files carry
uncommitted, verified bookkeeping edits — `v6/sprefa-extract/src/lib.rs`
(GoSource re-export), `v6/sprefa-extract/src/types.rs` (status matrix Go column
`[x]`, const `[-] n/a`), `v6/sprefa-seed/src/_3_extract/_7_tasks.rs` (BUILD
STATUS header updated for GO). The `(go)` ledger entry was never inserted; its
frozen text is in Phase 0 below.

Code-cleanliness judgment (receipts-based, not a re-review): the arc stayed
clean and consistent. Evidence: zero-divergence parity vs the captured v5 oracle
on all 4 golden cases; the Go merge (`74a12940`) added one `lang/go.rs` + roster
line with no structural change to dispatch/seams/types; the single waiver (rust
closure df-node name) is self-verifying by test; dependency hygiene held
(tree-sitter unified with ast-grep transitives, only the pre-existing hashbrown
dup; banned-dep grep empty); Epic U's byte-identical-snapshots condition held
through two language ports. Residual risk: no fresh-eyes read of `go.rs` itself
— Phase 0.5 closes that.

## Decisions

- **V5 IS CORRECT (2026-07-24, user ruling; supersedes anything conflicting
  below).** Wherever v6's behavior diverges from v5 on a facet v5 emits, v6
  changes to match v5. Divergences are not codified; waivers are scaffolding
  to eliminate, not ratifications. Consequences:
  (a) the closure df-node name waiver is queued for ELIMINATION — derive v5's
  closure names (`lam_sym` shape, e.g. `...::closure::<byte>`) inside v6's
  ts.rs/rust.rs df walkers from span data (no sym machinery), then delete the
  waiver machinery — sequenced AFTER the lambda increment (same files);
  (b) the deferred v5 facets (type_edge, df aux, docs) are all ports-to-do on
  this arc, none optional — docs loses its "no ratchet, human decides" status;
  (c) cross-lang v6 inconsistencies resolve PER-LANG toward v5 (v5's own
  per-lang behavior is the target), not toward v6-internal unification.
  **AMENDMENT (2026-07-24, user): v5 oddities may now be questioned + fixed —
  the port discipline is proven.** Two-lane rule: PORTED rows stay v5-byte-
  exact (the ratchet is the asset); FIXES arrive either as v6-only ADDITIVE
  rows (free — the harness only reports v6-only, never asserts) or as
  explicit adjudicated breaking changes (the harness then shows exactly what
  moved). Oddity candidates found during the CLI spot-check:
  1. df ret self-edges (`809→809`) — artifact of expr-body == ret span
     identity; candidate: suppress self-edges or merge the nodes (BREAKING
     the df_edge stream; adjudicate).
  2. rust `if/match/loop/block` vs ts `cond/logic` — same concepts, two
     slugs; DfNodeKind(23) is the union of v5's per-lang vocab. Candidate:
     unify slugs as ADDITIVE normalized kind, or rename (BREAKING;
     adjudicate).
  3. missing closure-capture edges — NOT an oddity, deliberate scope: v5 df
     is intra-procedural; the join is commit-4 Resolve work (span
     containment). Do not "fix" in df.
- Subagents execute one increment each; the orchestrator audits every increment
  before the next starts. Rejected: letting one agent run multiple phases
  (context drift is how the last session died mid-ledger).
- The `(go)` ledger entry text is FROZEN (recovered verbatim from the dead
  session). Insert exactly; do not wordsmith. Rejected: regenerating it (it
  would drift from the commit hashes the session cited).
- The untracked `chat_log/20260723.0/.1`, `20260724.0` files and the modified
  `chat_log/20260722.0` are OUT OF SCOPE for every commit here (the prior
  session deliberately excluded them).
- Push, `extract-go` worktree removal, and `port/go-extractor` branch deletion
  require explicit human approval — never an agent's call.
- Resolve order: TS first (scip-typescript), then rust-analyzer-scip / scip-go
  reuse the proven shape. `df`/`cst` NEVER resolve (spec, `_2_traits.rs`:80-84).
- The docs facet has no ratchet anywhere in the spec; Phase 2 measures it,
  does not port it. Rejected: bundling docs into commit 4 (spec defers it).
- Kimi-agent commits carry no `Co-Authored-By` trailer (the Claude trailer was
  that session's own convention).

## Conventions (binding on every increment)

- Gate: `cargo test --features cli` (cwd `v6/sprefa-extract`) — all green.
- Snapshots are committed `.snap` files diffed byte-for-byte. `UPDATE_SNAP=1`
  is FORBIDDEN unless the increment exists to change snapshots, and then it
  must be declared in the report.
- Dep rails: `cargo tree | grep -E 'tokio|sqlx|sea-orm|rusqlite|axum'` empty;
  `cargo tree -d` shows no NEW dupes (pre-existing dups allowed: `hashbrown`;
  `syn 2.0.119` vs `3.0.3` via serde_derive/thiserror-impl — predates even the
  Rust skeleton, verified at `de94cceb^`).
- Frozen seams: `shape.rs`, `family.rs`, `rows.rs`, `seams.rs`, `source.rs`,
  `wire.rs`, `dispatch.rs`, `lang/ts.rs`, `lang/astgrep.rs`, `bin/` are
  read-only EXCEPT where a phase's allowlist explicitly names them.
- Commit style: `v6/extract: <imperative summary>` + structured hyphen-bullet
  body. Commit each green increment. NEVER push, NEVER `git stash`.
- Shell: non-interactive forms only (`cp -f`, `mv -f`, `rm -f`).
- Formatting: a non-concern (repo AGENTS.md: "formatting churn is not a review
  concern") — spend zero effort on it. The ONE rail: never run `cargo fmt` in
  this worktree, because the installed nightly rustfmt disagrees with the
  build the merged code was formatted under and produces ~1000 lines of churn
  across the frozen seams (Phase 0 nearly committed this; recovered via
  `git checkout HEAD -- v6/sprefa-extract`).
- Ledger ritual on every landed increment: `_7_tasks.rs` BUILD STATUS date,
  entry inserted before the `PENDING:` line, `NEXT:` kept current.
- Oracle regen (only when a phase adds fixtures): from the WORKTREE ROOT,
  `cargo build --quiet --example v5_normalize && cargo run --quiet --example
  v5_normalize -- <fixture> > <name>.v5.jsonl`. The oracle is captured, never
  linked.

## Phase 0 — Go closeout (one agent, ~30 min)

Allowlist (edit): `v6/sprefa-seed/src/_3_extract/_7_tasks.rs` only.

1. Verify the three pending diffs match the description in Context
   (`git diff` — lib.rs one line, types.rs matrix block, _7_tasks.rs header).
   If they differ in any unexpected way: STOP, report.
2. Insert the following text VERBATIM into `_7_tasks.rs`, between the end of
   the `(cli)` entry (line 211) and the `PENDING:` line (line 212):

```rust
//!   (go)     THIRD LANGUAGE LANDED: `GoSource` (lang/go.rs, ~1120 lines), PREPENDED in the
//!             roster so .go routes to it. Mirror of TsSource/RustSource: cst via ast-grep
//!             (ast-grep's go grammar) + type/call/df via tree-sitter-go (`go_parse` ->
//!             `tree_sitter::Tree`, the "floor as the only tier" - no oxc/syn analog for go).
//!             Ports v5 `src/graph/typegraph/go.rs` (GoTypes): TypeF entities + arrow sigs,
//!             CallF (defs + sites), DfF (nodes + Direct edges). tree-sitter yields BYTE
//!             offsets directly (node.start_byte/end_byte) -> v6 Span with NO line/col bridge
//!             (simpler than the syn port). TIER-2 PARITY GREEN vs the v5 oracle (go arm in
//!             v5_normalize.rs): ZERO divergence (type/call line-exact, df byte-exact). v5 go
//!             emits NO const facet (walk_go_entities skips const_declaration); v6 matches
//!             (const_value=0 both sides). DEFERRED (same set as TS/Rust): type_edge
//!             (Resolve<TypeF> commit 4), docs, df aux. Commits 8abdc38e (skeleton) /
//!             aa3c782e (TypeF) / aab204d1 (CallF) / d6427eab (DfF) / 16bc0855 (parity gold).
//!             tree-sitter + tree-sitter-go unify with ast-grep's transitives (one copy each).
```

3. `cargo fmt`, then gate: `cargo test --features cli` green.
4. Commit exactly the three files (+ this plan file if the orchestrator asks)
   with message:

```
v6/extract: re-export GoSource from crate root + ledger entries

- lib.rs: pub use GoSource (mirrors f501e5bd for RustSource)
- types.rs: status matrix Go column [x]; const facet [-] n/a (v5 go emits none)
- _7_tasks.rs: BUILD STATUS header for GO; (go) ledger entry
```

Done condition: `git status` shows only the three untracked chat_log files;
gate green; ledger reads header → (go) → PENDING with no seam.

<!-- todo(docs): commit the go-closeout bookkeeping (lib.rs export, types.rs matrix, _7_tasks.rs header + (go) entry) — text frozen in v6/plans/2026-07-24-extract-go-closeout-and-resolve4.md -->

## Phase 0.5 — consistency sweep (one agent, read-only, report-only)

Judge whether `lang/go.rs` actually matches the ts.rs/rust.rs conventions.
Checks (all read-only):
- `lang/mod.rs` roster order: GoSource routes `.go` before AstgrepSource.
- Module-doc header shape of `go.rs` vs `ts.rs`/`rust.rs` (ports-cited,
  dropped-machinery list, span story).
- The rust closure waiver test still asserts every waived line is a
  `df_node closure` row.
- All 4 snapshots byte-identical with NO `UPDATE_SNAP`.
- `extract --schema` / `--help` mention go (bin is self-describing).
- Dep rails from Conventions.
Report findings as a list: OK / DRIFT (file:line). Fixes are NOT this phase —
each DRIFT becomes its own micro-increment assigned by the orchestrator.

## Phase 1 — Resolve<\*> (commit 4), four increments — **COMPLETE 2026-07-24**

All of 4a–4d landed and merged; see the State + recovery section at the end
of this file for the SHAs and the final gate. The text below is the frozen
historical decomposition.

### 4a — hollow Resolve surface (design freeze; HUMAN REVIEW GATE)

Allowlist: `seams.rs`, `source.rs`, `types.rs`, `family.rs`, `lib.rs`,
`_7_tasks.rs`. (This phase — and only this phase — may touch the frozen
seam files, because the seam IS the deliverable.)

Deliverable: the phase-2 surface exactly per spec — `Resolve<F>` with
`resolve(&ExtractOutput, &ProjectCx) -> Vec<ProjectEdge>` extending `Source`;
`ProjectCx` per `_2_traits.rs`:35-51; cache key `(BlobHash, ProjectDigest,
FamilyMask)`; resolves for call/type/module only. Bodies `todo!()`. Also
answer, in the ledger entry: how phase-2 edges surface on the wire
(`FlatFact` extension vs side channel) and whether snapshots grow type_edge
lines. NO behavior, no call-site changes. STOP for human review before 4b.

### 4b — Resolve<TypeF> for TsSource

Allowlist: `lang/ts.rs`, `tests/golden_parity.rs`, `tests/fixtures/**`,
`wire.rs` (only if 4a's wire answer requires it).
Emit `TypeEdgeKind{Field, Variant, Impl, Generic, Param, Returns, Uses}`
(types.rs:194-202). Measurement harness already exists: the golden_parity
deferred `type_edge` facet flips from reported to asserted per fixture.
`UPDATE_SNAP=1` allowed ONLY if 4a declared snapshot growth.

### 4c — ScipSource seam + Resolve<CallF> (TS)

Allowlist: new `lang/scip.rs` (or per 4a), `seams.rs` registration,
`lang/ts.rs`, fixtures. Implement the spec's `ScipSource` (build = subprocess,
load = protobuf parse) for scip-typescript; emit
`CallEdgeKind{NameResolve, ScipOverride}` (types.rs:316-323). The ratchet:
occurrence/resolution parity vs SCIP as ground truth — NOT a raw symbol diff
(`_7_tasks.rs` ORACLE entry, lines 215-219).

### 4d — rust + go resolve arms

Only after 4c's ratchet shape is proven. rust-analyzer-scip, scip-go. Same
allowlist pattern as 4c.

<!-- CLOSED 2026-07-24: commit 4 Resolve<*> COMPLETE (4a approved design, 4b ts, 4c scip ratchet, 4d rust+go) — see State + recovery below -->

## Phase 2 — broaden parity fixtures — **LANDED 2026-07-24** (lambda:
`56b729c6`; docs ×3: `c321935d`; merged `b93b576a` + `1a824f51`)

- Lambda fixture: mine v5 `tests/fixtures/callables/ts.ts` for lambda-heavy
  input; capture the v5 oracle; add a golden_parity `Case`; the deferred
  df/lambda rows become measured, not assumed.
- Docs fixture: input that exercises v5's `doc` facet (ts docs,
  `rust_docs_from`, `walk_go_docs`); oracle captured; reported-only (docs has
  no ratchet — human decides later whether to port).

<!-- CLOSED 2026-07-24: fixtures broadened (lambda + docs ×3, merged) -->

## Design audit findings (2026-07-24, read-only structural audit)

Verdict: CLEAN-WITH-DEBT. Coupling is one-directional (lang/* → seams/family/
rows/shape only; no ts↔rust↔go cross-imports; zero store/engine/db names in
code; bin is parser-blind — Epic U's condition holds; dispatch fully generic).
The per-language walker triplication is DELIBERATE (per-parser AST types);
the family dimension is type-level as claimed (a plain 6th family touches
~10 pinned sites, no new wire arm unless it brings a new aux shape).

Share-worthy accidental duplication (~140 lines). SEQUENCING RULING
(2026-07-24, user): ALL dedup is deferred to ONE sweep AFTER the Resolve
pass (4a–4d) fully lands — not interleaved with it, not even after 4a.
Rationale: the dedup items and the resolve arms live in the same files
(lang/*.rs, types.rs, wire.rs), and weaker agents are uncoordinated —
concurrent structural churn guarantees merge conflicts and moving seams
under in-flight resolve work. One coherent arc at a time; the dedup sweep
is its own arc with its own audit afterward.

1. cst masked-extract block, 4 byte-identical copies — ts.rs:1471-1480,
   rust.rs:1130-1139, go.rs:1075-1084, astgrep.rs:121-130. → one
   `project_cst(path, content, &mut Strings)` helper in astgrep.rs.
2. Push helpers (~11 fns, ~70 lines) triplicated — df_push/df_edge/push_entity/
   push_sig/push_def; the empty-name filter rule exists in 3 places
   (ts.rs:1177, rust.rs:1089, go.rs:1034). → generic
   `FamilyBundle<F>::push_named/push_edge` in types.rs; span conversion stays
   per-lang.
3. Go param-seeding ×2 intra-file — go.rs:541-560 vs 849-875. → one
   `go_seed_params` helper.
4. wire.rs per-family flatten loops ×4 (nodes ×3, edges ×2) + the inline
   `CstEdgeKind::Child => "child"` match (wire.rs:71-73). →
   `Family::kind_str` + `CstEdgeKind::as_str`, one generic node/edge pass.

Documented divergences, NOT debt (do not "fix"): ts df nodes carry full spans
vs rust/go zero-length anchors (parity keys on span.start; undocumented —
document, don't unify); rust/go sort+dedup sig refs, ts does not
(deterministic anyway); rust/go native front-ends bypass the Parser/Project
seams with free project_* fns where ts.rs implements them → 4a rules on
whether Resolve<F> impls mirror that or must implement the seams.

### 4a must-encodes (from the audit's Resolve-arm triplication preview)

Without these, 4b–4d WILL copy-paste resolution across three langs:

1. A lang-agnostic `DefIndex` (name → (blob, span, kind)[]), orchestrator-built
   ONCE per refresh from phase-1 CallF+TypeF bundles — never per-lang, never
   built by re-parsing `reader` bytes. Handed into `resolve` explicitly or via
   a pre-filled OnceLock.
2. Shared caller-binding + same-file lookup helpers (site span → innermost
   covering def span via sorted-span binary search; name → def within one
   bundle). Pure functions over FamilyBundle/DefIndex, zero AST, written once.
3. Phase-1 specifier rows need a home BEFORE 4b (aux on an existing family or
   revived ModuleF) — today NO lang collects import/use specifiers; without
   phase-1 rows, each resolve arm re-walks its own AST in phase 2 (the exact
   triplication the phase split exists to prevent).
4. One site-key discipline: rule whether `callee_path` is collected uniformly
   (today rust fills it, ts/go emit None), and state that method resolution is
   name-only + ScipOverride so 4b–4d don't each invent receiver typing.

These fold into 4a as an addendum (agent resume) BEFORE the human review gate.

### 4b-i candidate ruling (2026-07-24, user): OPTION (a) APPROVED

The 4b agent STOPPED (correctly): v5's `type_edge` is itself UNRESOLVED
text-target rows (`to` = free text, never node-joined — decls.rs:517), and
phase-1 carried candidates only for param/returns (TypeFAux.sigs). Ruling:
add unresolved type-edge-CANDIDATE rows to `TypeFAux` (owner span, to-name as
written, kind) collected by TypeProjector during the one parse — the
CallFAux.specifiers pattern. Sub-rulings: text dsts STAY text (no fake node
joins — the candidate row IS the parity target); same-file blob via DefIndex
span-join; sig-sourced param/returns filtered to Function-kind owners (v5
emits no method-sig type_edges). The genuinely RESOLVED span-to-blob edges
are a v6-only ADDITIVE layer (reported, never asserted). The 4a seam is
unchanged; phase 2 stays zero-AST. Delivery was ordered TDD-style for the
human: red parity run first (rows missing), then green.

### Diet-SCIP tier mapping (2026-07-24, user: "want all that purely here too")

v5's diet scenario = the Phase D diet-SCIP call graph
(`src/engine/family/mod.rs:405`): the static-parse call graph that works when
no scip index is available. In v6 this is NOT a mode — it is the Ast producer
alone, and the distinction is carried per-edge:

- DIET tier (always on, index or not): all phase-1 AST facts + every
  `CallEdgeKind::NameResolve` edge (static name-match via DefIndex).
- FULL tier (index present): `CallEdgeKind::ScipOverride` upgrades individual
  edges where scip disagrees with the name-match (user-allowed override).
- Index unavailable ⇒ structural degradation: no scip run → zero ScipOverride
  rows, all NameResolve rows still emitted. No special code path.
- The GENERAL ratchet — `ratchet(&[(Producer, ExtractOutput)]) -> Merged`,
  per-fact best-producer-wins over `Ast`/`Scip(&indexer)`/`Ghcacher`, the
  Producer tag riding the bundle — is EPIC 3 (seed _7_tasks.rs:56-57,
  688-691, 754, 784-785), sequenced after commit 4 + Epic 2 (RAM). The
  extract crate does not yet tag bundles with Producer — Epic 3 adds it.
- OPEN PIN: an explicit "scip unavailable ⇒ NameResolve-only output, no
  failure" test belongs at the tail of 4c or in Epic 3 — the degradation is
  structural today but unasserted. (Track under the 4c/Epic-3 increments.)

## Verification

The arc proves itself per increment: `cargo test --features cli` green;
snapshots byte-identical without `UPDATE_SNAP` (unless declared); golden_parity
cases green (4 today, growing in Phase 2); 4b's type_edge deferred-set flips to
asserted; 4c's ratchet report (occurrence/resolution parity vs scip) in the
ledger; ledger updated every increment; dep rails clean.

## Staffing

- Executors: Kimi coder subagents, ONE increment each, run in the existing
  worktree (no new worktrees; base SHA `74a12940` + subsequent closeout commit).
- Suite budget per increment: one full gate (~minutes) + oracle regen only
  where the phase says so.
- Orchestrator audits per the checklist below before starting the next
  increment; the human reviews the 4a design and any push/cleanup.

### Subagent brief template (orchestrator fills `[...]`)

```
Work in /Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan
(branch plan/extract-golden-plan). Do NOT create commits beyond the one this
task specifies; NEVER push; NEVER git stash; NEVER touch chat_log/.
Read first: v6/plans/2026-07-24-extract-go-closeout-and-resolve4.md (your phase
is [X]), then the spec docs it names. Task: [phase text verbatim].
Edit allowlist: [files]. Everything else is read-only.
Gate: cd v6/sprefa-extract && cargo test --features cli (must be green).
UPDATE_SNAP=1 is forbidden [unless declared].
Commit message: [exact message].
If anything is ambiguous, conflicts with the spec, or requires editing outside
the allowlist: STOP and report — do not improvise.
Report: files changed, gate output tail, commit SHA, divergences (must be zero),
open questions.
```

### Orchestrator audit checklist (per increment)

- `git show --stat` — only allowlisted files touched.
- Independent `cargo test --features cli` run — green.
- No `UPDATE_SNAP` in the diff history unless declared; `.snap` diffs
  byte-reviewed when present.
- Dep rails clean; no new dupes.
- Ledger: entry before `PENDING:`, BUILD STATUS date, `NEXT:` current.
- Report's claims match the actual diff.

## State + recovery (2026-07-24 — written so any session can pick this up)

READ FIRST on pickup: this file, then `v6/sprefa-seed/src/_3_extract/_7_tasks.rs`
(the ledger), then `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`. Do NOT
re-derive decisions.

### Landed (branch `plan/extract-golden-plan`, worktree `.claude/worktrees/extract-golden-plan`)

- `f9b8ce37` Go closeout (GoSource export + ledger) · `7c72249a` --help matrix fix
- `875258cf` + `652fc46a` 4a hollow `Resolve<F>` surface + must-encodes addendum
  (HUMAN-APPROVED: scip-override allowed; blake3 in)
- `b93b576a` + `1a824f51` fixture merges (docs ×3 measured; lambda fixture +
  TS lambda call_defs ported per v5)
- `e117bf2e` 4b-i partial (blake3, DefIndex helpers, ProjectEdge wire arm) ·
  `beb60e73` 4b-ii TS specifiers (CallFAux.specifiers, v6-only)
- `2f377fd2` 4b-iii type-edge candidates + `Resolve<TypeF>` ts → type_edge
  ASSERTED ts (user ruling: option (a) phase-1 candidates; text dsts stay text;
  Function-only sigs)
- `29c7977a` waiver-kill (lam_sym closure names ts+rust+go; waiver DELETED)
- `d9e98f27` 4c-i ScipSource seam (vendored proto/scip.proto + prost runtime;
  bindings committed at `src/scip/scip_proto.rs`; v5's scip+protobuf pairing
  rejected — measured dup violation)
- `6a948a7f` 4c-ii `Resolve<CallF>` ts + 6-leg scip ratchet (5 NameResolve,
  1 ScipOverride, 9 external-no-edge; 0 miss/disagree/overbound; missing scip
  = loud test failure)
- `efa11843` + `9f4760ec` 4d-go (type arm — v5 go DOES emit type_edge, new
  edges.go case 7 rows 0 divergence; call arm + scip-go ratchet 1/1/0 misses),
  merged `139d94e3` (+ `0a4a7ef2` INDEX regen)
- `6b5d80f6` + `d31bb93d` 4d-rust (type arm 3+3 rows 0 divergence; call arm +
  rust-analyzer-scip ratchet 2 NameResolve / 3 ScipOverride / 0 misses; leg-6
  local-symbol adaptation), merged `e579e5ec`
- RESOLVE PASS (commit 4) COMPLETE 2026-07-24. Final gate: golden_parity 8/8
  (ported facets, type_edge ts+go+rust, ledger, ratchets ts+go+rust — all
  three real indexers run in-test), snapshot 2/2, all resolve worktrees
  retired.

### In flight — RESOLVED 2026-07-24: both landed + merged. THE RESOLVE PASS IS COMPLETE.

- 4d-go landed: `efa11843` (type arm, go_edges 7 rows 0 divergence — v5 go DOES
  emit type_edge, fixtures just never exercised it) + `9f4760ec` (call arm +
  scip-go 0.2.7 ratchet: NameResolve 1 / ScipOverride 1 / external 1, zero
  misses). Merged as `139d94e3` (+ `0a4a7ef2` INDEX regen).
- 4d-rust landed: `6b5d80f6` (type arm, rust 3+3 asserted 0 divergence) +
  `d31bb93d` (call arm + rust-analyzer-scip ratchet: NameResolve 2 /
  ScipOverride 3 / external 2, zero misses; leg-6 rust local-symbol adaptation
  documented). Merged as `e579e5ec`.
- Final gate after both merges: `cargo test --features cli` — golden_parity
  8/8 (ported facets, type_edge ts+go+rust, ledger, ratchets ts+go+rust — all
  three REAL indexers run in-test), snapshot 2/2. Both worktrees retired.

### NEXT — queued 2026-07-25 (user: "write briefs so we can resume"): launch
order I0a → I0b → I1 → I2 → I2.5 (doc lane, seed S2) → I3 → I4a–d → I5 → I6 → I7, STRICTLY SEQUENTIAL
(shared files; parallel weak agents = merge collisions). Full briefs in
"Increment briefs" below. CLI-8 (project mode), I8, and ModuleF/specifiers
stay human-gated.

### Parked (do NOT start without the user's explicit word)

- df aux port (args/fields/lits/param_pos/loops/nests) — v5-is-correct port.
- docs facet port — v5-is-correct; docs now measured (ts 8, rust 5, go 6).
- Oddity-fix arc (two-lane: additive v6-only rows free; breaking adjudicated):
  df ret self-edges (`809→809`); rust `if/match` vs ts `cond/logic` slug split.
- Dedup sweep (~140 lines; audit items 1–4: cst block ×4 → `project_cst`;
  push helpers → `FamilyBundle::push_named/push_edge`; `go_seed_params`;
  wire flatten loops → `Family::kind_str` + `CstEdgeKind::as_str`) — own arc.
- `callee_path` ts fill — declared-snapshot micro-increment (4c deferred;
  filling changes `sample.callf.snap`).
- Diet-tier hard-assert of scip-less name_resolve count (currently exercised +
  counted, not asserted) — user's call.
- Epic 3: general ratchet `ratchet(&[(Producer, ExtractOutput)]) -> Merged` +
  Producer tag rides bundles (after commit 4 + Epic 2 RAM).
- Housekeeping (HUMAN-APPROVED only): push `plan/extract-golden-plan`; retire
  extract-go worktree + `port/go-extractor` + `exp/fixture-*` branches;
  chat_log files stay uncommitted (session convention).

### Operating machine (conventions for any agent/orchestrator)

- Supreme ruling: V5 IS CORRECT — divergences resolve toward v5; waivers get
  eliminated, never created. Two-lane: ported rows asserted byte-exact;
  v6-only rows reported, never asserted.
- Subagents do the work (one increment each, frozen file allowlist,
  stop-and-report on ambiguity); orchestrator audits (`git show --stat` vs
  allowlist + independent gate) and merges. Humans gate design + pushes.
- Hard rules: no push, no stash, NO `cargo fmt` (nightly rustfmt churns
  ~1000 lines; the merged code was formatted under a different build), no
  chat_log/ commits, THIS plan file stays untracked, non-interactive shell
  (`cp -f`/`mv -f`/`rm -f`), commit style `v6/extract: <imperative>` with no
  Co-Authored-By trailer, INDEX.md hook auto-stage allowed.
- `UPDATE_SNAP=1` forbidden unless the increment IS a declared snapshot
  change, with eyeballed diffs quoted in the report.
- Dep rails: `cargo tree | grep -E 'tokio|sqlx|sea-orm|rusqlite|axum'` empty;
  `cargo tree -d` no NEW dupes (pre-existing: hashbrown; syn 2.0.119/3.0.3).
- Gate: `cd v6/sprefa-extract && cargo test --features cli`. Oracle regen from
  the WORKTREE ROOT: `cargo run --quiet --example v5_normalize -- <fixture> >
  <name>.v5.jsonl` (captured, never linked).

## Next-arc seed: new-language traits + codegen (2026-07-24, user ask)

What the arc MEASURED about adding a language (ts+rust+go as the sample):
a port is ~15 junction sites split into two cost classes.

**Hand-written (NOT generatable — the "Kotlin-sized" unit):** the AST walkers
in `lang/<lang>.rs` (type/call/df projectors, edge-candidate collector,
lam_sym coords). Deliberately per-parser divergent (audit verdict; the repo
bar: three similar lines > premature abstraction). Never trait-ify these.

**Generatable/mechanical (the scavenger hunt to kill):** roster entry
(`lang/mod.rs`), `lib.rs` export, `scip.rs` indexer row, fixtures (sample +
oracle, the scip/ module trio + per-lang manifest: go.mod/Cargo.toml/
tsconfig.json), golden_parity Case + ledger match arms, the type_edge parity
test fn (currently copy-paste ×3), the ratchet test fn (copy-paste ×3,
~200 lines each), `--help` matrix line, `types.rs` status matrix, the
`v5_normalize.rs` oracle arm (root crate), ledger entries.

### Traits/generics to build (lands with the parked dedup sweep, in this order)

1. **Single copies of the lang-neutral helpers** (audit items 1–4 + the 4d
   triplications the agents flagged: `resolve_type_dst`, `call_name_match`,
   `scip_call_target`, `ScipGo::load` vs shared `load_index`).
2. **`ResolveLang` trait** — the per-lang resolve entry points
   (`type_edge_candidates`, `call_name_match`) currently inherent methods
   copy-pasted ×3. One small trait so the harness is generic; impls stay in
   `lang/<lang>.rs`.
3. **ScipSource impls → DATA** — one `IndexerSpec` row per lang (binary,
   argv, discovery/fallback, staging policy) mirroring v5's `INDEXERS` row
   (`src/scip_setup.rs:50`). `load` is already shared (`load_index`); go's
   staged-vs-not and rust's always-staged become two policy variants, not
   two code paths.
4. **Table-driven tests** — ONE type_edge parity body + ONE ratchet body,
   generic over a `LangDescriptor { dir, extensions, source, resolve_lang,
   scip: Option<IndexerSpec>, fixture notes }`; N descriptors. Epic-U
   precedent: "4 hand tests → ONE loop-driven test". The ratchet body
   duplication is the single biggest copy-paste in the crate today.
5. Explicitly NOT traits: per-family kind enums (enum-not-trait, spec), the
   walkers (above).

### The generator (after 1–4 stabilize the skeleton)

`xtask new-lang <name> <exts...>` (or a `.dl` script, house style) writing
every mechanical site above from templates, with `todo!()` walkers. Port v5's
honesty mechanism verbatim: every junction carries
`// LANG-JUNCTION(<slug>): <what a new language wires here>` and a
`gen-lang-junctions.dl --check` drift rail (pattern: `examples/gen-lang-skill.dl`
+ `sprf-add-language` skill). The generator writes the markers; the rail keeps
the map true forever after.

## Increment briefs (queued 2026-07-25 — launch in order, ONE agent each)

Launch each by pasting the Staffing §Subagent brief template with `[X]` =
the increment id and the fields below. Shared constants for ALL briefs (the
template's fixed lines): worktree
`/Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan`,
branch `plan/extract-golden-plan`; gate
`cd v6/sprefa-extract && cargo test --features cli`; oracle regen (only where
an increment declares it) from the WORKTREE ROOT
`cargo run --quiet --example v5_normalize -- <fixture> > <name>.v5.jsonl`;
one commit only, NEVER push/stash/chat_log; STOP-and-report on ambiguity or
out-of-allowlist edits; report = files changed + gate tail + commit SHA +
divergences (must be zero) + open questions. "Ledger" everywhere =
`v6/sprefa-seed/src/_3_extract/_7_tasks.rs` (entry before `PENDING:`, BUILD
STATUS date, `NEXT:` current).

PRECONDITION for I1–I3: the KOTLIN increment (in flight at queue time) is
merged. If it is not, drop `kotlin.rs` + kotlin fixtures from the allowlist
and ledger-note that kotlin's aux/docs rows ride the NEXT port increment.
Never rebase onto in-flight kotlin commits.

### I1 — df aux port (df_args / df_fields / df_lits / df_param_pos)

Grounding established at queue time (do not re-probe):

- The four row kinds ALREADY sit in the captured oracles — they are the
  assertion targets and are NEVER regenerated in this increment:
  ts/sample 8 args / 2 lits / 7 param_pos; go/sample 2 args / 1 fields /
  2 param_pos; rust/sample 2 / 1 / 4; ts/docs 4 / 2 / 5; ts/lambdas
  14 / 2 / 9; go/docs 2 / 1 / 2; rust/docs 2 / 1 / 4. Kotlin oracle: probe
  at launch (`cut -f1 kotlin/sample.v5.jsonl | sort | uniq -c`).
- v5 emission homes: `src/engine/extract/dataflow.rs` + the per-lang walkers
  (`src/graph/typegraph/ts/flow.rs`, `rust/mod.rs`, `go.rs`, `kotlin.rs`).
- `loops`/`nests` (named in the parked line) appear in NO captured oracle:
  do NOT fabricate rows; ledger-note the absence.

TASK: port v5's df aux emission for ts, go, rust, kotlin into the v6 Df
family (aux rows on the existing DfF projection, V5-IS-CORRECT byte-exact).
Move the four row kinds from DEFERRED to PORTED in golden_parity (zip
discipline, the type_edge precedent). FIRST determine whether the aux rows
reach flatten_jsonl: if yes this is a DECLARED SNAPSHOT CHANGE —
UPDATE_SNAP=1 permitted for the affected .snap files only, eyeballed diffs
quoted in the report; if no, snapshots stay frozen (prove byte-identical).

ALLOWLIST: `v6/sprefa-extract/src/lang/{ts,go,rust,kotlin}.rs`;
`v6/sprefa-extract/src/types.rs` (DfF aux fields only);
`v6/sprefa-extract/tests/golden_parity.rs`;
`v6/sprefa-extract/tests/snapshots/**` (only via the declared UPDATE_SNAP);
ledger.

COMMIT: `v6/extract: port df aux rows (args/fields/lits/param_pos) for ts+go+rust+kotlin`

STOPs: any captured aux row not byte-reproducible (no waivers — report the
row); finding wire.rs/dispatch.rs/snapshot.rs in your edit set; any urge to
"also fix" the df ret self-edge or the slug split (that is I3, not here).

### I2 — docs facet port (`doc` rows)

Grounding established at queue time:

- `doc` × 19 captured: ts/docs 8, go/docs 6, rust/docs 5. ZERO `doc` rows in
  the sample/lambdas oracles — assertions live in the docs fixtures only.
- v5 side: the `doc_comment`/`doc_tag` "built-in doc relation" emitted by
  the per-lang typegraph extractors (split note at
  `src/rels/extract_family.rs:28`). `src/ingest/mod.rs` `IngestLang` is the
  MARKDOWN-document lane — NOT this one; do not port MarkdownDoc.

TASK / ALLOWLIST / SNAP-ruling / STOPs: same shape as I1, for the single row
kind `doc`.

COMMIT: `v6/extract: port doc rows for ts+go+rust+kotlin`

### I3 — oddity fix, ADDITIVE LANE ONLY (evidence-first)

Scope: df ret self-edges (`809→809`). FIRST lay evidence in the report: v5
oracle df_edge rows vs v6 emission on every fixture. Two-lane ruling: v5
emits & v6 drops → port it (asserted); v6 emits & v5 doesn't → v6-only row
(keep + report, never assert) or remove — recommend with evidence, implement
only the v5-is-correct direction.

The slug split (rust `if/match` vs ts `cond/logic`) is the BREAKING lane:
NOT in this increment, awaits user adjudication; touching it = STOP.

ALLOWLIST: `v6/sprefa-extract/src/lang/*.rs`;
`v6/sprefa-extract/tests/golden_parity.rs`; snapshots per the I1 ruling;
ledger.

COMMIT: `v6/extract: reconcile df ret self-edges with v5 (additive lane)`

### I4 — dedup sweep → traits (FOUR increments, strict order; ALL pure
refactors: snapshots byte-identical, zero behavior change, gate green)

- I4a single-copy helpers: audit items 1–4 (cst block ×4 → `project_cst`;
  push helpers → `FamilyBundle::push_named`/`push_edge`; `go_seed_params`;
  wire flatten loops → `Family::kind_str` + `CstEdgeKind::as_str`) + the 4d
  triplications (`resolve_type_dst` ×3, `call_name_match` ×3,
  `scip_call_target` ×3, `ScipGo::load` → shared `load_index`). Expect
  ~140+ lines net removal.
  COMMIT: `v6/extract: dedup lang-neutral helpers (audit items 1-4 + 4d triplications)`
- I4b `ResolveLang` trait (seed §2): trait-ify `type_edge_candidates` +
  `call_name_match`; impls stay in `lang/<lang>.rs`; the harness goes
  generic.
  COMMIT: `v6/extract: ResolveLang trait over the per-lang resolve entry points`
- I4c `IndexerSpec` as data (seed §3): one row per lang (binary, argv,
  discovery/fallback, staging policy) mirroring v5 `src/scip_setup.rs:50`
  INDEXERS; staged-vs-not and always-staged become policy variants, not
  code paths.
  COMMIT: `v6/extract: ScipSource impls to data-driven IndexerSpec rows`
- I4d table-driven tests (seed §4): ONE type_edge parity body + ONE ratchet
  body, generic over `LangDescriptor` (Epic-U precedent). Test COUNT may
  drop (~400 dup lines); assertions must be identical.
  COMMIT: `v6/extract: table-driven parity + ratchet tests over LangDescriptor`

STOPs (all of I4): any snapshot or oracle drift; any behavior change
"while here"; scope creep into the walkers (deliberately per-parser
divergent, NEVER trait-ified — seed §Hand-written).

### I5 — micro-batch (ONE agent session, TWO commits)

- 5a `callee_path` ts fill (4c-deferred): DECLARED SNAPSHOT CHANGE — the ts
  callf snaps grow; eyeballed diffs quoted in the report.
  COMMIT: `v6/extract: fill callee_path for ts call sites`
- 5b diet-tier hard-assert of the scip-less name_resolve count —
  CONDITIONAL: launch only on the user's explicit opt-in (currently
  exercised + counted, not asserted).
  COMMIT: `v6/extract: hard-assert diet-tier name_resolve count`

### I6 — codegen (launch only after I4a–d merged)

`xtask new-lang <name> <exts...>` writing every mechanical site from the
seed's Generatable list (roster entry, lib.rs export, scip.rs indexer row,
fixtures + per-lang manifest, golden_parity case + ledger match arms,
`--help` matrix line, types.rs status matrix, the v5_normalize oracle arm in
the ROOT crate) with `todo!()` walkers. Port v5's honesty mechanism
verbatim: `// LANG-JUNCTION(<slug>): <what a new language wires here>`
markers + a `gen-lang-junctions --check` drift rail (pattern:
`examples/gen-lang-skill.dl` + the `sprf-add-language` skill). The generator
writes the markers; the rail keeps the map true.

COMMIT: `v6/extract: xtask new-lang generator + LANG-JUNCTION drift rail`

### I7 — python port (first consumer of I6; proves the generator)

v5 semantics: `src/graph/typegraph/python.rs`. Front-end: tree-sitter floor
per the go/kotlin precedent (proposing rustpython-parser instead = STOP for
adjudication). Fixture + captured oracle via a new `.py` arm on the root
`examples/v5_normalize.rs`. IN SCOPE: phase-1 families + parity. Resolve
arms + scip ratchet: orchestrator's call at launch time (the kotlin ruling
deferred them; post-I4d the table-driven harness makes them cheap — decide
then).

COMMIT: `v6/extract: python port on the generated skeleton (phase-1 + parity)`

### I8 — Epic 3 general ratchet — STUB ONLY

Needs its own design pass (Producer tag rides bundles;
`ratchet(&[(Producer, ExtractOutput)]) -> Merged`). Do NOT launch an agent
from this section.

### Decision block — ModuleF/specifiers (HUMAN GATE; no increment until ruled)

4a addendum (3) flagged `CallFAux.specifiers`
(`Specifier{span, name: NameId, kind: SpecifierKind}`; vocabulary = the
seed's BindingKind Named/Default/Namespace/SideEffect/Reexport) as the
module-binding home — hollow row shape only, ZERO lang emission today
(verified by grep at 4a). v5's fuller Binding side table
(`_1_mask.rs`:67-76; local/source/imported) is the evolution path; the TS
from-module question is the open sub-question. If ruled IN: the increment is
"collect specifiers in all four langs + wire the from-module side data" and
its assertion strategy is part of the ruling (NO captured oracle rows exist
for specifiers). If ruled OUT: the DefIndex remains the whole cross-file
story and the shape stays hollow forever.

## Dogfood findings (2026-07-25 — CLI used as a ts/rs/kotlin user, break-in-spirit battery)

Battery: real repo files (extension.ts 763ln, derive.rs 2751ln), routing edge
cases, encoding/syntax stress, modern-syntax files, 2MB scaling, mask timing.
ZERO crashes, ZERO panics, error-tolerance confirmed (broken syntax, empty
file, BOM, CRLF all produce sane partial output). Scaling linear (16KB min.js
33ms; 2MB 3.8s debug). `--family` masks AT EXTRACT (df-only on derive.rs
halves time, cst=0). `.kts` precedence over `.ts` correct. `.d.ts` parity
with v5 verified (v5 also mints no entities for `declare module`; both emit
exactly one let_bind). FALSE ALARM recorded so nobody re-chases it: `--bench`
prints NODE counts only — sites/edges/sigs are separate records invisible in
the summary; `call=0` means "no call defs", NOT "no call sites". The kotlin
"kts top-level site suppression" suspicion was this misreading; script.kts
matches v5 exactly (listOf/map/println sites all present).

### CLI-1..8 (user-visible, ordered by cost/benefit; micros can batch into ONE agent session)

- CLI-1 (micro, DO FIRST — it misled the dogfooder): `--bench` reports only
  per-family NODE counts. Print per-RECORD totals (nodes/edges/sites/sigs/
  consts) so the summary matches what the stream carries.
- CLI-2 (micro): non-UTF8 input → silent zero rows exit 0, but `--help`
  promises exit 1 for non-UTF-8. Pick one: enforce exit 1 (preferred — a
  silent empty graph is a lie) or fix the doc.
- CLI-3 (micro): `--family const` is advertised in the coverage table but
  parse_mask rejects it silently; unknown names (`--family tyep`) also
  silently yield zero rows. Accept `const` (alias into the types mask) and
  eprintln-warn on unknown names.
- CLI-4 (micro, ride with I5a): `--schema` says callee_path is "filled by
  resolution" — wrong; phase-1 fills it for rust (multi-segment), I5a fills
  ts. Fix the sentence when I5a touches the same surface.
- CLI-5 (micro): io errors print a raw `Os { code: 2, ... }` Debug dump.
  Wrap with `extract: <path>: <kind>` context (also covers EISDIR).
- CLI-6 (small increment): stdin input (`extract -` or piped) + an explicit
  `--ext <lang>` override so pipe workflows can name the language. Piping
  proof exists for stdout; the input side is missing.
- CLI-7 (micro): case-insensitive extension routing (`FOO.TS`, `X.Rs`) —
  real on macOS/Windows checkouts.
- CLI-8 (BIG, human-gated design): project mode — multi-file/dir/glob input
  over the seed's BlobSource seam, prerequisite for streaming RESOLVE rows
  (type_edge / caller→callee) on the CLI, which 4b already declared "its own
  increment". As a user this is the most-wanted capability: it turns the
  tool from a fact faucet into a navigation tool. Do NOT start without a
  design pass (wire shape, snapshot growth, parity implications).

### Survival log (no action needed)

modern.ts (decorators, satisfies, template-literal types, bigint, generators,
private fields, accessor) and modern.rs (async trait fns, const generics,
impl Trait returns, match guards, raw strings, macro_rules!) both extracted
clean. Macros (`println!`) mint no call sites — consistent with v5's model.

### I0a — CLI micros batch (ONE agent session, ONE commit; launch before I1)

All from the dogfood findings (§Dogfood CLI-1..5,7). The bin is clap 4
derive (`src/bin/extract.rs`); keep it thin — logic stays in the lib.

- CLI-1 bench per-record totals: `bench()` (bin/extract.rs:123-146) prints
  only `b.nodes.len()` per family — sites/edges/sigs/consts are invisible
  (this misled the dogfooder into a phantom bug hunt). Derive the summary
  from the ALREADY-flattened `facts` Vec instead: tally by (family, record
  tag) so sites/sigs/consts/edges all appear. Zero extraction changes.
  CHECK FIRST: grep tests/ for any bench-output assertion (expected none —
  bench is stderr human output; if one exists, STOP).
- CLI-2 non-UTF8 exit 1: today a latin-1 file in a CLAIMED language routes
  to its Source, UTF-8 failure collapses to `None`, and the CLI prints zero
  rows exit 0 — while --help promises exit 1. Fix at the CLI: guard
  `std::str::from_utf8(&content)` in main() before dispatch; on Err,
  eprintln + exit 1. Uniform across langs; NO lib change.
- CLI-3 `--family const` + warn-on-unknown: `parse_mask`
  (bin/extract.rs:100-112) — accept `const` as an alias INTO the types mask
  (const rows ride the TypeF aux; agent verifies on a ts const fixture that
  `--family type` surfaces them today) and eprintln-warn on ANY unknown
  family name (typo = loud, not silent zero rows).
- CLI-4 schema doc: the SCHEMA const's callee_path line says "filled by
  resolution" — wrong; phase-1 fills it for rust multi-segment paths (ts
  fill is I5a). Reword: "the full qualified path as written when >1 segment
  (filled at phase 1 where the lang collects it; else null)". Folded into
  THIS batch; I5a's scope is now only the ts fill + snaps.
- CLI-5 io error context: `std::fs::read(&path)?` (bin/extract.rs:89)
  dumps a raw `Os { code: 2, ... }` Debug. map_err to
  `extract: <path>: <io error>`; exit stays 1. Covers EISDIR too.
- CLI-7 case-insensitive extension routing: lowercase the extension before
  the FIRST-MATCH roster walk (find `source_for`; keep roster ORDER — the
  `.kts`-before-`.ts` precedence is load-bearing, kotlin report §ast-grep
  routing). `FOO.TS`/`X.Rs` must route; behavior for lowercase unchanged.

ALLOWLIST: `v6/sprefa-extract/src/bin/extract.rs`; the ONE file home of
`source_for` (find it; read-only otherwise);
`v6/sprefa-extract/tests/snapshot.rs` (ADDITIVE assertions to
`roster_routes_by_extension` only — snapshots byte-identical);
`v6/sprefa-extract/tests/cli.rs` (NEW: std::process::Command +
`env!("CARGO_BIN_EXE_extract")`, NO new deps — non-UTF8 exit 1, unknown
--family warns, missing-file message names the path); ledger.

GATE: standard; UPDATE_SNAP FORBIDDEN; zero .snap diffs; dep rails (no new
deps — tests/cli.rs is std-only).

COMMIT: `v6/extract: CLI micros (bench record totals, UTF-8 exit 1, family const alias + warn, io error context, case-insensitive routing, schema doc)`

STOPs: any urge to touch dispatch/wire/snapshot.rs or a lang file; finding
bench output asserted anywhere; `const` rows NOT visible under
`--family type` (means const does not ride TypeF — report, do not invent a
new mask bit).

### I0b — stdin + --ext (ONE agent; launch after I0a merges)

`extract -` (or piped stdin) reads stdin to bytes; `--ext <lang>` is
REQUIRED with stdin (language cannot be inferred) and routed via a
synthetic name (`stdin.<ext>`) so the ONE data-driven path
(dispatch -> flatten -> stdout) and the roster's first-match order are
untouched. `--ext` without stdin: reject (clap arg relation) — overriding
file routing is a separate question, not this increment. Unknown `--ext`:
error listing valid extensions. Errors: stdin without --ext = exit 2
(usage). tests/cli.rs grows: piped ts produces sites, missing --ext fails
loud. Update --help LONG_ABOUT coverage block + PATH_LONG (stdin mode) —
the self-describing contract stays true.

ALLOWLIST: `v6/sprefa-extract/src/bin/extract.rs`;
`v6/sprefa-extract/tests/cli.rs`; ledger.

COMMIT: `v6/extract: stdin input with --ext language override`

STOPs: touching the lib (dispatch/roster/lib.rs); adding deps; buffering
concerns (stdin read-to-end is fine — single file by design).

## Dogfood analysis #2 (2026-07-25 — v6/dl + v6/sprefa-store measured with the CLI)

38 src files + consumer trees extracted (208k facts, name-joined offline).
Headlines: (1) dl code-depends on store-js via direct ESM imports
(0_ast_bridge -> lower/rulegraph; 3_runtime -> engine cascade/lib/lowerSql) —
no FFI anywhere; store-js<->store-rs is a MIRROR-BY-CONVENTION (43 shared
callable + 59 shared type names; tasks/lib/spine/algo paired; measure.rs
rust-only harness). (2) Feature envy: GENUINELY ZERO signals (classes +
interfaces, own-vs-foreign member-name heuristic) — the TS is
functional-module; OOP mass sits in 5 god-classes (DatalogEvaluator 175 defs,
Store 152, Tasks 99, HostRunner 71, DlRuntime 29). (3) Useless separation,
grep-verified: the Tasks write/nav surface (upsert_node/upsert_edges/
children/parents/mint/create + rs thread_namespace/two_stores_independent/
per_tuple_unlock_evidence) has ZERO textual call sites in BOTH languages
incl. tests/examples/seed — dead surface kept in mirror-sync twice. Also dead
both langs: spine table_names + int codecs; measure.rs record_*/on_* family.
dl-only dead: negation path (slotToNegArg; toNegArg single-caller),
support-retract trio (retractThroughSupport/supportCoverageGaps/sqlTuple),
shHost. dl layering: ONE numeric violation (1_hosts -> 3_runtime via
normalizeValue/rows/commit). storejs-only cycle: tasks<->algo mutual
(rust is one-directional — mirror drift).

TOOL-GAP EVIDENCE for parked arcs: dep matrix needed import SPECIFIERS
(had to source-grep) -> strengthens the ModuleF/specifiers ruling; envy-grade
analysis needs receiver identity on member nodes -> design input beyond I1
(df_fields alone insufficient); the 52-file shell-loop driver is exactly what
CLI-8 project mode should absorb; cross-blob dead-def needed consumer name
pools -> resolve-layer territory. Raw artifacts: /tmp/extract-dogfood/
(v6-*.jsonl + analyze.py), ephemeral.

## Design seeds (2026-07-25, user-approved for later; PARKED — no briefs yet)

### S1 — Similarity: α-normalized anti-unification over df subgraphs

The user's thought, named: clone detection where names don't matter
(α-equivalence via de Bruijn slots — df use-def edges make renaming SOUND,
not textual), literals widen to primitive tags (lit/template -> str/num/bool),
LHS bind-names erased, types + reference ORDER kept (sig rows + edge
topology). Score = anti-unification retention (least general generalization:
exact dup = whole skeleton; near-miss = skeleton with holes; score + PRINTED
SKELETON WITNESS = fact with evidence, not an opinion). Substrate advantage
over token tools: df kinds are language-neutral, so ts<->rust mirror code
collides naturally — the built-in ratchet corpus is tasks.rs vs tasks.ts
(nearest-neighbor assertion) + determinism gate (same fixture -> byte-exact
scores, normalization_version field). Build: Stage 0 = canonicalize df
subgraph per fn, blake3, cluster identical hashes -> `dup_of` rows (few
hundred lines). Stage 1 = kind n-gram buckets -> anti-unify candidate pairs
only -> `similar_to(a,b,score,skeleton,version)`. Stage 2 (probably never):
VF2 edit distance on top candidates. Speed comes from BUCKETING (O(n) hash,
pairwise only on collision), not rust. Home: sibling bin in the extract crate
consuming the same bundles (facts in, facts out; no watchers/daemon;
deterministically testable = inside the boundary). v6-only rows: no v5
parity, determinism + mirror ratchet is the gate.

### S2 — Three layers + the doc lane (JSON/YAML path rows; RTKQ golden)

The articulation: (1) INDEX "what is written" (extract), (2) DERIVE "what
follows" (store+dl), (3) PROGRAMS "what it means for your framework" (.dl
scripts). Boundary law proven by v5's examples/rtkq-op-recovery.dl: the
useGetUserQuery string surgery lives in DATALOG, never in the extractor.
Acceptance criterion: every family/facet earns its place via a program that
needs it. FACTS: .json/.yaml ALREADY emit cst rows via the ast-grep floor
(measured 22/39 nodes); .toml has NO grammar in-tree. But cst is the wrong
doc shape — v5's jsonp builtin proves programs want KEY-PATH -> VALUE rows
(paths.*.*.operationId). So: doc-lane facet (walk the floor, emit
path/value/primitive-type rows) for json/yaml — zero new deps, sibling to
I2, queued as I2.5. TOML waits for its program (Cargo.toml dep queries;
tree-sitter-toml = dep-rails decision then). Golden-test ladder: code lane
PROVEN (all 5 RTKQ hook sites stream from components.tsx today); doc lane =
the missing piece; end-to-end = rtkq-op-recovery.dl on the v6 pipeline
(openapi-sim corpus: openapi.json + components.tsx + client.ts + hooks.ts +
server.rs). JSX/hooks program needs df through props + dependency arrays =
I1's fields/lits is its fact-sufficiency step.

### S3 — Derived inter-procedural flow (NOT extracted)

Intra = within one fn (phase-1, per-file pure, parallel, cacheable — KEEP).
Inter (arg->param, ret->call-res) is DERIVED in the engine, three rules:
flow(arg,param) <- site resolves to D, arg pos i, D param pos i;
flow(ret,call_res) <- resolved def, ret in body; value_path = df_edge o
flow (fixpoint). Join keys = I1's df_args/df_param_pos + commit-4 resolve
edges. Seed vocabulary ALREADY reserved: FlowEdgeKind{ArgToParam,
RetToCallRes, LambdaElem, LambdaRet}; v5 precedent: flow_edge was a dl union
(std/flow.dl:89). Data honesty: new edges are O(call sites); blowup only if
you materialize all-pairs — so evaluate ON DEMAND (labs/prolog.ts SLG/
tabling experiment = the demand-evaluation prototype; not dead weight).
Precision is a program-level dial: context-insensitive first, k-limited
later, summaries after. Eager whole-repo context-sensitive extraction = the
IFDS trap, correctly feared, never queued. Lambdas (LambdaElem/LambdaRet) =
cheapest first win, hooks/JSX is full of them.

### S4 — Analysis family map (what's a PROGRAM vs a FACET vs a RABBIT HOLE)

Frame: extract -> fact store -> query programs IS the CodeQL shape
(Semmle extractor -> db -> QL); cst+df+call as merged graph planes IS
Joern's Code Property Graph. The field was not missed; it was rebuilt.

PROGRAMS over already-emitted facts (zero new extraction):
- dead code / liveness (dead stores = one df query)
- TAINT (source->sink + sanitizers = S3 flow + endpoint annotations; THE
  killer app; sqli/xss demos)
- TYPESTATE / API-protocol ("open before read"; rules-of-hooks IS one;
  call graph + temporal rule)
- metrics/architecture (coupling, cohesion, cyclomatic; dogfood #2 was
  this by hand)
- effect/purity ("touches globals/DOM/network" = call-graph reach to an
  effectful-API list; render-purity + hook discipline want it)

ONE NEW FACET, then a program:
- CONTROL DEPENDENCE (dominance/frontiers from cst) -> PROGRAM SLICING
  ("what affects this line" = PDG complete; the nav superpower)
- POINTER/ALIAS (underlies call-graph QUALITY + field-write questions;
  Doop = the datalog-native lineage; deepest facet, own arc later)
- abstract interpretation lite (const facet is already baby constant
  propagation; interval/range = same fixpoint over a lattice)
- escape/capture (lam_sym closures already carry captures; escape-lite =
  a df query)

ADJACENT UNIVERSES (know, don't build):
- symbolic execution (SMT religion, not facts-first)
- shape/heap (TVLA; eats careers)
- concurrency (races hard statically; cheap slice = lock-order graphs +
  await-across-lock lints)
- termination (the ENGINE guarantees it; analyzer needn't prove it)

THE TRAP: big-O. Precise static complexity is unsolved; heuristics only
(loop depth x call-graph cycles x input-size params). Sell as "smells
quadratic" or it lies.

TAKEAWAY (priority): taint + slicing = the two highest-value next
programs (ride S3 + control-dependence); pointer = deepest facet arc;
typestate = the everyday workhorse (hooks/RTK/API-misuse). Everything
else: programs, NOT extractors.
