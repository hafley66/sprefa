# dep-resolve-recursion (issue: dependency-resolve-recursion, size:med, priority:high)

FIRST ACTION: `git merge --ff-only 046cbc510804671d2441aca36536bbd310eef485`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. Issue (full body, read it):
/Users/chrishafley/projects/sprefa/issues/dependency-resolve-recursion/item.md

GOAL: the recursive dependency crawl closes its frontier. Works today:
repo/rev/file -> source bytes -> source_specifier(owner span, target, binding,
kind) -> dependency row. You build the next leg: dependency target ->
package/module resolved to repository coordinates -> LOCATE the repository
locally -> select revision -> enumerate its files -> repeat to closure.

SCOPE RAIL (respect it exactly): LOCAL LOCATE ONLY. Network acquisition
(cloning absent repos) is the open issue remote-acquisition-policy and is the
USER's policy call — your resolver takes a roster of locally present repos
(e.g. a root dir like ~/orgs/<org>/<repo> plus the current checkout set) and
resolves targets against it. A target that resolves to coordinates with no
local copy emits a NAMED fact row (e.g. dep_unresolved(target, reason)), never
a network call, never a silent drop.

WHERE (from the issue, keep it): new module
v6/sprefa-engine-rs/src/dep_resolve.rs, a sibling concern module — NOT a
source_bind grow. SourceBind _1_runtime.rs emits specifier rows; you walk them
to repo coordinates. Wire a public entry per the crate's existing module
conventions (read src/lib.rs and follow it).

TERMINATION (the issue names it, treat as a gate): the frontier MUST close.
Visited-set on (repo, rev); a cycle or an unbounded chain terminates with the
visited set as the receipt. Write the termination test FIRST (a synthetic
cycle: A depends on B depends on A) and show it failing/hanging is impossible
by construction (bounded iterations = repo count).

VALIDATION (paste outputs, each leg twice):
1. `cargo test -p sprefa-engine-rs` green including your new tests (cycle
   termination + a 2-hop resolve + an unresolved-target named row).
2. `cd v6 && just crawl-bench` — runs clean under nice; paste before/after
   stmts/tick and wall so drift is visible (comparable corpus, cap 8).
3. `cd v6 && just multirepo-golden` — unchanged vs base.
4. `bash v6/sprefa-engine-rs/grade.sh` from your worktree root IF it runs
   there; if it fails on the ../sprefa-v6 sibling path (known worktree
   breakage), say so and cite the diff-scope proof instead (your change is
   engine-rs-only; no emitter or compile/out files in the diff).

BUILD-VS-BUY note: version/specifier parsing (semver, go mod paths, npm
specifiers) — check what the crate already depends on before adding any
parser; if a new dep is needed, one paragraph of candidates in the commit
message, no bespoke version parser without it.

FILES YOU OWN: v6/sprefa-engine-rs/src/dep_resolve.rs (new), the one mod-wiring
line in lib.rs, additive tests under v6/sprefa-engine-rs/tests/.
FORBIDDEN: SourceBind runtime beyond READING its rows, v6/prolog/**, v6/tsv2/**,
extractor src/lang/**.

COMMIT plain, COMMENT_RAIL_IDLE_MS=3000, never pipe a commit, commit ONLY in
your worktree (`pwd` before every git commit). No eprintln; tracing only.
Close: `issuectl --json close dependency-resolve-recursion --commit <sha>:<summary>`.
Report: frontier-closure receipt on a real local corpus (name it), the four
gate numbers, unresolved-target row counts.
