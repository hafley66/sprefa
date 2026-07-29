# dl — datalog over files in repo/rev/time
# `just` runs from the repo root (also the crate root).

repo := justfile_directory()

# list recipes
default:
    @just --list

# debug build
build:
    cargo build

# release build
release:
    cargo build --release

# run any example by name (without .dl): `just ex callgraph-sg`
ex name="callgraph-ast":
    cargo run --bin dl -- examples/{{name}}.dl --root {{repo}}

# the AST (tree-sitter) call graph
callgraph-ast:
    cargo run --bin dl -- examples/callgraph-ast.dl --root {{repo}}

# the ast-grep call graph
callgraph-sg:
    cargo run --bin dl -- examples/callgraph-sg.dl --root {{repo}}

# openapi coverage: json + regex + anti-join
openapi:
    cargo run --bin dl -- examples/openapi.dl --root {{repo}}

# WORK vs HEAD diff across git revs
time:
    cargo run --bin dl -- examples/time.dl --root {{repo}}

# watch an example live; edit a source file and watch facts re-tick
watch name="callgraph-ast":
    cargo run --bin dl -- examples/{{name}}.dl --db /tmp/dl-{{name}}.db --root {{repo}} --watch

# ast-grep extraction bench (timing + peak RSS), cold then warm
bench prog="bench/rust.dl" root=repo:
    bash "{{repo}}/bench/run.sh" "{{prog}}" "{{root}}"

# v4 linux bench equivalent: count printk() call sites
# local stand-in fixture:
bench-printk:
    bash "{{repo}}/bench/run.sh" "{{repo}}/bench/printk.dl" "{{repo}}/bench/linux-sim"
# real kernel:  just bench-printk-on /path/to/linux
bench-printk-on linux:
    bash "{{repo}}/bench/run.sh" "{{repo}}/bench/printk.dl" "{{linux}}"

crawl-bench:
    nice -n 19 bash "{{repo}}/v6/tsv2/scripts/crawl-bench.sh"

# remove scratch dbs
clean-db:
    rm -f /tmp/dl-*.db* /tmp/dlbench.db*

# ── rust-analyzer oracle ───────────────────────────────────────────────
# WHAT/WHY: sprefa's `call_edge` is a syn-based heuristic (lexical spine +
# light resolution). rust-analyzer resolves names, traits, macros, and impls
# fully, so a SCIP index from RA is the ground-truth oracle. Comparing the two
# quantifies sprefa's noise — edges RA resolves that sprefa misses (undercount)
# and edges sprefa invents that RA doesn't (false positives).
# WHEN: needed once symbol-profile.dl's fan_in/fan_out counts started driving
# refactor decisions (2026-06-26 session). Those counts must be trustworthy
# before they guide a split of engine.rs, so RA is the cross-check.
# RUNTIME: completes in ~48s on a quiet tree. An earlier attempt appeared to
# "hang" — that was RA's internal cargo invocation blocking on the build lock
# while another session held it, NOT a scip problem. Re-run once the lock frees.
# scip flags (unstable per RA): <path>, --output, --exclude-vendored-libraries,
# --num-threads, --config-path. There is NO --only; scope-limit via the cargo
# config or --exclude-vendored-libraries instead.
# RESULT (2026-06-26, src): RA=193 file edges, sprefa=122, shared=55 ->
# recall 28%, precision 45%. Heuristic undercounts (misses trait/re-export/dyn
# paths) AND over-counts. STUDY.md's "prefer resolved, fall back to ast" is the
# remedy; this recipe is how you measure it. Currently FILE-granularity only —
# function-level (validate one fn's fan_out) needs scip_import extended.
oracle-index:
    rust-analyzer scip . --output {{repo}}/index.scip

# run the file-level RA-vs-sprefa comparison. Requires `just oracle-index` first
# (produces {{repo}}/index.scip). --root . (v5) so sprefa's paths (src/…) match
# RA's SCIP paths (also src/…); a repo-root --root yields src/… keys that
# don't join. SPREFA_SCIP_INDEX points at the index outside --root. --no-daemon
# forces the in-process path; a running daemon must never serve a stale cached
# program on a one-shot oracle run (this once manufactured a phantom cycle).
oracle:
    SPREFA_SCIP_INDEX={{repo}}/index.scip cargo run --bin dl -- examples/oracle-check.dl --root . --db /tmp/dl-oracle.db --no-daemon

# x-ray one symbol's full multi-graph neighborhood: callers/callees, fan counts,
# SCC cohort (mutual-recursion mates), forward blast radius, reverse dependents
# (the move-safety set), def span + call sites. Edit the `target(sym)` fact
# (and the two inlined literals in the reachability rules) to change symbol.
# --no-daemon forces the in-process path so a running daemon can't serve stale.
profile:
    cargo run --bin dl -- examples/symbol-profile.dl --root {{repo}} --db /tmp/dl-profile.db --no-daemon

# longest-path topological tiering of the RA oracle file graph. Requires
# `just oracle-index` first. Tier 0 = foundations (depended-on, depend on
# nothing); top tier = entry points with the longest dependency chains. The
# proposed module hierarchy for the refactor. --no-daemon + isolated --db so a
# running daemon can't serve a stale cached program.
dag:
    SPREFA_SCIP_INDEX={{repo}}/index.scip cargo run --bin dl -- examples/dag-layers.dl --root . --db /tmp/dl-dag.db --no-daemon

# the 100%-recall function-level call graph (scip_fn_edge). fn-level fan_out
# ranking, Engine.tick's true callee count, and mutual-recursion clusters.
# Requires `just oracle-index` first. --no-daemon + isolated --db.
fn:
    SPREFA_SCIP_INDEX={{repo}}/index.scip cargo run --bin dl -- examples/fn-graph.dl --root . --db /tmp/dl-fn.db --no-daemon

# feature-envy refactor hints: per fn, which foreign type does it drill into
# most (calls many of that type's methods but isn't itself on it). Read-only
# analysis — surfaces move/extract candidates, does not refactor. Requires
# `just oracle-index` first. --no-daemon + isolated --db.
envy:
    SPREFA_SCIP_INDEX={{repo}}/index.scip cargo run --bin dl -- examples/feature-envy.dl --root . --db /tmp/dl-envy.db --no-daemon

# ── vscode extension (editors/vscode-dl) ─────────────────────────────────────

# compile the extension TypeScript
ext-build:
    cd {{repo}}/editors/vscode-dl && npm run compile

# package the vsix (runs tsc via vsce's prepublish)
ext-package:
    cd {{repo}}/editors/vscode-dl && npm run package

# install the newest vsix into VS Code (reload the window afterwards:
# cmd+shift+p -> "Developer: Reload Window")
ext-install:
    code --install-extension $(ls -t {{repo}}/editors/vscode-dl/*.vsix | head -1)

# the full chain: compile -> package -> install, serialized by a mkdir lock
# (the daemon's effect drain runs requests in parallel; concurrent vsce runs
# would race on the same vsix). `.dl/watch-ext.dl` fires this through the
# daemon whenever an extension source changes.
ext-reload:
    @until mkdir {{repo}}/.ext-reload.lock 2>/dev/null; do sleep 1; done
    -just ext-build ext-package ext-install
    @rmdir {{repo}}/.ext-reload.lock

# ── dev loops (deterministic ceremony → scripts, not agents) ─────────────────

# EVERY test: the full suite, the load-sensitive ones run serially, the rails,
# and a named inventory of what is excluded and why. This is the one to run.
# Exit 0 = nothing in this repo is untested-and-unmentioned.
all-tests:
    bash scripts/all-tests.sh

# full suite with the FSEvents flake re-run policy, then the repo rails
# (magic-rel audit, recompute guard). Exit 0 = verified green.
# Prefer `just all-tests` — this tier alone leaves 28 `#[ignore]`d tests unrun.
verify:
    bash scripts/verify.sh

# regenerate every self-hosted doc (gen-*.dl + README zone splicers) with a
# fresh db each, require second-pass convergence, run the checked-claims rail.
regen-docs:
    bash scripts/regen-docs.sh

# cut a release: verify, changelog gate, scripts/release.sh, commit audit.
# Never pushes — release.sh prints the two manual push commands.
cut version:
    bash scripts/verify.sh
    @awk '/^## \[Unreleased\]/{f=1;next} /^## \[/{f=0} f&&NF' CHANGELOG.md | grep -q . || { echo "CHANGELOG [Unreleased] is empty"; exit 1; }
    bash scripts/release.sh {{version}}
    git show --stat HEAD

# build the small, daemon-free reactivity probe; never runs it
perf-reactivity-build:
    CARGO_BUILD_JOBS=2 DL_RAYON_THREADS=2 cargo build --release --example reactivity_probe

# run only the prebuilt probe against generated repo-local fixtures; never
# builds. `repeats`/`warmup` control the measured-vs-discarded iteration
# count per size (default 5 measured + 1 warmup) — the probe reports
# mean/stdev/min/max per phase over the measured repeats, not a single run.
perf-reactivity out="target/reactivity/probe" repeats="5" warmup="1":
    @test -x "{{repo}}/target/release/examples/reactivity_probe" || { echo "missing release probe: run 'just perf-reactivity-build' explicitly"; exit 2; }
    CARGO_BUILD_JOBS=2 DL_RAYON_THREADS=2 python3 "{{repo}}/bench/reactivity/probe.py" --harness "{{repo}}/target/release/examples/reactivity_probe" --output "{{repo}}/{{out}}" --repeats {{repeats}} --warmup {{warmup}}
