# Brief: executors are modules you `use`; bare names in programs; dot and slash paths are aliases

Base sha: the spawner prints it (main 1e39b557874ff790b2a2d8d0591e134216eda558).

## The user's decisions (2026-08-22, in order, latest wins)
1. "Put the dots back; keep the slashes as an alias for dot; never use them."
2. "Don't require dots either. You should be able to import things with an alias or by
   module name."
So: an executor family (`soopy`, `http`, `extract`, `scip`, `clock`, `toml`,
`env`, `dl`, `cargo`) is a MODULE. A program writes `use soopy.` and then
`files(glob: key(text)) -> (...)` bare, or `use soopy as s.` and `s.files(...)`. The
in-file path forms `rel soopy.files(...)` and `rel /soopy/files(...)` keep parsing as
aliases of the same thing and no file in this repo writes either one after this arc.

## How it works today (verified, cite these in your PR)
- `use "x.dl6"` = dependency edge, child rels arrive flat. `use "x.dl6" as a` adds a
  `mount_decl(a, Child, Owner, Paths)` (`v6/prolog/use_resolve.pl:100-117`, ruling
  `mount_alias_additive`); `a.rel(...)` resolves to the child's own flat name by
  identity in `v6/prolog/0_dot_expand.pl:252-262` `declared_path/3`. No rel is minted.
- An in-file dotted or slash rel (`rel soopy.files`, `rel /soopy/files`) is
  `rel_path_decl(soopy__files/N, [soopy,files])` from `parse_dl_dcg.pl:758,761`
  (`module_path_name/2`, `__` join; `dotted_path//1` at `:1599-1613` takes both
  separators). That `soopy__files` atom is the engine's executor key
  (`v6/sprefa-engine-rs/src/hosts.rs:81` `executor_for`, roster string `:55-64`;
  registry roster `v6/prolog/compile/registry.pl:308-560`).
- Rulings: `rulings.pl:840` `executor_namespacing`, `:855` `executor_path_slashes`.

## Build this
1. `use <module>.` and `use <module> as <alias>.` where `<module>` is a bare ident
   naming an executor family in the registry roster (no quotes; a quoted string stays a
   file). Grammar in `parse_dl_dcg.pl` `use_item//1` (`:496`). Resolution: the
   roster's families become mountable modules whose Paths are `[leaf]-<family>__<leaf>`
   for every roster rel; reuse `mount_decl` so `0_dot_expand.pl` needs no new clause for
   the aliased form. For the unaliased form, a bare `files(...)` in a program that
   `use soopy.` resolves to `soopy__files` ONLY when the program declares no rel named
   `files` itself (a program's own rel wins; document that in the ruling). Two
   executor modules exporting the same leaf, both used unaliased = compile stop
   `ambiguous_executor_leaf(Leaf, [ModA, ModB])`, fixed by aliasing one.
2. The rel DECLARATION also goes through the module: a program declares
   `rel files(glob: key(text)) -> (path: text, digest: text).` after `use soopy.`
   and the declaration binds to the executor. Keep every downstream atom identical
   (`soopy__files`) so `hosts.rs`, emitted SQL names, adapters sidecars and every
   golden stay byte-identical. Your PR proves that: `grade.sh` byte-clean count does
   not move and `just ghcacher-rust` goldens=6 hold without regenerating any golden.
3. Rewrite every `.dl6` in the repo that declares `rel /x/y(` or `rel x.y(` to the
   `use` form with bare names. `grep -rlE '^rel /' --include=*.dl6 .` = 47 files, 112
   declarations at base. EXCEPT these, which another lane owns and the coordinator
   re-spells after both land: `v6/dl/ghcache/**`, `v6/dl/prwatch/**`,
   `v6/dl/ghcacher/**`, `v6/dl/fixtures/ghcacher*.dl6`, `v6/dl/fixtures/crawl_org.dl6`.
   Never touch `chat_log/**`, `plans/**`, `issues/**`.
4. Ratchet, additive: a plunit test in `v6/prolog/compile/test/plunit_tests.pl` walking
   every `.dl6` outside `chat_log/ plans/ issues/` and archive dirs, failing on any
   `^\s*rel\s+(/|[a-z_]+\.)`. One conformance fixture per alias proving byte-identical
   emit for the three spellings of one program (`use soopy.` bare, `rel soopy.files`,
   `rel /soopy/files`).
5. `rulings.pl`: replace the `executor_path_slashes` row and comment (`:850-856`)
   with `executor_modules_use_import` carrying decisions 1 and 2 verbatim; amend
   `executor_namespacing` to say namespacing is by `use`, and that "no bare files"
   now means "no bare files WITHOUT a use". Update the two registry.pl comments
   (`:308`, `:560`), the parser comment (`:1597`), and the tmLanguage / d2 lexer
   (`git show 40d94c2b9 --stat | grep -iE 'tmLanguage|syntax|lexer'`; the d2 fork is
   `~/projects/d2-dl6`): add `use <ident>` highlighting, keep the path forms.
6. Docs: `README.md` and `docs/**` examples that show a slash or dotted executor rel
   move to the `use` form. Selfdoc: run `cd v6 && just selfdoc` at the end and commit
   the regenerated output if it changed.

## Ownership
Yours: `v6/prolog/**` (all of it), every `.dl6` except the list in item 3,
`README.md`, `docs/**` (not `docs/failure-modes.md` entries of other lanes; append
yours), `v6/sprefa-engine-rs/tests/**` only where a test string spells a path form,
tmLanguage + `~/projects/d2-dl6` lexer. FORBIDDEN: `v6/sprefa-engine-rs/src/**` (the
`soopy__files` atoms do not change, so the engine needs no edit; if you think it does,
hail with the line), the item-3 list, `v6/tsv2/**`.

## Gate (print every number in the PR body)
- `cd v6/prolog/conformance && swipl -g go -t halt go.pl` (440 PASS at base; never shrinks)
- `cd v6 && just plunit` (1042/0 at base)
- `bash v6/sprefa-engine-rs/grade.sh` (graded=440 byte-clean=335 at base)
- `cd v6/sprefa-engine-rs && cargo test` (175/0 at base)
- `cd v6 && just ghcacher-rust` (goldens=6 at base)
- `bash v6/dl/ghcache/gate.sh` (GHCACHE_RUST_DOOR_HOLDS ticks=13 at base)
- `bash v6/dl/crosswalk/gate.sh` (10/10 at base)
- `swipl -g go -t halt v6/prolog/ARCH.pl`
`export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`. `timeout` on every command. Nothing
foreground over 10s: background it and poll with an `until` loop. Measure a failing leg
three times before calling it broken; read `.github/CI-KNOWN-RED.md` first.

## Laws
FIRST ACTIONS: `git merge --ff-only <base sha>`, then `bash v6/tools/doctor-deps.sh` (DEPS OK
for both crates). Failure = STOP and hail. Never spawn subagents. Commit every green step.
PR against `main`; the PR body carries every gate number and every receipt. `v6/tsv2/**` is
paused: never edit it; emitted TS for an unchanged program stays byte-identical.
No em dashes. Banned in prose and identifiers: provenance, substrate, load-bearing, regime,
refusal, "ground truth" (say oracle). Comments state constraints only; no change-log
narrative, no dates, no PR numbers in comments. dl variable names descriptive, never
single-letter. Surrogate INTEGER keys; no composite TEXT keys. One failure-ledger entry in
`docs/failure-modes.md` per incident this arc fixes (incident, RCA, fail-pre-fix test, rail).
Language design is NOT yours: where this brief leaves a design fork open, pick the
spelling this brief gives, and if none is given, hail the coordinator with the fork and
continue on the other work.

## Reaching the coordinator
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<one line>"` lands in
the coordinator inbox at its next turn. Use it when blocked, when done (PR number + every
gate number), when this brief is wrong, when you find a defect outside your ownership.
`boop beep lane list` shows your lane name. A lane that ends its turn parks idle; hail
before you stop.

