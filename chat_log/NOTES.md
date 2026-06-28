# Session notes (inline)

Drop an HTML comment whose body is `@` + a lowercase tag + text (tags like
@plan / @idea / @gotcha / @todo) anywhere in this file or any chat_log markdown.
`v5/examples/autodoc-plans.dl` transcludes them into PLANS.md. The seeds below
are from the 2026-06-28 session; edit or delete freely. (This paragraph avoids
the literal marker syntax so the finder does not transclude its own docs.)

<!-- @plan I2 proof fixture: Kotlin interface getUser + 2 impls (one via `by` delegate) + TS client + Rust get_user, prove cross-lang refs/impls/goto -->
<!-- @idea canon() dl builtin: lowercase + strip non-alnum so getUser ~ get_user ~ getUsers match across codegen rhythms -->
<!-- @idea pull() source: materialize a build-artifact spec (FastAPI live /openapi.json) into the file space; poll-as-push gated by blake3; stored interruptible activation state -->
<!-- @idea DSL-driven hover sections: hover_section(name, title, body) convention rel so a dl page adds ref/type/impl/flow hover blocks, same seam as diag/def_target -->
<!-- @gotcha cross-repo goto needs a repo column on def_target/the spine, or absolute paths at the LSP boundary; root.join assumes one root -->
<!-- @gotcha content-addressed FileId collapses byte-identical files across checkouts: great for "all refs", ambiguous for "which checkout"; group worktrees via git rev-parse --git-common-dir -->
