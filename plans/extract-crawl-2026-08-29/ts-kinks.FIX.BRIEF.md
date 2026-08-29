# Brief: TypeScript crawl kinks (lane `fix-extract-ts-crawl`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law).
Findings: `plans/extract-crawl-2026-08-29/ts5.REPORT.md` section 6 (this
branch carries it once PR from `crawl/extract-typescript-5` merges; until
then read it at
`/Users/chrishafley/projects/sprefa/.boop-worktrees/crawl/extract-typescript-5/plans/extract-crawl-2026-08-29/ts5.REPORT.md`)
and the fixtures under `tests/fixtures/ts5_findings/` there (copy the ones
you use into your tree under the same path).

## First action
```
git merge --ff-only c60e5c4cc
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-ts-crawl sprefa-coordinator "<one line>"`.

## Files you own
`v6/sprefa-extract/src/lang/ts.rs`, `src/lang/ts_resolve.rs`,
`src/lang/ts_paths.rs`, new `tests/53_ts_crawl_kinks.rs`,
`tests/fixtures/ts5_findings/**`, `tests/fixtures/ts/*.v5.jsonl` (only
with a stated reason). Forbidden: every other file. In particular
`src/project.rs` (`caller_name` closure folding is NOT yours; the rust
lane credits closures on its own arm, and a generic fold is a later
decision) and `src/lang/rust.rs`, `src/lang/go.rs` (other lanes own them).

## Kinks, in order
1. A top-level statement's call site has no covering def and is DROPPED
   from `--resolve` (1,358 sites in `src/**`; `src/tsc/tsc.ts` has 8
   sites and 0 edges, so the crawl cannot even leave the entrypoint).
   Fix: give module-level sites the module itself as caller (a synthetic
   `<module>` def spanning the file, the shape the python arm already
   mints, see `lang/python/_0_source.rs` `<module>` Ext entity), so the
   edge exists with `caller_name: "<module>"`. Fixture
   `ts5_findings/top_level_callee.ts`.
2. `export * from` barrels are not followed: `forEachChild` through
   `./_namespaces/ts.js` stays ambiguous (1,241 of 11,768 ambiguous sites
   narrow to one def once the barrel is followed). Fix in the ts resolve
   arm: when a name is imported from a module whose file is a barrel
   (`export * from` rows in its specifiers), follow the re-export chain
   (bounded depth, cycle-safe) to the file that declares the name, and
   prefer that def over name-only candidates.
3. Receiver-blind name match mints wrong edges: `(x).push(...)` binds to
   `tracing.ts:push` (2,064 edges), `fn.bind(...)` to `binder.ts:bind`.
   Fix: a site whose callee is a member expression on a receiver that is
   NOT an imported/local module binding must not name-match against a
   free function def of the same name; keep it as unresolved with reason
   `member-call-unknown-receiver`. Fixture `ts5_findings/receiver_blind*`
   (also `tests/fixtures/ts_findings/receiver_blind_method/` from PR #538).
4. A function named as a value (`transformers.push(transformES2015)`)
   is not a call site; 5 of the 10 largest unreachable defs. Fix: emit a
   `reference` row (`family: call`, `position: value`) for an identifier
   argument that names a corpus def, without minting a site; and in resolve
   emit an edge with `kind: value_ref` from the enclosing def. Check the
   schema line for `reference` first; if `position` values are a closed
   set in `src/types.rs`, add `Value` there (you may touch `types.rs` for
   that one variant only) and state it.
5. `--scip-facts` requires a git worktree and the help does not say so:
   ONE line in `help.rs` `SCIP_FACTS_LONG` (you may edit that constant only).
Out of scope, report only: the exported flag on `node` rows (wire change),
scip-typescript's 1 MB document drop.

## Method
Failing test first per kink, red pasted in the commit body, one commit
each. Gate `cargo test --features cli --no-fail-fast`, SUM over binaries.
Golden regen only by `tests/6_kind_vocab.rs`'s procedure, hunk count stated.
No whole-crate `cargo fmt`. No subagents.
Receipt: rerun `plans/extract-crawl-2026-08-29/ts5.crawl.py` (from the
crawl worktree path above) over `~/projects/TypeScript-5.9/src` with your
binary: resolved_edge and A_strict reachability before/after (before:
75,089 edges, 3,509 of 14,047 reachable) in a Fixes table appended to
ts5.REPORT.md.
Then push, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-ts-crawl sprefa-coordinator "ts kinks: PR #N, edges <b>-><a>, reachable <b>-><a>, gate <p>/<f>"`.
