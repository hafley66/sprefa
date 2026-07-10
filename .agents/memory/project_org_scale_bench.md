---
name: project_org_scale_bench
description: 800-repo org corpus (grafana+hashicorp) + manifest-first cross-repo seam graph; now a bench scaling factor (repo count). Manifests discover seams BEFORE SCIP.
metadata: 
  node_type: memory
  type: project
  originSessionId: 33b32503-a5e3-4068-bf31-2f4638b4145e
---

Org-scale validation arc (2026-07-01). Goal: scan a ~600-repo org, run large
algorithms, validate cross-repo dataflows with multi-pass AI. Extends
[[project_scip_turnkey_xrepo]] and [[project_v5_dl_engine]].

**Corpus (external, not in repo — 4GB):** grafana (389) + hashicorp (411) = 800
real-source repos at `~/orgs/{grafana,hashicorp}/repos/` (shallow `--depth 1`).
Filter that picked them: `gh repo list ORG --no-archived --source`, jq keep
`isFork==false and 50 < diskUsage < 50000` (KB) — the <50MB band drops
generated/test-data blobs (median ~1MB = real small source). Combined config
`~/orgs/all.config.toml` (800 `[[repos]]` slug+root). Rebuild via the manifests
`~/orgs/grafana/manifest*.txt` + the clone script pattern (xargs -P8, wrap each
child to `exit 0` — BSD xargs ABORTS the whole batch if any child returns 255,
which `gh` does on empty/odd repos; that truncated the first run at 85/389).

**THE KEY INSIGHT — manifests give the cross-repo seams for FREE, before SCIP.**
`go.mod` (and package.json/lock/Cargo.toml) DECLARE the inter-repo dep graph.
Pure-dl program (`bench/org/xrepo-go.dl`): `module_id(repo,module)` from the
go.mod `module` line via `match(p,rev,/^module\s+(?<mod>\S+)/,l)`;
`dep(repo,module)` from require lines `match(/(?<mod>\S+\/\S+)\s+v[0-9]/,l)`;
internal edge `repo_dep(a,b) <- dep(a,m), module_id(b,m), a!=b`; closure for
transitive; `count` for fan-in hubs. **110 hubs over 800 repos in ~10s, no
compile/toolchain.** Top: hashicorp/go-hclog 187, go-multierror 149, yamux 138;
grafana/grafana-plugin-sdk-go 40. This INVERTS the pipeline: manifests → seam
skeleton (cheap) → SCIP only ON the seams (expensive, targeted) → AI voting only
on SCIP-resolved cross-repo flows (tiniest set). Each layer ~10x smaller + ~10x
costlier. Don't index 800 repos to FIND seams.

**Org fan pattern:** `scan(r, "HEAD", glob, p, rev)` with `repo(r,_,_)` binding r
is the data-driven multi-repo fan (a bare scan-headed rule that joins `file()`
for repo FAILS: "head var r unbound in source rule" — source rules bind only
from the source op; put the var in scan's repo slot instead). 42,739 files
across 389 grafana repos in 5.9s cold, RAM FLAT — proves the disk-backed/SQLite
choice scales where DD (resident arrangements) can't. go 31662 / ts 7602 / tsx 3475.

**Real SCIP → dl proven** on one repo: `scip-typescript index` (67KB, 76ms, NO
npm install) → dl loads `index.scip` at --root → scip_def=117, scip_ref=85 real
monikers. Toolchains: scip-typescript/scip-python PRESENT, scip-go MISSING (`go`
present → `GOBIN=~/.cargo/bin go install .../scip-go@latest`). Real SCIP over 600
= hours + partial failures (scip-go compiles + `go mod download`; scip-typescript
wants node_modules) — lazy+cache is the mitigation, not optional.

**Lazy `scip_want` design (NOT built):** opt-in built-in the program derives
(`scip_want(r) <- repo(r,_,_), lang(r,"go").`) → engine gate → per repo ensure
`.dl/index.scip` (reuse `scip_setup.rs` runner, rev-cached via ScipKind.dirty
which already keys on index.scip) → merge (reuse `merge_files`) → load existing
`scip_def/ref/edge`. **NO schema change** — SCIP monikers carry module/package
path so a merged multi-repo index self-disambiguates (don't repo-key the rels,
it breaks flow_scip.dl + oracles). Contained: a gate + a loop over existing fns.

**Progressive rev loading = ghcacher (PROVEN).** `progressive-revs.dl`: `@async`
`sh fetch_rev` effect resolves short→full sha (`gh api commits/<sha>`) + `git
fetch --depth 1`, headed off `scan_target`. Closed the loop: 12/14 dskit revs
fetched. HARD-WON DRIVERS/GOTCHAS (now in v5/README.md "Sharp edges" section):
(1) effects only drain under the PERSISTENT daemon — `dl --daemon --root X`
(bg) + `dl --load prog --root X`; a plain one-shot / `--no-daemon` does NOT
drain (effect_log empty), and `dl --lsp prog </dev/null` dies "disconnected
channel" (LSP needs live stdin). (2) `@async` fans out ALL distinct coordinates
on one tick → external secondary rate limit → mass fail (1/14); `clock(N,b)`
re-fires EACH coordinate per bucket (retry), it does NOT stagger distinct
requests — a bare clock join re-bursts. FIX = jitter in the effect body
(`sleep $((RANDOM%25))`) to desync (1/14 → 12/14), and/or presence-gate.
(3) content-addressed effect id = (head,kind,args) NOT the shell template —
editing the backtick body won't re-fire; use a fresh `--db`.

**Now a bench scaling factor:** `bench/org/` sweeps repo count N (the new axis
beyond single-repo), running the fan + cross-repo graph, reporting wall + peak
RSS per N. Expected: time ~linear in files, RAM flat (the whole point).

**THE DRAIN GATE — FIXED in v0.2.1 (2026-07-01, commit 71a8df6, pushed main +
tag).** WAS: effects didn't fire under `dl --daemon` alone — queue sat at
`state='queued'` forever because the drain (in `daemon.rs::poll_tick`) was gated
on env `DL_POLL_SECS`, default UNSET = no poll. NOW: `poll_interval_secs()`
defaults to `DEFAULT_POLL_SECS=2` when unset (drains by default); `poll_loop`
cheap-gates on `async_effect_arity(prog).is_empty()` so an effect-free daemon
pays nothing; `DL_POLL_SECS=N` overrides cadence, `DL_POLL_SECS=0` = explicit off.
So a bare `dl --daemon --root X` + `dl --load prog --root X` now fires effects.
Also v0.2.1: `head var unbound in source rule` now prints the scan-repo-slot fix,
and `examples/npm-crawl.dl` + `examples/crawl` (see below) shipped.

**Self-seeding corpus = npm-crawl (2026-07-01, local uncommitted).** No pre-clone
+ no config.toml: name ONE public npm package, the `@stream` effect crawls its
dep graph straight from the registry. `examples/npm-crawl.dl`: `sh* npm_deps(pkg)
-> (dep,range) = curl registry/{pkg}/latest | jq to_entries` (content-addressed →
each pkg fetched once); `dep_at(pkg,dep,d) <- @stream frontier(pkg,d), npm_deps`;
frontier expands one BFS layer/tick `frontier(dep,d1) <- dep_at(_,dep,d), d<3,
d1=d+1`; `gen("_npm/graph.d2","{pkg} -> {dep}")` rewrites progressively; optional
`@async` shallow-pull `git clone --depth 1` (source only, NO npm install/build).
Driver `examples/crawl <pkg> [depth]` owns daemon+DL_POLL_SECS+load+fixpoint-
wait+render as ONE command. Fixpoint detect = edge count stable across 3 polls (2
isn't enough — a BFS layer takes a poll cycle, brief inter-layer plateau false-
triggers). PROVEN: express@d1=81 edges + fanin hubs (debug←5) + all repos pulled
~; cross-spawn@d2=5 edges (which→isexe, shebang-command→shebang-regex). Same
manifest-first shape as go.mod but manifest=registry, crawl=progressive. Scoped
`@scope/name` needs `%2F` URL-encode (template passes slash raw; unscoped works).
Output `examples/_npm/` gitignored (v5/.gitignore, path rel to v5/).
