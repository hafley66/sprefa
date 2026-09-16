# Brief: cross-repo crawl on the Rust door, real org fixture, entry points at rev X, watch under a RAM budget

Issue: file one with `issuectl --json new --type feature --title "Cross-repo crawl on the Rust door: crosswalk"
--epic cheap-fast-analysis --reporter chris --assignee chris` as your first commit and use its slug in
`Refs-Issue:`. Base sha: printed by the spawner; FIRST ACTION `git merge --ff-only <sha>`; failure = stop and
report. Never spawn subagents. Deliver through a GitHub PR against `main` with the receipts below.

## The user's ask (2026-08-21, verbatim intent)
"crawling the grafana github org fixture git checkout better yield results. i want to use this at dayjob
to track cross repo logic and flows and paths and i want to declare the entrypoints of an app at rev X
because some logic/wiring was used at that time to indicate this structure. i want to have reactive
efficient conditional logic watching in background without reaming my ram."

## Laws in force (CLAUDE.md)
- tsv2 paused: Rust door only; read `v6/tsv2/goldens/multirepo_crawl/**` as the spec, never edit it.
- Zero shell in the engine: every host links a Rust executor (soopy for git, sprefa-extract for parsing,
  `cargo_metadata`, `ureq` for HTTP). `Command::new("git")` is also banned in the engine: soopy is the git.
- Banned words in any form: "ground truth" (say oracle). Banned in prose and identifiers: provenance,
  substrate, load-bearing, regime, refusal, honest(ly), ground* as a verb, support. No em dashes.
- No new syntax. `order by` on `?`, spread in `decode`, aggregates exist. Seeds are plain facts or
  `--arrive`. Every `.dl6` snippet carries its pure-rxjs lowering as a comment.
- Every command wraps `timeout`. Nothing foreground over 10s. Network once per fixture build, capped,
  cached on disk. Nothing seizes the machine (`apply_daemon_budget`): a watch that grows RSS without bound
  is a blocking defect.
- `tracing` only. Surrogate keys. Comment budget. Commit messages imperative ending
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`, last paragraph `Refs-Issue: @<slug>`.

## Read first
`CLAUDE.md`; `v6/tsv2/goldens/multirepo_crawl/{README.md,0_multirepo_crawl.dl6,4_dep_crawl.dl6,
7_git_refs.dl6,10_change_facts.dl6,1_corpus.sh,6_history_corpus.sh,9_change_corpus.sh}` (the hosts:
`dep_crawl_{repo,edge,visited,unresolved}(checkout_root, seed, frontier)`, `git_{ref,tag,merge_base,
ancestor,ahead_behind,change,changed_line,rename}`, `repo_grep_at(root, rev, glob, pattern)`; the corpus
is four synthetic one-commit go repos; the gates are v5-graded and tsv2-run: you replace the RUN, not the
programs); `v6/sprefa-engine-rs/src/hosts.rs` (executor table `executor_for`, `IHostExecutor::run ->
Vec<HostRow>`, `SoopyFilesExecutor`, `CargoMetadataExecutor`, `FixtureExecutor`, `ScipNamespaceExecutor`,
`select_columns`/`carries_every_column`, `HostLiveRunner::collect` claim-once by witness), `src/types.rs`,
`src/serve.rs` (READ ONLY: the watch loop, `apply_daemon_budget`; another lane owns it), `src/bin/dl6.rs`
(the CLI: build/serve), `v6/dl/deadcode/dead-module-rail.dl6` + `receiver-rail.dl6` + adapters (the host
pattern, `cargo_targets`, `scip.call`/`scip.diet.call`), `v6/dl/reach/**` if present (a peer's entry point
x feature matrix; reuse its rels), `v6/dl/ghcacher/**` if present (a peer's `ureq` fetch + soopy
checkout executors under `src/executors/`; REUSE, do not duplicate: if `executors/checkout.rs` exists,
call it; if not, write `executors/git_checkout.rs` and say so), `~/projects/hafley-rs/crates/soopy/src/`
(`_2_repository.rs`, `_3_revision.rs`, `_3a_files.rs`, `_4_worktree.rs`, `_5_git_tree.rs`,
`_6_git_batch.rs`, `_9_git_files.rs`, `_11_refs.rs`, `_12_revision_graph.rs`, `_13_fetch.rs`,
`_14_multi_repo_refresh.rs`: the git API; every `git_*` host maps to one of these; a missing call is a
request to hafley-rs with the exact signature, never a `git` spawn), `v6/prolog/compile/registry.pl:330-
480` (`host_input_contract` rows; `repo_extract` is `(repo, path, digest)` identity/identity/freshness),
`docs/failure-modes.md` (last entries; append yours after).

## Deliverables
1. **Executors** under `v6/sprefa-engine-rs/src/executors/`: `git_refs.rs` (`git_ref`, `git_tag`),
   `git_history.rs` (`git_merge_base`, `git_ancestor`, `git_ahead_behind`, `git_change`,
   `git_changed_line`, `git_rename`), `repo_at.rs` (`repo_files_at(root, rev, glob) -> (path, digest)`,
   `repo_grep_at`), `dep_crawl.rs` (`dep_crawl_*`: go.mod parsing through sprefa-extract's `data` family
   or a `go.mod` parser crate; build-vs-buy note), registered in the `hosts.rs` table (one hunk). Each
   answers the columns the golden programs declare. Adapter sidecars for `0_multirepo_crawl.dl6`,
   `4_dep_crawl.dl6`, `7_git_refs.dl6`, `10_change_facts.dl6` under `v6/dl/crosswalk/adapters/` (the
   programs are read from tsv2 in place; copy them only if a compile needs a path change, and say so).
2. **Gate on the synthetic corpus**: `v6/dl/crosswalk/gate.sh` builds the four-repo corpus through the
   existing `1_corpus.sh`/`6_history_corpus.sh`/`9_change_corpus.sh` (they are corpus builders, not the
   paused runtime; if they shell out to `git`, that is the corpus builder's business, state it), runs
   each program through `emit_rust_harness --live-hosts --final-tsv`, diffs against
   `v5_golden/MANIFEST.tsv` the way `2_gate.sh`'s classify step does, WITHOUT python: the comparison is
   `sort`/`comm`/`diff` over TSV. Target: `dep_pin`, `skewed`, `skew_row`, `skew_width` byte-identical,
   the `dep_ver` gap stays named. `just crosswalk-gate` recipe.
3. **Real org fixture**: `v6/dl/crosswalk/fixtures/grafana.tsv` lists 3 public grafana repos
   (`grafana/grafana-plugin-sdk-go`, `grafana/loki`, `grafana/tempo` or smaller ones you measure first;
   choose by clone size under 200MB each; state sizes) each pinned at one rev (a tag from 2026, recorded
   in the tsv). `fixtures/grafana.sh` clones them ONCE through soopy into `${SPREFA_CACHE:-$HOME/.cache/
   sprefa}/crosswalk/<org>/<repo>` (shallow at the rev if soopy supports it; else full, stated), capped
   `timeout 600` total, idempotent (a second run is 0 network). If the network is unavailable, the
   synthetic gate is the deliverable and the grafana run is reported as skipped with the error text.
4. **`v6/dl/crosswalk/crosswalk.dl6`**: seeds `repo_rev(repo, rev)` and `entry_point(repo, rev, path,
   name)` as plain facts in a per-fixture `grafana.entries.dl6` (the user declares entry points at rev X
   by hand; the program never guesses them); `files` at rev through `repo_files_at`; defs and sites
   through `call_node_at`/`extract` with the `repo_extract` contract `(repo, path, digest)`; `scip.diet.
   call` per repo; `dep_edge(from_repo, to_repo, module, version)` from go.mod; `reach(repo, rev, entry,
   path, name, hops)` within a repo; `cross_reach(from_repo, entry, to_repo, path, name)` across a dep
   edge when the callee's module path matches the dependency's module (state the matching rule and what
   it cannot see); queries with `order by`: `? cross_path(from_repo, entry, to_repo, to_name, hops) order
   by hops.`, `? skew(module, repo_a, version_a, repo_b, version_b).`, `? entry_unreached(repo, rev,
   entry).` Wall under 30s on the grafana fixture with the index fresh; paste the trace table top rows.
5. **Watch under a RAM budget**: run the program in watch mode (`dl6 serve` or whatever `src/bin/dl6.rs`
   offers; read it) over the grafana checkout for 5 minutes with a file touch every 30s, sample RSS every
   10s (`ps -o rss= -p <pid>`), and report the series. Acceptance: RSS flat after the first tick (growth
   under 5% over the run) and each re-tick under 2s. If RSS grows, find the holder with the trace
   (`DL_TRACE_SUMMARY`, `RUST_LOG=sprefa_engine_rs=debug`) and write the exact request for `serve.rs`
   (owned by a peer) with file:line; do not edit it. If `apply_daemon_budget` is not applied on this
   path, that is a finding with the site.
6. Gates, each wrapped in `timeout`, pasted: `cd v6/sprefa-engine-rs && cargo test --release` (114 +
   yours: one test per executor against a soopy-built temp repo, no network), `bash grade.sh` (439/335
   rc=0, unmoved), `bash shared-frontier-gate.sh` (8/8), `cd v6 && just oracle-rustc && just oracle-knip`,
   `just crosswalk-gate`, `bash v6/dl/deadcode/dead-module-rail.sh ~/projects/hafley-rs 'crates/*/src/*.rs'`
   (0 dead / 16 unproven / 0 unreachable). Background the batteries.
7. `docs/failure-modes.md`: one entry per incident you hit (not per task).

## File ownership (peers live: ghcacher owns `src/executors/{fetch,env,repos,checkout,toml}.rs` and
`v6/dl/ghcacher/**`; N+1 audit owns `incremental.rs, sql.rs, serve.rs, driver.rs, trace.rs, program.rs`
and extract internals; reach owns `v6/dl/reach/**`; selfdoc owns `v6/dl/selfdoc/**`; compiler owns
`v6/prolog/**`)
YOURS: `v6/dl/crosswalk/**` (new), `src/executors/{git_refs,git_history,repo_at,dep_crawl}.rs` (new),
ONE hunk in `hosts.rs` `executor_for`, `Cargo.toml`/lock, `v6/sprefa-engine-rs/tests/<new>.rs`,
`v6/justfile` ONE appended recipe, `docs/failure-modes.md` append, `issues/` your issue.
FORBIDDEN: everything else. Requests (soopy signatures, serve.rs, registry rows) go in the PR body.

## Report (PR body), tables and lists only
executor table (host / executor / soopy call / spawn count 0); synthetic gate table (rel / identical? /
first diff); grafana fixture table (repo / rev / clone MB / wall); crosswalk matrices (trimmed); watch RSS
series + re-tick walls; gate outputs; requests.
