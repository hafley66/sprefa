# Brief: fresh-machine.md + engine config (cards issues/fresh-machine-page S, issues/engine-config-no-env S; epic productionize-rust-door). BLOCKED until dl6c-saved-state and dl6-build-single-binary merge; the coordinator spawns this after.

Repo `~/projects/sprefa`. Base = origin/main sha the coordinator gives you (do not rebase). Worktree: `git -C ~/projects/sprefa worktree add ~/projects/sprefa-worktrees/<branch-dir> -b <branch> <sha>`; first action inside: `git log -1` = sha, tree clean; then `just boop-start` if it exists (it builds the extractor into the shared target; do not rebuild by hand). Never touch the main tree at `~/projects/sprefa`. Never `git stash`. Read `CLAUDE.md` standing laws and style laws first: no eprintln, banned words provenance/substrate/load-bearing/regime/refusal/honest/ground*, no em dashes, build-vs-buy paragraph before any hand-rolled infra, 10-second law, failure ledger, comment budget. Measure three times, never once. You MAY spawn ONE sonnet subagent for mechanical parts with disjoint files stated in its prompt; you review and test every line; no further fan-out. PR: `gh pr create --base main` with 1-3 plain sentences, `## Reading order`, `## Tests` (name, input, expectation, printed before); no words gate/leg/receipt/door/probe/refusal; do NOT merge; tick the AC you land on the card in your worktree and commit the card with the code.

Branch `docs/fresh-machine-page`.

## Deliverables
1. Config (`engine-config-no-env`): in the `dl6` bin and the built program binary, one config source: CLI flags, then `<prog>.toml` beside the binary, then env; no `CARGO_MANIFEST_DIR` default at runtime (`types.rs:631`); `<prog> config` prints every key with its source; a missing executor path (`DL_EXTRACT_BIN`, `SOOPY_BIN`) is a named error at boot, never a mid-tick spawn failure; fixtures keep `$DL_EXTRACT_BIN` spelled in `sh` templates (the shell adapter fills it from config). `docs/config.md` table of keys.
2. `docs/fresh-machine.md` under 80 lines, opens with a TOC, tables and code only: install dl6c (`just install-dl6c`), install the engine bin (`cargo install --path v6/sprefa-engine-rs` or the recipe that exists by then), write `hello.dl6` (one base rel, one rule, one `sh` that runs `echo`), `dl6 build hello.dl6`, `./hello serve --socket /tmp/hello.sock`, `curl --unix-socket /tmp/hello.sock localhost/rel/<name>`, one `POST /arrive` and the re-read.
3. The run: execute the page with `HOME=$(mktemp -d)`, PATH holding only the two installed binaries plus system tools, no sprefa checkout reachable; attach the transcript (`issuectl attach fresh-machine-page <file>`) and tick the AC.

## Files owned
`docs/fresh-machine.md`, `docs/config.md`, `v6/sprefa-engine-rs/src/bin/dl6.rs` and `src/build_template/**` (config only), `v6/sprefa-engine-rs/src/types.rs` (the adapters-dir default only), `v6/sprefa-engine-rs/tests/config.rs`, the two cards. Nothing in `v6/prolog`.

## Tests to run at the end
`cargo test -p sprefa-engine-rs`, the fresh-HOME run itself (three times), `bash v6/sprefa-engine-rs/grade.sh`.
