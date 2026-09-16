# feature-prolog-rehome-dl6: pass 3, SUPERSEDES the FOLLOWUP brief

Chris re-scoped (2026-08-24): NO dl6 program. Delete `v6/dl/prolog_rehome/`.
Keep the committed extractor rows and the hosts.rs root fill exactly as they
are at 2fef45076 (they land later). Stop all rehome.dl6 work now.

The deliverable is ONE CLI verb on the sprefa-extract binary:

    extract move <old.pl> <new.pl> [--root DIR] [--commit] [--shim]

"move this file there, boom, done". Dry run by default.

## Where

`v6/sprefa-extract/src/bin/extract.rs:279` dispatches `argv[1] == "query"`
to `0_query.rs` through `#[path]`. Add `argv[1] == "move"` the same way,
body in a new `v6/sprefa-extract/src/0_move.rs`. clap `Parser` for the verb's
own args (clap is a dep). Ownership: `src/0_move.rs`, `src/bin/extract.rs`
(dispatch lines only), `tests/1_move.rs` (new). Forbidden: everything else,
`v6/prolog/**` is read-only except inside the receipt below.

## What it does

1. `--root` defaults to the git root of `<old>` (`soopy::discover`, the shape
   at `src/0_query.rs:61-66`). Paths print repo-relative.
2. Walk root for `*.pl` and `*.plt`, skipping `.boop-worktrees/`, `.git/`,
   `target/`, `node_modules/`. Extract each with `PrologSource.extract(path,
   bytes, FamilyMask::ALL)` and keep the Specifier rows (kinds `named`,
   `side_effect`, `include`, `reexport_module`, `reexport`; the spelling is
   pinned in `tests/0_prolog.rs`).
3. Resolve a specifier text to a file: strip quotes, ignore `library(...)`,
   resolve relative to the importer's directory, append `.pl` when the bare
   path is not a file. Importers of `<old>` = files with a row resolving to it.
4. Plan, as one soopy `StageRequest` (`soopy::StageRequest`, `SourceAction::
   {Move, Replace, Create}`, root id from the opened root the way
   `v6/sprefa-engine-rs/tests/15_source_mutation_hosts.rs:81-95` builds it):
   - Move `<old>` -> `<new>`.
   - For every importer, Replace the specifier text with the path relative
     from the importer's directory to `<new>`, no `.pl`, quoted as it was.
   - For every relative `include`/`use_module` INSIDE `<old>`, Replace it so it
     still resolves from `<new>`'s directory.
   - `--shim`: instead of rewriting importers, Create at `<old>` the two lines
     `:- module(<mod>_shim, []).` / `:- reexport('<rel to new, no .pl>').`
     where `<mod>` is the `module/2` name from `<old>`'s own module row.
5. Dry run: `soopy::stage_mutations` into a state dir OUTSIDE root (default
   `~/.agent/soopy-state/`, `--state` overrides), then print one line per
   preview `path_before -> path_after  <summary>` and, for each Replace, the
   unified diff. Exit 0, tree untouched (`git status` unchanged).
6. `--commit`: stage, then commit through soopy (`_7f_commit.rs:186`), print
   the same table plus the stage id.

## Receipt (run all, paste output in the PR)

- `cargo test -p sprefa-extract --test 1_move`: a temp git repo with
  `a.pl` (module, `use_module('lib/b')`), `lib/b.pl` (module with
  `include('b_part.pl')`), `lib/b_part.pl`; `move lib/b.pl core/b.pl` dry run
  yields exactly 1 Move + 2 Replace (a.pl's import -> `'core/b'`, b.pl's include
  -> `'../lib/b_part.pl'`); `--commit` then `swipl -g halt -l a.pl` rc=0.
  `--shim` yields 1 Move + 1 Create, and `swipl -g halt -l a.pl` still rc=0.
- Real dry run in your worktree: `extract move v6/prolog/compile/registry.pl
  v6/prolog/next/registry.pl`, paste the full table (count importers).
- Real commit of the same, then `cd v6/prolog && swipl -g go -t halt ARCH.pl`
  rc=0 and `cd v6/prolog/conformance && swipl -g go -t halt go.pl` PASS count
  equal to main's; then `git revert` that commit so the PR moves no prolog.
- 10-second law: no single command over 10 s foreground; batteries in the
  background with a per-test cap.

Commit as you go. PR against main from this branch. Hail sprefa-coordinator
with the PR number and the receipt numbers.
