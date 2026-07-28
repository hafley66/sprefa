# v5 repo/rev receipts for v6 stdlib-spine design

Collected 2026-07-27. Read-only survey of the v5 rust engine (repo root:
src/, .dl/, examples/, docs/, bench/, chat_log/). Every claim cites
file:line. No interpretation beyond what the cited line states.

## 1. The scan op

| claim | file:line | receipt |
|---|---|---|
| `scan` surface form, 2-ary/5-ary, repo default `.`, rev default WORK | `docs/reference/syntax.md:22` | `scan([repo,][rev,] glob, path[, rev_out])` table row |
| `WORK` is a literal-text ALIAS, never stored; stored revs match `^[0-9a-f]{40}\+?$` | `src/engine/revid.rs:1-11` | module doc, INV-1 |
| alias funnel: every scan rev literal resolves through one fn | `src/engine/repo.rs:13-19` | `resolve_rev`: `if rev == WORK_ALIAS { self.worktree_rev.resolve(...) }` |
| non-alias rev resolves via `git rev-parse`, cached (cross-tick for immutable sha, per-tick for movable ref) | `src/engine/repo.rs:20-35` | two-cache lookup then `Self::rev_parse` |
| miss triggers escalating on-demand fetch ladder (full-sha, tag, all-tags, unshallow) | `src/engine/repo.rs:44-79` | 3-rung ladder + shallow-deepen fallback |
| `DL_NO_FETCH` makes a miss a hard error instead of a fetch | `src/engine/repo.rs:40-43` | offline guard |
| WORK resolution: HEAD oid + dirty flag (`git diff-index --quiet`, then `git ls-files --others`) | `src/engine/revid.rs:218-237` | `probe_dirty`, cheapest-first probes |
| repo coordinate resolves to `(slug, root)`; `.`/`""`/`self` = own repo, else config slug, else existing path | `src/engine/repo.rs:185-209` | `resolve_repo` |
| `scan("*", ...)` / `scan("all", ...)` fans out over every configured repo, cloning missing ones | `src/engine/repo.rs:265-284` | `resolve_scan_repos` |
| data-driven scan: repo/rev as `Term::Var`, bound from a body atom compiled to SQL over LAST tick's tables (one-tick latency) | `src/engine/repo.rs:286-417` | `resolve_scan_bindings`, doc comment 286-295 |
| self-form scan under rootless daemon resolves to the rule's own `.git` ancestor via `nearest_git`; foreground/LSP roots win instead | `src/engine/repo.rs:314-330` | branch on `self.root_implicit` |
| WORK-arm walk: `ignore::WalkBuilder`, skips `.git` and nested `.git` dirs (submodules), blake3-hashes changed files, mtime+size fast path with a racy-write second-resolution guard | `src/engine/scan.rs:127-223` | `enumerate_with_hash`, doc 17-55 |
| non-WORK arm: `git ls-tree -r -l <rev>`, blob oid IS the digest, nothing hashed, line count left `-1` | `src/engine/scan.rs:224-265` | comment "A git rev reads blob oids... nothing is hashed here" |
| pinned-rev content read: `git cat-file --batch` via a persistent per-root subprocess (`GitBatch`), keyed `<rev>:./<path>` (the `./` form survives a scan root nested under the repo root) | `src/engine/repo.rs:1143-1238` | `GitBatch`, `git_batch_read` |
| a scan root outside any git work tree falls back to the filesystem for both walk and read | `src/engine/repo.rs:1196-1218`, `src/engine/mod.rs:1209-1220` | `root_is_inside_work_tree`, `read_content` |

## 2. The repo registry

| claim | file:line | receipt |
|---|---|---|
| repo list today is BOTH config (TOML) and a derivable rel — `repo` is one of 5 builtin rels | `src/engine/mod.rs:269` | `const BUILTIN_RELS: [&str; 5] = ["repo", "rev", "content", "file", "true"]` |
| config file: `[[repos]]` (slug/root/url/allow_missing), `[[org]]` (dir of checkouts, auto-expand, max_depth, foldername/flatten), `default_org` prefix | `src/config.rs:1-97` | module doc + `RepoConfig`/`OrgConfig`/`SprfConfig` structs |
| the config doc's own multi-repo example is a grafana org folder | `src/config.rs:41`, `README.md:844` | `dir = "~/orgs/grafana"` |
| **a program CAN head the repo list**: a rule whose head is `repo(slug, root, url)` is a dynamic-pull sink, drained post-fixpoint | `src/ast.rs:597` | `is_repo_sink(&self) -> bool { self.head.rel == "repo" }` |
| repo-sink drain: SELECT the body, for each row clone-if-missing (unless a github org allowlist blocks it) and register into `self.repos`; explicit ground facts bypass the allowlist | `src/engine/repo.rs:448-592` | `run_repo_pulls` |
| org allowlist is `org(name)` fact set; a row's org comes from `parse_github_org(url)` (github https/http/ssh only) | `src/engine/repo.rs:464-476`, `979-1004` | allowlist build + `parse_github_org` |
| registered repos land in `repo` the NEXT tick (idempotent re-derivation) | `src/engine/repo.rs:452-453` | doc comment on `run_repo_pulls` |
| test proving the whole loop end to end, no network (roots pre-created) | `tests/it/repo_sink.rs:1-40` | `repo(slug, root, url) <- candidate(slug, root, url).` two-tick drive |
| registered set persists to `_repo` (slug, root, url, registered_at), wiped+reinserted each save | `src/engine/repo.rs:419-440` | `save_repos_meta` |
| query-facing `repo` relation reads back from the `repo` table (post `refresh_builtin_rels`) for diagnostics/tests | `src/engine/rpc.rs:430-445` | `Engine::repo_relation` |
| OG-archive prior art: v4's config.rs is the direct ancestor, v5 "trimmed to the `repos` table v5 needs" | `src/config.rs:1-2` | module doc line 1-2 |
| `checkout` is a second, separate sink (`checkout(repo, branch, pr_heads)`) that fetches + fast-forwards NON-DESTRUCTIVELY (never stash/reset --hard); min-interval gate `DL_CHECKOUT_MIN_SECS` (default 300s), dedicated 2-wide rayon pool `DL_CHECKOUT_WIDTH` | `src/ast.rs:598`, `src/engine/repo.rs:594-977` | `checkout_min_secs`, `checkout_pool`, `checkout_one` |
| `roots.json` is a DIFFERENT registry: daemon-served project roots (multi-project daemon), not the multi-repo-within-one-program config above | `src/daemon/home.rs:112-114`, `src/daemon/mod.rs:22` | `daemon_home().join("roots.json")` |

Answer to "config-only or headable": **headable**. `repo` is a builtin rel
like any source rel, and a plain derived rule with head `repo(...)` is
recognized (`is_repo_sink`) and drained by the engine — a program can derive
its own repo set from any other rel (an `org` scan, a manifest, an API
response) rather than hand-listing `[[repos]]`.

## 3. Rev extraction

Covered in table 1 (scan.rs / repo.rs / revid.rs are one seam). Content
addressing: WORK files are blake3-hashed (`src/engine/scan.rs:214`); git-rev
files use the blob oid from `ls-tree` as the identity, never re-hashed
(`src/engine/scan.rs:245,263`). Cache key per (repo, path, rev) is the
`_file` table row, probed by the RESOLVED rev text — never the alias — so a
key collision across ticks cannot happen (`src/engine/scan.rs:188-193`).
Ref-observation + rev-log: `observe_ref` resolves a name to an oid, diffs
against `_ref` (last-seen), appends one `_rev_log` row on change
(`src/engine/repo.rs:1006-1061`). `files_changed_between` intersects `git
diff --name-only old new` with the tracked `_file` path set per repo
(`src/engine/repo.rs:1063-1105`).

## 4. Watchers

The daemon runs one `notify::recommended_watcher` (FSEvents on macOS,
inotify on Linux, ReadDirectoryChangesW on Windows via the `notify` crate's
platform backend) per served root, spawned as a tokio task
(`src/daemon/shell/watch.rs:1-40`; task doc: "the notify crate stays
callback/thread-based... only the engine ops run via spawn_blocking").
`.git` is watched narrowly and separately (`watch_git_narrow`,
`src/daemon/shell/watch.rs:267-284`), and every repo in the engine's
resolved corpus (config + dynamically-pulled) gets its own recursive watch
added on top of the served root (`src/daemon/shell/watch.rs:50-52,73-75`),
so a repo pulled mid-run via the `repo` sink becomes watched without a
restart. One-shot / `--no-daemon` runs install no watcher at all — scanning
is a single walk, no tail. The IO event trail's `file_changed` kind fires
from `ServedRoot::tick_paths`, which writes the full changed-path list (not
a count) before the daemon lock is taken, replacing the old "15 changed
path(s)" black box (`src/daemon/root.rs:170-193`; trail doc
`src/eventlog.rs:1-24`). No cross-platform caveats found in-tree beyond the
`notify::recommended_watcher` platform dispatch itself.

## 5. Multirepo bench receipts

**No in-tree grafana crawl benchmark file exists today** (nothing under
`bench/`, `examples/`, or `tests/` scans the actual grafana org). The
measured run was external/one-off; the numbers survive only in a memory doc
and one chat_log session, both cited below. `bench/` does hold smaller
fixed multi-repo corpora (`bench/corpus/otel-{go,rust,python,js,kotlin}`,
`bench/seams/corpus/{payments,docs,mobile,gateway}`) but none is a grafana-
scale crawl.

| claim | file:line | receipt |
|---|---|---|
| org-scale bench memory doc, corpus + numbers | `.agents/memory/project_org_scale_bench.md:1-104` | full arc write-up |
| corpus: grafana(389)+hashicorp(411)=800 repos, `gh repo list` + `50 < diskUsage < 50000 KB` filter, shallow `--depth 1` | `.agents/memory/project_org_scale_bench.md:14-22` | corpus section |
| org-fan scan: `scan(r, "HEAD", glob, p, rev)` joined on `repo(r,_,_)` — **42,739 files across 389 grafana repos in 5.9s cold, RAM flat** (go 31662 / ts 7602 / tsx 3475) | `.agents/memory/project_org_scale_bench.md:37-42` | "Org fan pattern" paragraph |
| manifest-first cross-repo seam graph: go.mod `module`/`require` lines via `match_line`, closure for transitive deps — **110 hubs over 800 repos in ~10s, no compile/toolchain**; top hubs hashicorp/go-hclog 187, go-multierror 149, yamux 138, grafana/grafana-plugin-sdk-go 40 | `.agents/memory/project_org_scale_bench.md:24-35` | "THE KEY INSIGHT" paragraph |
| real SCIP indexing measured on one repo only (scip-typescript 67KB/76ms); 600-repo real SCIP explicitly NOT run ("hours + partial failures") | `.agents/memory/project_org_scale_bench.md:44-49` | SCIP section |
| pin-skew cross-org validation: after a shallow-clone guard, 120 real stale pins (grafana repos pinning hashicorp/go-retryablehttp), 0 false diverged, 218 loud skips | `chat_log/20260701.4.dl-want-tier-demand-builtins-pin-skew.md:52-56` | corpus-smoke paragraph |
| session narrative for the same arc (task list, commit 8111515) | `chat_log/20260701.4.dl-want-tier-demand-builtins-pin-skew.md:58-144` | tasks + turn log |
| the resulting `.dl` programs are in-tree today | `examples/pin-skew.dl:1-48` | module doc names the exact chain (go.mod → pin → rev_behind) |
| npm-crawl: self-seeding corpus (no pre-clone, no config.toml), BFS via `@stream` effect over the npm registry; measured express@depth1=81 edges, cross-spawn@depth2=5 edges | `.agents/memory/project_org_scale_bench.md:89-104` | "Self-seeding corpus" paragraph |
| npm-crawl driver + program in-tree | `examples/npm-crawl.dl`, `examples/crawl` | README-documented (`README.md:1013,1234-1255`) |
| "now a bench scaling factor" — `bench/org/` intended to sweep repo count N reporting wall+RSS | `.agents/memory/project_org_scale_bench.md:74-76` | stated as a plan; **`bench/org/` does not exist in the current tree** (verified via `find`) |
| perf harness that DOES exist: `bench/scip_perf.sh` + `bench/scip_perf_results.md` (single-repo SCIP timing, not multirepo) | `bench/scip_perf.sh`, `bench/scip_perf_results.md` | file presence only, not read in depth this pass |

## 6. gh-cache.dl (v5 twin of v6 ghcacher)

| claim | file:line | receipt |
|---|---|---|
| two clocks distinguished: `DL_POLL_SECS` (daemon retick cadence, free) vs `clock(300,b)` (actual re-fetch cadence, the rate cap) | `examples/gh-cache.dl:23-28` | RATE SAFETY comment |
| clock-bucket digest salt, named form and wildcard form | `examples/gh-cache.dl:45-66` | `poll(ep, prev, b) <- watch(ep), etag(ep, prev), clock(300, b).`; wildcard variant discussed 58-63 |
| latest-wins reduction: `resp_latest(ep, max(b))` then `resp_current` joins it back, so etag carry stays single-valued despite `resp` accumulating every bucket's response | `examples/gh-cache.dl:92-104` | comment + 3 rules |
| pr_number -> change_log carry split (the hazard this repo's style notes call out): `pull_request` derives per-tick from `resp_current`; `change_log`/`change_log_next` is the separate `@next`-accumulated append-only rel it feeds | `examples/gh-cache.dl:120-137` | `pull_request` rule + `change_log_next` union |
| checkout half (clone/fetch/ff) is a SEPARATE file, not covered by gh-cache.dl | `examples/gh-cache.dl:19-21` | module doc, points to `examples/gh-checkout.dl` |
| companion fixture-driven parity test against the real ghcacher | `tests/it/ghcacher_parity.rs`, `bench/ghcacher_vs_dl.sh`, `tests/.fixtures/gh-cache/*.resp` | file presence, ~90 canned `.resp` fixtures |

## Notes on scope not covered

Did not re-derive numbers, did not run any bench, did not open
`bench/scip_perf_results.md` in depth, did not audit `.dl/rails.dl`'s
`named_call_site` duplication (already tracked in project CLAUDE.md). Did
not check `.claude/worktrees/*` copies (excluded as non-canonical).
