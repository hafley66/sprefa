# PR graph-diff (dataflow differential vs main)

Date: 2026-07-03. Route: worktree-pair (base checkout + head worktree as two
config repos) — works with today's engine. The rev-aware-extraction engine arc
(thread rev through type_entity/call_def/df, type_edge_rev as template)
replaces the pair with `scan("main")` vs `scan("WORK")` later; the diff rels
and panel mode below survive that swap unchanged (repo column becomes rev).

## Why worktree-pair now

Extraction (type/call/df families) emits WORK-only rows; only
type_edge_rev/module_edge_rev carry rev. Two checkouts fake two revs as two
repos: syms arrive prefixed with the root dir basename (`sprefa::…` vs
`sprefa-base::…`), so the diff is prefix-strip + set difference. Stable diff
keys = sym-keyed graphs only (member_edge, port_edge, type_link, call_edge);
df-node ids embed line numbers and MUST NOT be diffed raw.

## Diff semantics

```
bare_edge(repo, a, b, kind) <- member_edge(A, B, kind),
    repo = prefix(A), a = strip(A), b = strip(B).
edge_added(a, b, kind)   <- bare_edge(HEAD, a, b, kind), !bare_edge(BASE, a, b, kind).
edge_removed(a, b, kind) <- bare_edge(BASE, a, b, kind), !bare_edge(HEAD, a, b, kind).
node_added / node_removed: same shape over member_node syms.
```

HEAD/BASE slugs come from a `diff_pair(base_slug, head_slug).` fact — the
convention-read pattern; the setup script writes it.

## Panel representation

- added: solid arc, hot color; removed: dashed, muted red, rows survive as
  ghosts (the symbol may not exist at head); unchanged: dimmed.
- chips: added / removed / unchanged toggles; count pill `+N −M`.
- diff preset = nodes from both namespaces prefix-stripped and unioned
  (dedup by bare sym), edges tagged with the diff class as kind suffix.

## SCIP: two runs, one per root

`scip_want(repo)` already runs ensure_index per wanted root and merges into
ONE load. So: `git worktree add ../sprefa-base main` + register both roots +
`scip_want` facts for both = two indexes, automatic. OPEN HAZARD (D3 probe):
both roots emit IDENTICAL symbol monikers (same crate, two revs) — if the
merged load cross-resolves a head ref to a base def (or vice versa),
call_edge/type_link get cross-wired and the diff lies. If the probe shows
cross-wiring: scope resolution within-repo (engine tweak) or drop SCIP and
rely on the syntactic name-unique fallback, which is at least SYMMETRICAL
(same over/under-approximation both sides, diff stays fair).

## Test protocol (D4, the gate)

Synthetic PR with a controlled edit set on a scratch branch:
1. move a call site from fn A to fn B  -> expect: edge_removed(A→callee),
   edge_added(B→callee), exactly one each in call kind
2. add a field fill (new struct literal field) -> expect: one edge_added
   kind=fill + node_added for the field if new
3. delete a small fn -> expect: node_removed + its edges all removed
4. no-op edit (comment only) -> expect: zero added/removed (the noise gate;
   this is the one that catches line-keyed leakage)
Assert exact counts via sqlite queries in a bash harness
(bench/ or tests/fixtures/ script, not a cargo test — engine untouched).
Gate: all four scenarios pass with SCIP on AND off (D3 informs whether
"on" is per-repo or skipped).

## Task queue

- [ ] D0 setup script: `git worktree add ../sprefa-base main`, write
      diff.config.toml (slugs base/head) + diff_pair fact, verify
      rel_member_node shows both prefixes. No SCIP yet.
- [ ] D1 diff rels: .dl/graph-diff.dl (bare_edge, edge_added/removed,
      node_added/removed, diff_pair convention). --check + live counts on
      the pair.
- [ ] D2 panel diff preset + 3 visual classes + chips + count pill.
- [ ] D3 SCIP collision probe: index both roots, query whether any scip_ref
      def_file crosses roots for a same-name sym; decide per-repo scope vs
      symmetric-off.
- [ ] D4 synthetic-PR test harness (4 scenarios above), run with D3's
      decision applied.
- [ ] D5 (separate engine arc, unblocks the clean form): rev-aware
      type_entity/call_def/df extraction; then diff_pair becomes (rev, rev)
      and the worktree pair retires. PR-as-rows (gh effects -> sha -> scan
      rev slot) composes after this.

## Execution runbook (self-contained — any coordinator session can drive this)

COORDINATION RULES (apply to every task):
- One Sonnet agent per task, one task in flight per FILE (D1 and D3 can run
  in parallel after D0 — disjoint targets).
- Every agent prompt MUST include: (a) the worktree path
  /Users/chrishafley/projects/sprefa/.claude/worktrees/vscode-flow-panel and
  "run everything from there"; (b) GIT SAFETY: never run git
  restore/checkout/stash/clean outside your named target files; (c) "do not
  read README.md/docs//src/*.rs, no cargo, no test suites" unless the task
  needs it; (d) an explicit verify budget; (e) "No commits."
- A full `dl` one-shot run triggers doc-gen splices into README/docs — agents
  verify with --check unless the task needs a live probe; live probes go
  --db into the agent's scratchpad and any README/docs dirt is LEFT ALONE.
- .dl house rules to paste into .dl-editing prompts: one rel = one rule kind
  (source and derived rules never head the same rel; facts count as their own
  kind); a rule with a scan/ast/sg/json/comment atom cannot bind head vars
  from a builtin/derived rel read — split scan-forcer seed rel + pure derived
  join rule; dedup by negation (no MinBy); closure() rels can't be read
  unpinned — seeded recursive rules instead; 5-ary fan scan form is
  scan("*", "WORK", glob, path, rev) — the 4-ary form's first string is a REV
  selector, not repo.
- SHIP PIPELINE after any flow-panel.html change (coordinator runs it, not
  the agent):
  1. python3 NUL-byte count on the file = 0; extract last <script>, node --check
  2. cd editors/vscode-dl && npm run compile && npx vsce package --allow-missing-repository
  3. code --uninstall-extension sprefa.dl-lsp; rm -rf ~/.vscode/extensions/sprefa.dl-lsp-*;
     code --install-extension dl-lsp-0.4.1.vsix
  4. md5 -q media/flow-panel.html ~/.vscode/extensions/sprefa.dl-lsp-0.4.1/media/flow-panel.html
     — MUST match (same-version reinstall silently no-ops otherwise)
  5. user reloads window + reopens panel (webview caches HTML)
- SHIP PIPELINE after any .dl/*.dl or std/*.dl change: dl --check; cp to
  ~/projects/sprefa/<same path> (daemon serves from there; std/entry.dl
  precedent); then tail ~/projects/sprefa/.dl/daemon.log — a NEW
  "ON CONFLICT" line after the sync = the lattice hot-reload wedge (ledgered
  bug); restart the daemon if it appears
  (~/.cargo/bin/dl --daemon --root /Users/chrishafley/projects/sprefa).

D0 SPECIFICS (gotchas already known):
- main is checked out in ~/projects/sprefa itself, so a second checkout needs
  detach: `git -C ~/projects/sprefa worktree add --detach ../sprefa-base main`
- artifacts live in bench/graph_diff/: setup.sh (idempotent — skip if the
  worktree exists), diff.config.toml with [[repos]] base=~/projects/sprefa-base
  + head=<the dev worktree root>, and the probe .dl.
- config loads via SPREFA_CONFIG=bench/graph_diff/diff.config.toml on the
  one-shot; do NOT touch ~/.config/sprefa/config.toml (the live daemon's).
- sym prefixes come from root dir BASENAMES (sprefa-base, vscode-flow-panel),
  not config slugs — the selfv5→sprefa mismatch is known; diff_pair carries
  the BASENAMES.
- probe passes when rel_member_node has both prefixes and
  SELECT count(*) per prefix is within 2x of each other (same codebase,
  two revs).

D1 SPECIFICS: new file .dl/graph-diff.dl; diff_pair("sprefa-base",
"vscode-flow-panel"). fact; prefix strip = replace_re(sym, "^[^:]*::", "")
(the house transform); edge_added/removed + node_added/removed per the
semantics section; --check + live counts against the D0 pair (expect
nonzero both directions — the branches genuinely differ).

D2 SPECIFICS: diff preset queries rel_edge_added/rel_edge_removed/
rel_bare_edge; removed nodes may not exist at head — synthesize ghost rows
from node_removed; do NOT touch applyCollapse/focus/trace/marquee/pins code.

D3 SPECIFICS: dl index (or scip_want facts) per root; the probe query is
"does any scip_ref row resolve a file under root A to a def_file under
root B" — file paths in the merged load are the join key to inspect;
decision recorded IN THIS PLAN under a new '## D3 verdict' heading.

D4 SPECIFICS: scratch branch off main in the sprefa-base worktree is the
BASE mutation side (git -C ~/projects/sprefa-base switch -c diff-harness);
4 scenarios as specced; harness = bench/graph_diff/harness.sh asserting
exact counts via sqlite3; comment-only scenario asserting ZERO diff rows is
the gate that fails if anyone ever leaks a line-keyed id into the diff.

D5 is an engine arc (Rust): separate plan when picked up; typegraph.rs +
extract family digests; type_edge_rev is the template.

## D3 verdict

**Cross-wired at BOTH layers — and SCIP does not make it better OR worse.**
Original framing of this section (below the fold) assumed the syntactic
name-unique fallback was the safe escape hatch ("at least SYMMETRICAL").
D1's live run of `.dl/graph-diff.dl` disproved that: with NO SCIP index
loaded at all, `edge_removed` was 3054 rows, and every sym-resolved kind
(call/field/impl/uses) existed **only** with `sprefa-base` sources — zero
for `vscode-flow-panel` — while df-sourced kinds (fill/param/read) stayed
symmetric. I reproduced this independently in my own db (see below) and
then re-ran the SAME diff rels with `scip_want("base")` added: **byte-
identical** counts, both `edge_removed` (3054, same per-kind split) and
`rel_bare_edge` per-repo counts (`sprefa-base=5179`, `vscode-flow-panel=
2125`, unchanged to the row). SCIP neither rescues nor worsens the
collapse — it is already flat before SCIP engages, because the SCIP
override table (`scip_ref`) suffers the identical bare-symbol collapse
documented in the "Root cause" section below, so `scip_name_defs()`
contributes nothing new to resolution when both roots are the same crate.

**Decision: SCIP off for the diff AND per-repo scoping is required in the
syntactic resolver too — this is not solved by "just skip SCIP".** The
plan's original fallback assumption is wrong; folding this into D1's
ledger entry (CLAUDE.md Open, "syntactic name resolver is corpus-flat") is
the correct next step, not a diff-feature workaround. Two independent
defects, one shared symptom (global-not-per-repo symbol resolution):
1. **SCIP importer** (`src/scip_import.rs`) — structural, proven by static
   read + the merge-count evidence below.
2. **Syntactic name-unique resolver** (`src/engine/extract.rs`,
   `refresh_type_rels`/`refresh_call_rels`) — proven empirically (D1 +
   my own rerun), NOT proven at the exact code line. The `by_name`/
   `sym_at` maps in `refresh_type_rels` ARE lexically keyed by
   `(repo, name)` / `(repo, file, name)` (extract.rs:507-533) and
   `member_node`/`type_entity` DO carry the correct per-repo `repo::sym`
   prefix symmetrically (my query: `member_node` 4285/4285 split, `src`
   side of `type_link` 502/502 split) — so entity ATTRIBUTION is correct.
   The RESOLUTION step (`dst = resolve(repo, path, &edge.to)`) is what
   goes corpus-flat: querying my own db, every `type_link` row whose `dst`
   carries a repo-qualified prefix carries `sprefa-base::`, never
   `vscode-flow-panel::` (142 rows, 0 counterexamples), even for edges
   whose `src` is unambiguously `vscode-flow-panel::`-prefixed (those
   edges' `dst` was bare/unresolved instead, never
   `vscode-flow-panel::`-qualified). The exact mechanism (whether it's
   `_file.repo` vs the git-basename `rid` disagreeing somewhere in
   `repo_roots()`/`cached_facts` keying, or a HashMap iteration-order
   artifact) is NOT isolated within this probe's budget — flagged for
   whoever picks up D1's proposed **D5a** (engine fix: scope syntactic
   resolution candidates to the ref site's repo).

**Setup**: `sprefa-base` (main, `2c6711d`) and the `vscode-flow-panel`
worktree (also `2c6711d` — literally the same commit right now, the worst
case) both indexed via `dl index --install` (rust-analyzer on PATH).
Indexing time: base 31.0s cold, head 12.8s (rust-analyzer warm caches from
the base run). Both produced identical `scip_def=3548 scip_ref=4724
scip_edge=548 scip_fn_edge=10646`.

**Root cause (read `src/scip_import.rs`, `rows()` + `merge_files()`,
`src/rels/scip.rs` `resolve_index()`)**:
- `scip_def.file` / `scip_ref.file` / `scip_ref.def_file` are bare
  repo-relative paths (`doc.relative_path`, e.g. `src/agent.rs`) — no root
  prefix, no repo column anywhere in the SCIP importer relations. Two roots
  with the same file layout are byte-identical strings on this axis alone,
  confirming the plan's suspicion.
- rust-analyzer's own moniker scheme is package+version scoped, not
  path-scoped (`rust-analyzer cargo sprefa-dl 0.4.1 scc/…`) — visible
  directly in `dl index`'s own "Duplicate symbol" warnings during indexing,
  which fire even within a SINGLE root (multiple targets sharing module
  paths). Two roots building the same crate at the same version mint the
  IDENTICAL symbol string.
- `merge_files()` (scip_import.rs) is a dumb concatenation of
  `documents`/`external_symbols` — no root disambiguation.
- `rows()`'s def-collection is `def_file.entry(occ.symbol.clone())
  .or_insert_with(|| doc.relative_path.clone())` — first-wins, keyed on the
  bare symbol string alone. Once head's documents (self index, listed first
  in `resolve_index`'s `parts`) populate `def_file[S]`, base's identical
  occurrences of `S` are silent `or_insert_with` no-ops. The refs loop then
  does `def_file.get(&occ.symbol)` — a ref recorded in a base-root document
  resolves against head's def_file entry (or vice versa depending on
  `parts` order), with zero signal anywhere that a cross-root hop happened.

**Evidence query** (`bench/graph_diff/scip-probe.dl`, run twice — once with
no `scip_want` fact (self root only) and once with `scip_want("base")`
added — against `SPREFA_CONFIG=bench/graph_diff/diff.config.toml
--no-daemon --db <scratch>/d3.sqlite`):

| run | scip_def rows | scip_ref rows |
|---|---|---|
| self-root only (`--root .`, no `scip_want`) | 3548 | 4724 |
| merged (`scip_want("base")`, base+head indexes concatenated) | 3548 | 3548 |

Merging in a second FULL index (base's own 3548 defs / 4724 refs) added
**zero** net rows. If the two roots' data were kept distinguishable, the
merged counts would be up to 2x (or somewhere between 3548 and 7096 for
partial overlap); observing an EXACT match to the single-root baseline
means every def/ref pair from `base` collapsed onto an identical
`(symbol, file)` / `(file, symbol, def_file)` tuple already contributed by
`head`. `sqlite3 <scratch>/d3.sqlite "select count(*) from rel_def_row"` →
3548; `count(distinct file)` → 174 (matches a single repo's file count,
confirming no root ever shows up twice in the joined set).

A `def_multi` check (does any single symbol resolve to >1 distinct file)
found only 8 pre-existing SELF-collisions (module-vs-impl-block moniker
reuse within one root, e.g. `scc/`, `daemon/`, `mcp/serve()`) — the SAME 8
in both runs. This is the sharper confirmation: since defs collapse
first-wins per symbol, `scip_def` can never show 2 distinct files for the
same symbol post-merge even though two full roots' worth of definitions
went in — the second root's rows are absorbed, not merely occasionally
misattributed.

**Evidence query, layer 2 (syntactic resolver, SCIP-on vs SCIP-off)**:
same `.dl/graph-diff.dl` D1 already wrote, run twice against my own scratch
dbs — once bare (`--db d3_nosc.sqlite`), once with `.dl/graph-diff.dl
bench/graph_diff/scip-want.dl` merged on the command line (multi-file
merge, `scip_want("base")` the only added fact, `--db d3_scip.sqlite`):

| run | `edge_removed` total | call / field / impl / uses | `bare_edge` sprefa-base / vscode-flow-panel |
|---|---|---|---|
| no SCIP | 3054 | 2612 / 67 / 37 / 338 | 5179 / 2125 |
| `scip_want("base")` | 3054 | 2612 / 67 / 37 / 338 | 5179 / 2125 |

Byte-identical to the row. `member_node` (entity attribution) is correctly
symmetric (4285/4285 per repo) both runs; `type_link` rows whose `dst`
carries a repo prefix are 142/142, **100% `sprefa-base::`, 0
`vscode-flow-panel::`**, both runs. SCIP is a complete no-op here because
`scip_name_defs()` reads the already-collapsed `scip_ref` table (layer 1's
defect) — there is no live SCIP data left to override the syntactic
fallback with once both roots are the same crate.

**Decision**: `edge_added`/`edge_removed`/`node_added`/`node_removed`
(D1) must NOT be trusted for `call`/`field`/`impl`/`uses` kinds while base
and head are two on-disk roots of the same crate, **with or without
SCIP** — turning SCIP off does not make the syntactic path safe, contrary
to this section's original assumption. D1's fork stands as the actual
plan of record:
- **(i) INTERIM** (unblocks D2/D4 now): scope the diff to the
  SYMMETRIC kinds only — `member_node`/`member_edge` rows sourced from
  `df_*` (fill/param/read) plus plain node add/remove, which do not route
  through `resolve()`. Ships without any engine change.
- **(ii) D5a** (new, smaller than D5): fix the syntactic resolver's
  per-repo scoping in `refresh_type_rels`/`refresh_call_rels`
  (`src/engine/extract.rs`) — the `by_name`/`sym_at` maps are already
  *declared* per-repo but the resolved `dst` is empirically corpus-flat;
  needs isolation (my read didn't find the exact line, see "Root cause,
  layer 2" above). Unblocks all kinds for this pair, and fixes latent
  cross-wiring for any config repos that happen to share type names
  (not just deliberate diff pairs).
- **SCIP** (layer 1, `scip_import::rows()`) stays a SEPARATE fix — scoping
  `def_file`/`refs` by (root, symbol) instead of bare symbol — needed only
  if/when SCIP-backed resolution is wanted for a same-crate pair; not on
  this arc's critical path since (i)/(ii) don't depend on it.
- Both hazards disappear structurally once D5 (rev-aware extraction) lands:
  one root, one `index.scip`, one tick — the base/head split moves into
  `type_entity`/`call_def`/`df`'s rev column instead of a second on-disk
  root, so `resolve_index`'s `parts` and `refresh_type_rels`'s `repo`
  loop each only ever see ONE physical location again.

Dirt left: `~/projects/sprefa-base/.dl/index.scip` (11.0 MB, gitignored) and
this worktree's own `.dl/index.scip` (11.0 MB, gitignored) — both are
turnkey `dl index` artifacts, harmless to leave, `dl index` overwrites them
idempotently. `bench/graph_diff/scip-probe.dl` and `bench/graph_diff/
scip-want.dl` are permanent artifacts of this probe, left in place for
anyone re-verifying D3 after an engine change. No git writes, no commits.

## D1 results (2026-07-03)

.dl/graph-diff.dl landed (same-dir `use "flow-panel.dl".` resolves). Identity
gate at identical commits (2c6711d both roots):
- NODES: PASS — node_added/removed = 0, bare_node 4285 = 4285 per repo,
  prefix strip/extract verified on real syms.
- EDGES: FAIL, root-caused OUTSIDE graph-diff.dl — edge_removed = 3054 =
  call(2612) + field(67) + impl(37) + uses(338): every sym-RESOLVED kind
  exists only with sprefa-base sources; df-sourced kinds (fill/param/read)
  are symmetric. The syntactic name resolver is corpus-flat: ambiguous
  names (identical in both roots) all resolve into sprefa-base. No SCIP
  loaded — the hazard the D3 section predicted fires in the plain
  syntactic path. Ledgered as an engine bug in CLAUDE.md Open.

CONSEQUENCE FOR THE ARC — decision fork:
- (i) INTERIM: D2/D4 proceed on the SYMMETRIC kinds only (fill/param/read
  member edges + node diffs) — honest, partial, ships without engine work.
- (ii) ENGINE FIX (new task D5a, smaller than D5): scope syntactic
  resolution candidates to the ref site's repo; cross-repo only via SCIP or
  explicit opt-in. Unblocks all kinds for the pair AND fixes latent
  cross-wiring for any config repos sharing type names.
D3's verdict (SCIP-on behavior) informs whether SCIP also needs the scope
guard or already keys per index.

## D5a result (2026-07-03) — resolver fixed, identity gate exact 0

Root cause was NOT flat keying. `by_name` in `refresh_type_rels` /
`refresh_call_rels` (src/engine/extract.rs) is correctly keyed
(repo, name), but pushed one entry per raw fact occurrence. The bench
config registers this worktree twice under ONE rid (config slug `head` +
the --root self slug both map to basename `vscode-flow-panel`), so every
def sym landed twice -> len 2 -> read as ambiguous -> the whole head repo
resolved bare, while single-scanned sprefa-base resolved normally. That
produced the one-sided look (type_link.dst 142/0, call_edge 2612/0).
Fix: dedup the ambiguity bucket by def sym before the uniqueness count
(both fns, same guard). scip_import.rs untouched (separate ledgered bug).

Gate after fix (same commit both roots): type_link.dst 142/142, call_edge
2617/2617, edge_removed 3054 -> 0, edge_added 0 — exact zero across ALL
resolved kinds, no residual. New test tests/it/resolver_repo_scope.rs
(two fixture repos w/ colliding names, one double-registered). Suites
202 lib / 459 it green, coordinator re-verified independently.

CONSEQUENCE: the D3 interim scope (symmetric kinds only) is RETIRED for
the syntactic path. D2's preset kind filter removed same day. SCIP stays
off for the diff (importer collapse still ledgered, off critical path).
D4 runs with ALL edge kinds.

## D2 result (2026-07-03) — panel diff preset landed

`diff` preset ("Diff (base vs head)") in flow-panel.html: nodes union
node_added/node_removed(ghost rows, file/line NULL)/bare_node-both-repos
as added/removed/unchanged; edges same three-way from edge_added/
edge_removed/bare_edge (kind filter removed post-D5a). Count pill
`+N −M`, class chips, missing-table branch renders the "no diff rels —
serve .dl/graph-diff.dl against a base+head pair" hint instead of the
error banner. Canvas view gets the diff CSS classes; list view renders
rows generically (deliberate cut — renderListRows untouched, adjacent to
protected focus/collapse code). Verified against the D0 pair db plus
synthetic injected rows (ghosts render, filter behavior real, not
asserted). Wiring for live data: the daemon the panel queries must serve
graph-diff.dl under a two-root config (diff.config.toml shape); the
normal single-root dev daemon shows the hint state.

## D4 results (2026-07-03) — harness landed, 3/4 exact, S2 = engine finding

bench/graph_diff/harness.sh (new, executable): derives roots from its own
path, SCIP-off via mv+trap, anchor-checked python3 mutation patches, exact
sqlite3 row asserts, per-file BASE restore, BASE-clean exit gate. Exit
codes: 0 = all-spec, 1 = hard-gate regression, 2 = S1/S3/S4 green with S2
df-blocked (current state).

Orientation confirmed: added = HEAD-only, removed = BASE-only; mutations
land in BASE's working tree so the verb inverts (add to BASE -> removed).

- S1 call-move: PASS exact (one call edge moves print_help->run, 0 nodes).
- S3 fn delete (latest_tag): PASS exact (fn + ret node, both edges, zero
  surviving base refs).
- S4 comment-only noise gate: PASS, 0/0/0/0 — no line-keyed leakage.
- S2 field-fill: BLOCKED by a real engine behavior, NOT a harness bug:
  the df family reads config repos at committed HEAD while type/call read
  WORK (WORK-only struct+ctor -> type_entity yes, df rows zero; df lines
  stay at committed positions). Ledgered in CLAUDE.md Open. S2 re-arms
  when df reads WORK, or structurally under D5 (rev becomes the diff
  axis). Note the plan's line-11 claim "df families emit WORK-only rows"
  was wrong for config repos.

Residual dirt (intentional, gitignored/arc-owned): sprefa-base porcelain
shows only ` M .dl/.gitignore` (D3's index.scip line); both roots keep
their .dl/index.scip.

## SCIP re-key result (2026-07-03) — per-repo scip rows, SCIP-on == SCIP-off

Per-index load with origin repo threaded through (rows(index, root, slug),
repo_of() = nearest-.git basename matching engine repo_id_of; index_inputs
replaces the single merged path; merge_files survives only as the
`dl index` on-disk artifact). scip_def/scip_ref/scip_edge carry a trailing
repo column; dl arity is exact so 13 positional readers swept with `_`.
scip_name_defs keyed (repo, file, name); both resolver closures
repo-scoped; cross-repo SCIP resolution dropped (matches D5a semantics).
Sleeper fixed en route: extract_input_digest now folds scip_ref.repo —
base's byte-identical (file,symbol,def_file) triples XOR-cancelled head's
and false-skipped the family.

Gates: second index ADDS 3548/4724 (was zero net; totals 7096/9448 split
exactly per repo); identity-shape gate = SCIP-on diff byte-identical to
SCIP-off (20/11 both ways — genuine head-side source edits from this very
arc, not noise; base bare_edge grew 5186->5485, symmetric with head 5494,
proving base resolves via its OWN index). Suites 203 lib / 459 it,
coordinator re-verified. New probes: bench/graph_diff/scip-count-probe.dl,
scip-on.dl. Known pre-existing lag: scip_want consumption lands tick 3.

CONSEQUENCE: the D3 decision "SCIP off for the diff" is RETIRED — SCIP on
is now safe for the worktree pair. scip_ref.repo+file joins
_file(repo,path,rev) and the module rels directly (the scope ladder).

## D5b result (2026-07-04) — df reads config-repo WORK, harness exit 0

Defect: refresh_dataflow_rels (extract.rs:1056) read all content via
self.root; config-repo paths resolved under the wrong tree (missing ->
zero df rows). Fix: roots.get(repo) like type/call. Second defect en
route: df rows are path-keyed (no repo), so flow-panel.dl's name-joined
field/fill rules fanned a base-only fill across BOTH repos (the diff went
blind, not wrong). NEW builtin df_node_repo(id, repo) emitted per (id,
repo) occurrence — first-seen dedup regressed to 204 false removals
because identical files across the pair share node ids — plus repo-scoped
joins in flow-panel.dl (owner_in_repo/member_bare_repo helpers). Digest
already WORK-hashed; the fix restored read/digest symmetry.

harness.sh S2 re-armed to a hard gate; exit codes now 0/1. On a synced
pair: all four scenarios PASS exact, exit 0, BASE clean. Against the real
sprefa-base the baseline is nonzero (this worktree carries the day's ~60
uncommitted files) — expected until the arc is committed and sprefa-base
fast-forwarded. Suites 204 lib / 460 it, coordinator re-verified.

OPERATIONAL: live daemon (installed 0.4.1) lacks df_node_repo; a daemon
restart on the new binary was auto-denied overnight, so the SERVED
flow-panel.dl is a compat downgrade (df_node_repo atoms stripped, poll
error cleared, no wedge). Worktree copy = real version; resync after the
binary upgrade. Ledgered as a morning action in CLAUDE.md.
