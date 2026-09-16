# Lane brief: dep-crawl manifests read at the pinned rev (issue soopy-depcrawl-manifest)

First action, before anything else, in your worktree:

```bash
git merge --ff-only f7ed05fa434aab8808c5a833a2bd94cb8448aead
```

Failure or missing tree = STOP AND REPORT. Do not work around it, do not
`--no-verify`, do not copy files in from elsewhere.

Read `CLAUDE.md` at the repo root before you edit.

## The defect

The dep crawl has two ways of reading a checkout and they disagree.

| leg | how it reads | file:line |
|---|---|---|
| the crawl's target leg | `CheckoutTrees::read_each`, soopy, pinned to a commit oid | `v6/sprefa-engine-rs/src/dep_resolve.rs:466-494` (`soopy::Revision::Commit` at `:476`) |
| the roster's manifest leg | `std::fs::read_to_string` on the live worktree | `dep_resolve.rs:161`, `:164`, and the two helpers `go_module_path` at `:409-415` and `package_json_name` at `:417-425` |

So a checkout whose `go.mod` has an uncommitted `module` line answers to a
coordinate that exists in nobody's history, while every target read out of that
same checkout comes from `HEAD`. One crawl, two revisions.

## The exact fix

Route the roster's two manifest reads through the same `CheckoutTrees` door, at
the checkout's `HEAD`.

1. `LocalRepoRoster::scan_checkout_root` (`:147-169`) keeps its signature
   `(root: impl AsRef<Path>) -> anyhow::Result<Self>` **unchanged**.
   `v6/sprefa-engine-rs/src/hosts.rs:248` calls it and `hosts.rs` is FORBIDDEN to
   you; a signature change there loses a sibling lane's work.
2. Open ONE `CheckoutTrees` for the whole scan, outside the `for child` loop.
   `CheckoutTrees` is private to this module (`:429`) and caches one
   `soopy::SourceTree` per coordinate, so reusing it across children is the
   point.
3. Per child directory, build the throwaway
   `LocalRepo { coordinate: <dir name>.to_string(), root: child.clone() }`
   (`LocalRepo` at `:79`), ask `CheckoutTrees::head` (`:451`) for the commit oid,
   then `CheckoutTrees::read_each(&repo, &head, &["go.mod".into(), "package.json".into()], visit)`.
4. In `visit`, dispatch on the path the callback hands you: exactly `"go.mod"`
   parses the module path, exactly `"package.json"` parses the name. A pathspec
   can match a nested `sub/go.mod`; a path that is not one of those two literals
   is ignored. Root manifests only, matching today's `child.join("go.mod")`.
5. `go_module_path` and `package_json_name` become pure parsers over `&[u8]`
   (or `&str`), keeping their `Option<String>` returns and their existing
   filters (`!path.is_empty()`, `!name.is_empty()`). They no longer touch the
   filesystem. Do not rename them.
6. **A child that is not a Git checkout keeps its directory-name coordinate and
   nothing more.** `head` and `read_each` return `anyhow::Result`; a failure from
   either is swallowed for that child (the old code swallowed it too, via
   `.ok()?`), and the scan continues. `scan_checkout_root` still only propagates
   the `read_dir` failure at `:150-151`. A checkout root holding one plain
   directory must NOT fail the whole scan.

Nothing outside `LocalRepoRoster::scan_checkout_root` and the two helpers
changes. Do not touch `CheckoutTrees`, `SpecifierFrontier`, `GoModFrontier`,
`DepResolver`, or any relation column list.

## Receipts, required in the commit body

**FAIL-PRE-FIX is required**, and it is the whole point of the arc. In
`v6/sprefa-engine-rs/tests/dep_resolve.rs` (you own that file; append, never
rewrite), add ONE test built on the existing `checkout_root_fixture` shape at
`:320-...`, which already makes one-commit checkouts under a temp root:

- a checkout whose COMMITTED `go.mod` says `module example.com/committed`;
- then overwrite the worktree copy with `module example.com/dirty`, leaving it
  uncommitted;
- `LocalRepoRoster::scan_checkout_root(root)`, then assert
  `locate("example.com/committed").is_some()` and
  `locate("example.com/dirty").is_none()`.

Run that test BEFORE the fix and paste the red output (it fails the other way
round: `dirty` resolves, `committed` does not). Then fix, and paste the green.
The commit body carries both.

Add the `package.json` twin of the same assertion if it costs you one extra
checkout in the same fixture; a single test covering both manifests is fine.

Second assertion, in the same or an adjacent test: a plain non-Git directory
under the checkout root still contributes its directory-name coordinate and does
not make the scan return `Err`.

## Gate, run each leg TWICE, echo rc explicitly, never pipe through `tail`

```bash
cd <your-worktree>/v6/sprefa-engine-rs
cargo build --all-targets; echo "BUILD rc=$?"
cargo test --test dep_resolve; echo "DEP rc=$?"
cargo test --test dep_resolve; echo "DEP rc=$?"
cargo test; echo "CRATE rc=$?"
cargo test; echo "CRATE rc=$?"
cargo fmt --check; echo "FMT rc=$?"
```

Two runs of the same leg must print the SAME pass/fail counts. A leg that moves
between runs is a finding: report it, never pick the green run.

The `#[ignore]`d `a_local_corpus_frontier_closes` (`tests/dep_resolve.rs:280`)
reads `DEP_RESOLVE_CORPUS` and needs a real checkout root. Leave it ignored; do
not try to run it, do not un-ignore it.

The baseline at the base sha is green, so any red is yours.

## File ownership

OWNS, and nothing else:

- `v6/sprefa-engine-rs/src/dep_resolve.rs`
- `v6/sprefa-engine-rs/tests/dep_resolve.rs`

FORBIDDEN, do not open to edit, a live sibling lane owns each:

- `v6/sprefa-engine-rs/src/hosts.rs` (and every other file under
  `v6/sprefa-engine-rs/src/`)
- `v6/sprefa-extract/**` in its entirety, including `src/0_query.rs`,
  `src/project.rs`, `src/types.rs` and every file under
  `v6/sprefa-extract/tests/`
- `v6/tsv2/goldens/scip_combo/**`
- `v6/prolog/**`, `v6/tsv2/**`
- everything outside `v6/sprefa-engine-rs/`

Touching a forbidden file loses both lanes' work.

## Laws that bind you

- Never spawn a subagent. Fan-out is the coordinator's call only.
- No `eprintln!` anywhere under `src/**`; `tracing` only.
- Infra is bought, never built. Soopy is the Git library here. Never hand-roll a
  `git` spawn, never shell out, never add a second manifest reader.
- Comment budget: comments state only constraints the code cannot show. No
  change-log narrative, no dates, no arc or issue references in code comments.
  The `/// Construction touches the filesystem; resolution never does.` header at
  `:132` becomes false in its current wording once the read is rev-pinned; state
  the new constraint in one line or delete it. Do not leave a comment that lies.
- No em dashes. Banned words in prose AND identifiers: provenance, substrate,
  load-bearing, regime.
- Surrogate-key and N+1 laws still apply if you touch anything storage-shaped.
  You should not need to.
- `pwd` before every `git commit`; commit ONLY in your worktree, never pipe a
  commit, use `COMMENT_RAIL_IDLE_MS=3000 git commit ...`.
- Commit trailer, required: `Refs-Issue: @soopy-depcrawl-manifest`.

## Landing

1. Your branch is already `fix/soopy-depcrawl-manifest`.
2. Commit with the FAIL-PRE-FIX red output and the gate receipts (rc lines, both
   runs of each leg) in the body.
3. `git push -u origin fix/soopy-depcrawl-manifest`.
4. `gh pr create` with a body carrying: the two-revision defect statement with
   its `dep_resolve.rs` line cites, the fail-pre-fix red, the receipts table
   (leg, run 1, run 2).
5. **Never merge.** Do not `gh pr merge`. The coordinator lands it.
6. Before you report done: `git log --oneline -3` and `git status` in your
   worktree, and paste both. An uncommitted deliverable is an undelivered one.
