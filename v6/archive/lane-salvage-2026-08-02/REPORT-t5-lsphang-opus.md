(Coordinator note: agent harness blocked its own REPORT.md write; relayed from its final message.)

# v5 dl --lsp exit hang: root cause, fix, receipts

- Root cause: background threads (daemon subscriber :200, diag-db poll :464) hold permanent clones of connection.sender; lsp-server's IoThreads::join cannot return until every clone drops. Loops were correct; the hang was entirely in the join. Proven with a macOS `sample` stack capture.
- Fix (src/lsp.rs only, +47/-10): finish_lsp() drops IoThreads handles instead of joining (detach wedged writer/dropper), settles exit code per LSP contract (0 shutdown-first, 1 otherwise). Message-loss argument via bounded(0) channel semantics in the report.
- Tests: tests/it/lsp_exit.rs, 3 tests, 2 red pre-fix (15s hang receipts pasted), all green post-fix in 1.61s; full lsp_ suite 33/0.
- Manual stdio transcripts pre (HUNG, SIGKILL) and post (exit 0 in 0.27s; EOF-only exit 1 in 0.05s).
- Residuals disclosed: daemon-subscriber leg covered by construction not test; background threads still uncancellable (join restorable if push seam reworked); src/lsp.rs 2000 lines pre-existing rail debt.
