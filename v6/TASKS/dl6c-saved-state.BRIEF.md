# Brief: dl6c, the compiler as one executable (card issues/dl6c-saved-state, epic productionize-rust-door, size S)

Repo `~/projects/sprefa`. Base = origin/main sha the coordinator gives you (do not rebase). Worktree: `git -C ~/projects/sprefa worktree add ~/projects/sprefa-worktrees/<branch-dir> -b <branch> <sha>`; first action inside: `git log -1` = sha, tree clean; then `just boop-start` if it exists (it builds the extractor into the shared target; do not rebuild by hand). Never touch the main tree at `~/projects/sprefa`. Never `git stash`. Read `CLAUDE.md` standing laws and style laws first: no eprintln, banned words provenance/substrate/load-bearing/regime/refusal/honest/ground*, no em dashes, build-vs-buy paragraph before any hand-rolled infra, 10-second law, failure ledger, comment budget. Measure three times, never once. You MAY spawn ONE sonnet subagent for mechanical parts with disjoint files stated in its prompt; you review and test every line; no further fan-out. PR: `gh pr create --base main` with 1-3 plain sentences, `## Reading order`, `## Tests` (name, input, expectation, printed before); no words gate/leg/receipt/door/probe/refusal; do NOT merge; tick the AC you land on the card in your worktree and commit the card with the code.

Branch `feature/dl6c-saved-state`.

## What exists
- Compile entry: `swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl -g "compile_dl6('<in>','<out>',[emitter(emit_rust:emit_program)])" -g halt` (`v6/sprefa-engine-rs/grade.sh:36-45`); TS emitter the same shape via `v6/prolog/compile/scripts/compile_dl6.sh`.
- SWI-Prolog 10.0.2 arm64. `qsave_program/2` available; zero uses under `v6/prolog` today (grep).
- Foreign/library deps to resolve before saving: `library(crypto)` (`0_type_ids.pl` SHA-256), `library(http/json)`, whatever `use_resolve.pl` loads lazily (autoload). Find them by saving once with `autoload(false)` and reading the undefined-procedure errors.
- Version stamp pattern: hafley-rs `crates/boop/scripts/install.sh` (`BOOP_BUILD_SHA` into `--version`).

## Deliverables
1. `v6/prolog/dl6c.pl`: a `main/0` that parses `dl6c <in.dl6> --target rust|ts --out <dir> [--version]` (use `library(optparse)` or `library(main)`, say which) and calls `compile_dl6/3` with the right emitter; exit 0 / 2 (named unsupported construct) / 1 (error), the same contract `bop check` uses (`v6/prolog/compile/scripts/bop_check.pl`).
2. `just build-dl6c` -> `v6/prolog/target/dl6c` via `qsave_program(Path, [stand_alone(true), goal(main), toplevel(halt), autoload(true), ...])` with the sha stamped in; `just install-dl6c` copies to `~/.cargo/bin/dl6c` and prints `dl6c --version`.
3. Tests (`v6/prolog/compile/test/dl6c.test.pl` registered in `plunit_tests.pl`, plus one shell test `v6/prolog/compile/scripts/dl6c_roundtrip.sh`): the saved state, copied to a temp dir with NO `v6/prolog` on any path, compiles `golden-flex.dl6`, `resident-coroutine.dl6` (has `sh`), `anonymous-type-syntax.dl6` for both targets and the bytes equal what `compile_dl6.sh` / the grade.sh line produce; `dl6c --version` prints the sha; a file with an unsupported construct exits 2 with the named reason.
4. Docs: `v6/prolog/README.md` or `v6/GETTING-STARTED.md` gains a 10-line `dl6c` section (install, usage, exit codes).

## Files owned
`v6/prolog/dl6c.pl` (new), `justfile` (two recipes), `v6/prolog/compile/test/dl6c.test.pl`, `v6/prolog/compile/test/plunit_tests.pl` (one registration line), `v6/prolog/compile/scripts/dl6c_roundtrip.sh`, the doc section, `issues/dl6c-saved-state/item.md`. Do NOT edit `compile.pl`, emitters, or any `0_*.pl` except to add an explicit `use_module` that autoload resolution proves missing (list each in the PR).

## Tests to run at the end
`cd v6 && just plunit` (5 known-red at `.github/CI-KNOWN-RED.md:32`), `bash v6/prolog/compile/scripts/dl6c_roundtrip.sh` three times, `swipl -g go -t halt v6/prolog/ARCH.pl`.
