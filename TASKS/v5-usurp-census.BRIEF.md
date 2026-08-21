# Brief: v5 usurped. The extraction parity matrix, the rail census, the retirement plan

Issue: file `issuectl --json new --type epic --title "Usurp v4 and v5: extraction parity, rail census,
retirement" --owner chris --priority high` as your first commit; child issues per gap you find (type task,
`--epic <that slug>`). Base sha: printed by the spawner; FIRST ACTION `git merge --ff-only <sha>`;
failure = stop and report. Never spawn subagents. Deliver through a GitHub PR against `main`, body ends
`Refs-Issue: @<epic slug>`.

## The user's ask (2026-08-21, verbatim intent)
"i just want all that v4/v5 to be usurped and its taking ages. this extract shit should be at parity with
all of v5 and below's extraction capabilities." Decision on record (CLAUDE.md): "I DO NOT WANT TO RUN V5
ANYTHING ANYMORE." Archives: `~/projects/sprefa-archive-20260701` (v3/v4), `-20260428` (OG).

## Laws in force
tsv2 paused (Rust door only). Zero shell in the engine. Banned words in any form: "ground truth" (say
oracle). Banned in prose and identifiers: provenance, substrate, load-bearing, regime, refusal,
honest(ly), ground* as a verb, support. No em dashes. A refusal is a hypothesis: every "v6 cannot do X"
row cites the throw site or the missing fixture, never a comment. Grep `v6/prolog/compile/out/manifest.json`
before calling anything unsupported. Every command wraps `timeout`; nothing foreground over 10s. Commit
messages imperative ending `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

## Read first
`CLAUDE.md`; root `Cargo.toml` (the v5 crate `sprefa_v5`, binary `dl`, 45k lines under `src/`);
`src/engine/family/` and `src/engine/` (v5 builtin rels: grep `builtin`/`lazy`/the rel catalog; the
`builtin-rel-implementer` and `extraction-op-implementer` agent definitions in `.claude/agents/` name the
checklists and the catalog files), `src/**/scip*` (v5's 10 scip rels), `examples/*.dl` and `.dl/*.dl` (163
files: the v5 rails and examples; `.dl/no-new-eprintln.dl`, `examples/recompute-guard.dl` are named in
CLAUDE.md as live rails), `justfile` (root) and `v6/justfile` (24 lines naming v5 or `target/release/dl`),
`.github/workflows/ci.yml` (v5 gates removed 2026-08-11), `plans/2026-07-20-v5-assimilation.md`,
`plans/2026-07-27-v5-port-perf-header.md`, `plans/2026-08-12-v6-native-lsp.PLAN.md` (diagnostics is the
one editor feature still reaching an editor through v5), `plans/2026-07-23-sprefa-extract-golden-plan.md`,
`plans/2026-08-16-extract-generic-typesystems.PLAN.md`, `chat_log/20260816.2.*.md` ("did we fully extract
v5 scip: 8/10 rels ported, occurrence/binding via passthrough, indexers 3/6"); `v6/sprefa-extract/src/
schema.rs` (every record per family), `src/lang/mod.rs` `sources()` (the language roster), `src/lang/*`,
`src/scip*.rs`, `src/deps.rs`, `src/project.rs` (`--resolve`, `--scip-*`, `--deps`), `src/bin/extract.rs`
(`--help`); `v6/sprefa-extract/tests/golden_parity.rs` (what parity is already pinned against v5 and how);
`v6/prolog/compile/registry.pl:330-480` (host rows: the extract-shaped hosts a dl6 program can declare);
`v6/dl/**` (the dl6 rails that exist: deadcode, reach, ghcacher, crosswalk, selfdoc, dataflow, hotpath,
typegen); `issues/` (`issuectl --json ls "text:extract"`, `text:v5`, `text:scip`, to link, not duplicate).

## Deliverables
1. **Extraction parity matrix** `docs/v5-extraction-parity.md` (TOC first): one row per v5 capability,
   grouped: builtin rels (every rel the v5 catalog lists, by name, columns, language coverage), extraction
   ops (`match`, `ast`, `sg`, `json`, regex/grep, span ops, comment ops), scip rels (all 10), dataflow/CPG
   planes, git/repo planes, module/import planes, per language (v5's roster vs `sources()`). Columns: v5
   name / v5 site (file:line) / v6 equivalent (record + family + host name, or `--flag`) / v6 site / parity
   (identical, superset, subset with the missing columns named, missing) / fixture that proves it (path, or
   "none: write it") / cost note if measured. Every "missing" row gets a child issue with the record shape
   proposed and the v5 site to port from. Count the rows per parity bucket at the top.
2. **Rail census** `docs/v5-rail-census.md`: the 163 `.dl` files bucketed: ported to dl6 (name the dl6
   twin), portable as-is (every construct has a dl6 spelling per the manifest; list the constructs), blocked
   (the construct and its throw site or missing fixture), dead (nothing references it: `git log` last touch,
   no justfile/CI/docs reference). For `.dl/no-new-eprintln.dl` and `examples/recompute-guard.dl` (CLAUDE.md
   names them as live rails): port them to `v6/dl/rails/` as dl6 programs with a gate each, run both, paste
   results (eprintln count in `v6/**/src/**` today; recompute-guard findings). Those two ports are the only
   code in this lane besides fixtures.
3. **Retirement plan** `plans/2026-08-21-v5-retirement.PLAN.md` + `.PLAN.visual.human.unga.md` (the second
   doc: TOC, one mermaid per phase, zero citations): what must exist in v6 before `src/` moves to an
   archive (diagnostics through the v6 LSP plan, the ported rails, the parity rows marked missing that any
   live consumer needs), the order, the gate per step, and the deletion commit's shape (the workspace
   `members`, the `dl` binary, the 24 justfile lines, the agent definitions under `.claude/agents/` that
   implement v5 checklists). Name the day v5 stops building.
4. Gates you run (paste): `cd v6/sprefa-extract && timeout 900 cargo test --release --features cli` (186
   today + your fixtures), `timeout 600 bash v6/sprefa-engine-rs/grade.sh` (439/335 rc=0), the two rail
   ports' gates, `cd v6 && timeout 600 just oracle-rustc && timeout 600 just oracle-knip`.

## File ownership (peers live: ghcacher, N+1 audit (engine internals + extract internals), reach, selfdoc,
compiler, crosswalk, one-rel plan)
YOURS: `docs/v5-extraction-parity.md`, `docs/v5-rail-census.md`, the two plan docs, `v6/dl/rails/**` (new),
`v6/sprefa-extract/tests/fixtures/v5_parity/**` (new fixtures only) + ONE new test file
`v6/sprefa-extract/tests/33_v5_parity_matrix.rs` that asserts each "identical" row by running both
shapes where v5's output is checked in (never by running the v5 binary), `issues/` (your epic + children),
`v6/justfile` appended recipes for the two rails.
FORBIDDEN: `src/**` (v5 itself: read only), `v6/sprefa-extract/src/**`, `v6/sprefa-engine-rs/**`,
`v6/prolog/**`, `v6/tsv2/**`, every other `v6/dl/*` dir. Requests with exact diffs in the PR body.

## Report (PR body), tables and lists only
parity bucket counts; the missing rows with their issue slugs; rail census counts; the two rail ports'
results; the retirement order as a table (step / gate / owner); gate outputs; requests.
