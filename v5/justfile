# v5 (dl) — datalog over files in repo/rev/time
# `just` runs from this directory. `repo` is the sprefa root (parent of v5).

repo := justfile_directory() / ".."

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
    bash bench/run.sh {{prog}} {{root}}

# v4 linux bench equivalent: count printk() call sites
# local stand-in fixture:
bench-printk:
    bash bench/run.sh bench/printk.dl bench/linux-sim
# real kernel:  just bench-printk-on /path/to/linux
bench-printk-on linux:
    bash bench/run.sh bench/printk.dl {{linux}}

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
# RESULT (2026-06-26, v5/src): RA=193 file edges, sprefa=122, shared=55 ->
# recall 28%, precision 45%. Heuristic undercounts (misses trait/re-export/dyn
# paths) AND over-counts. STUDY.md's "prefer resolved, fall back to ast" is the
# remedy; this recipe is how you measure it. Currently FILE-granularity only —
# function-level (validate one fn's fan_out) needs scip_import extended.
oracle-index:
    rust-analyzer scip . --output {{repo}}/index.scip

# run the file-level RA-vs-sprefa comparison. Requires `just oracle-index` first
# (produces {{repo}}/index.scip). --root . (v5) so sprefa's paths (src/…) match
# RA's SCIP paths (also src/…); a repo-root --root yields v5/src/… keys that
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
