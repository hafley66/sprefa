# crosswalk-use-cases (research; doc only; no builds)

User 2026-09-26: "match dl5's golden use cases, not the lang. half that is host / userland / kernel space."
The existing plans/v8/2026-09-26-crosswalk.md is organized by language feature (R01 ground facts, R09 min/max qualifiers, ...). Re-cut it by USE CASE.

Goal: write plans/v8/2026-09-26-use-cases.md. One row per v5 golden use case: an end-to-end outcome a user got from v5 (e.g. keep a markdown snippet in sync with source via comment_node; resolve calls across repos; rename a symbol across a revision; ...).
Columns: use case | v5 golden evidence (v5/tests/it file + the asserted output) | layer that owns it in v8 (kernel = ivm IR/engine; host = executors/soopy/runtime; userland = prelude or user .dl7 library) | v8 status (works / partial / missing) | v8 path (file:line, or the smallest missing piece) | ladder rung, if kernel.
Rules:
- A v5 language feature appears only as the means inside a use case, never as its own row. Drop rows that exist only because v5 had some syntax.
- The where/bytes/file/git/rev model is soopy (host). Never re-solve it.
- Group rows by layer. Kernel rows should be few; most use cases are host or userland.
- Append coverage: every v5/tests/it file -> use case id, or "lang-only (dropped)" with a one-line reason.
Inputs: plans/v8/2026-09-26-crosswalk.md (already has file:line work; reuse it), v5/tests/it (227 files), v5/src, soopy (hafley-rs crates/soopy via the boop-start symlink if present).
Commit subject: "plans: v5 golden use cases by layer" Boop-Status done.

## Laws
- Work only in $PWD (your worktree). Never cd to a primary checkout. Never push. Never commit on main.
- No cargo builds.
- Final message receipt: status, sha, files, validation, next.
