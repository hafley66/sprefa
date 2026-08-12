# BRIEF: v6 stops borrowing v5's LSP. Recon and price it.

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `154ae23c`.
- FIRST action: `git log --oneline -1`. Any other base = STOP AND REPORT.

## USER DECISION, 2026-08-12, verbatim
"I DO NOT WANT TO RUN V5 ANYTHING ANYMORE"

That is the constraint. Every design you price must hold under it. Do not
produce an option whose answer is "keep the v5 binary running".

## One sentence
v6's editor features currently reach the editor by writing tables that the v5
Rust binary's LSP polls; find every place v6 depends on a running v5, and
price replacing that path with something v6 owns.

## What we know, verify each line
| fact | citation |
|---|---|
| `hover_note` is a v5 built-in sink, fixed 6 columns | `src/engine/decls.rs:297-299` |
| v5's LSP reads it | `src/lsp.rs:884`, `hover_notes_at(path, line, character)` |
| v5 merges it into the hover response | `src/lsp.rs:859-861` |
| v6 gets in by NAMING | v6 names emitted tables by the bare rel name, `v6/prolog/lower.pl:156`, so a `.dl6` rel named `hover_note` IS the table v5 polls |
| the same trick for diagnostics | `lsp-diags.sh`, a `.dl6` rel named `diag_v5` |
| this shipped hours ago | PR #202, `plans/2026-08-12-import-openapi-hover.md` |
| that lane already found the bridge is thin | its own report: no note reached a real VS Code, the v5 bridge the brief assumed does not exist |

`src/` at the repo root IS v5. Anything under it is on the wrong side of the
user's line.

## Your job: recon and price. Build nothing.
This is a plan lane. Zero production code. Two docs and nothing else.

## Files you own
| path | permission |
|---|---|
| `plans/2026-08-12-v6-native-lsp.PLAN.md` | create |
| `plans/2026-08-12-v6-native-lsp.PLAN.visual.human.unga.md` | create |

Everything else READ-ONLY. Zero other paths in `git status`. Three other lanes
are editing the compiler, the emitter seam, and the CI ledger right now.

## Deliverable part 1: the dependency inventory
Every place v6 needs a running v5. One row each, with `file:line`. Do not stop
at hover; grep for the whole surface.

| column | content |
|---|---|
| feature | hover, diagnostics, code lens, go-to-def, formatting, whatever you find |
| how v6 reaches it today | the rel name, the table, the poll |
| v5 code that serves it | `file:line` under `src/` |
| does it work end to end today | measured, not assumed. PR #202's own lane could not get a note into a real editor |

A feature that turns out NOT to depend on v5 is a finding. Say so.

## Deliverable part 2: build-vs-buy, mandatory, no shortcuts
CLAUDE.md, non-negotiable, every agent:

> **Build-vs-buy**: never assert "write our own" for a common-shaped problem
> without library research + written candidate analysis first. No one-line
> dismissals of libraries.

An LSP server is the most common-shaped problem there is. Research real
candidates and write a table: name, language, what it gives you, what it costs,
what it forces on the architecture, and whether it fits a TypeScript+SQLite
runtime that already exists (`v6/tsv2`). At minimum look at
`vscode-languageserver-node`, and at whatever else your research turns up. A
one-line dismissal of any candidate voids the deliverable.

The v6 runtime is already TypeScript with an rxjs spine and a `serve` path
(`v6/tsv2/serve/`). Price "the LSP is another consumer of the existing serve
process" against "a separate server" honestly.

## Deliverable part 3: the forks
The user rules on design. You present cited forks, you do not choose.

Each fork carries: what it is in one sentence, what it costs in files and
lines, what it forces later, what it forecloses, and the throw sites or
citations that make the price real. Priced against the standing laws:
- exactly ONE manual `.subscribe()` per app, ratchet baseline 1
- Promise/async banned above the SqlRunner seam
- infra is bought, never built
- every new class declares its interface in the package's header `types.ts`

## Deliverable part 4: what happens to PR #202
It landed a `hover_note` sink that only pays off through v5. State plainly
whether it is: still useful unchanged, useful with a different consumer, or
dead weight to revert. Cite what you measured.

## Anti-cheat
| rule | why |
|---|---|
| every dependency row carries a `file:line` under `src/` | otherwise it is a guess |
| "does it work end to end today" is MEASURED | PR #202 shipped a path nobody had run into an editor |
| no candidate is dismissed in one line | the build-vs-buy law |
| you choose no fork | the user rules on design |
| you write zero production code | three lanes are in the tree |

## Worktree setup, before your first commit
```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
`git commit -n` and `--no-verify` are FORBIDDEN.

## Rails
- Commit after each deliverable part. Never spawn a subagent.
- Both docs are required. A plan without the `.visual.human.unga.md` companion
  (plain words, ascii or mermaid, ZERO citations) is undelivered.
- Docs open with a TOC.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; unbuilt is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Tables and lists over prose. Prose is a one-line caption under a table.
