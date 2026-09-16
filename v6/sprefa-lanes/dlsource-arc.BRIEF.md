# BRIEF: DlSource, the .dl6 extractor in sprefa-extract

## SALVAGE STATE (read first)
Two prior lanes died on provider flakes mid-work. Your branch is cut from the
salvage head and already carries the `tree-sitter-dl6/` crate scaffold
(Cargo.toml, build.rs, grammar.js, src/, tree-sitter.json). Read it, keep what
is correct, continue from there.

## Base
`git merge a6a0b9dad6becde23e76fd1a3940de2d385d1829`. Failure = STOP
AND REPORT. If a procedural line in this brief seems to forbid otherwise-correct
work, the work wins: note the conflict in your report and keep going.

## One sentence
`sprefa-extract` cannot read `.dl6` files (`src/lang/mod.rs` `source_for`
returns None for them); give it a DlSource backed by the finished lab
tree-sitter grammar so `.dl6` rails and xref can run on the extractor.

## URGENCY
The user is about to lift `v6/sprefa-extract` out into the hafley-rs workspace.
Everything you add must be SELF-CONTAINED under `v6/sprefa-extract/` so the move
is one `git mv`. No path deps reaching outside that directory.

## What exists, measured. Verify each line before building on it.

| fact | evidence |
|---|---|
| roster of Sources, first-match, order matters | `v6/sprefa-extract/src/lang/mod.rs:36-45` |
| the Source trait + ExtractOutput/FamilyMask | `v6/sprefa-extract/src/source.rs` |
| PrologSource is the template: tree-sitter parse, four planes, spans | `v6/sprefa-extract/src/lang/prolog/_0_source.rs:1-60` |
| ast query language arms live in `query_language` | `v6/sprefa-extract/src/0_query.rs:79-94` |
| a COMPLETE dl6 tree-sitter grammar lab: grammar.js (name "dl6"), fixtures, queries, run-tests.sh, generated src/ | `v6/labs/tree-sitter-door/` |
| the lab grammar was emitted/checked against the prolog DCG | `v6/labs/tree-sitter-door/emit_grammar.pl`, REPORT*.md |
| tree-sitter 0.25 is already a dep; grammar crates wrap `LanguageFn` | `v6/sprefa-extract/Cargo.toml:47-55` |

## Deliverables, in order
1. **Crate-ify**: `v6/sprefa-extract/tree-sitter-dl6/` as a real cargo crate
   (Cargo.toml, build.rs compiling the generated parser.c, `LANGUAGE:
   LanguageFn` export) built FROM the lab's grammar. Regenerate parser.c with
   the tree-sitter CLI if the lab's `src/` is stale; state which you did. Port
   the lab's corpus tests into the crate. Do NOT edit grammar.js semantics;
   grammar changes are language design and belong to the user.
2. **DlSource**: `v6/sprefa-extract/src/lang/dl6/` following the PrologSource
   file shape. Minimum planes: CstF always; CallF where a rule body references
   a rel (`rel_ref` as a call site, the rule head as the def); TypeF for `rel`
   declarations (rel name = type entity, columns = members). If a plane cannot
   be expressed, say why in the report, with the grammar node kinds cited.
3. **Register**: add to `sources()` before `AstgrepSource`; `matches` =
   `.dl6`. Add the `"dl6"` arm in `0_query.rs:query_language`.
4. **Tests**: extraction tests with 2-3 committed `.dl6` fixtures asserting
   exact nodes/edges (names + spans), the `hostDecode`-style exactness, not
   counts. Fail-first: paste the pre-fix failing run in each test header.
5. Leave `v6/labs/tree-sitter-door/` IN PLACE; the coordinator deletes labs on
   landing. Record in your report the exact lab files your crate consumed.

## Files you own
- `v6/sprefa-extract/**` (new crate dir, src/lang/dl6/, mod.rs roster line,
  0_query.rs arm, Cargo.toml/lock, tests)

FORBIDDEN: `v6/prolog/**`, `v6/tsv2/**`, `v6/sprefa-engine-rs/**`,
`v6/labs/**` (read-only), `CLAUDE.md`, every existing lang/ source file except
the one roster line in `lang/mod.rs`.

## Validation, run and paste verbatim, each three times
```bash
cd v6/sprefa-extract && cargo test 2>&1 | tail -5
cd v6/sprefa-extract && cargo build --release --features cli --bin extract 2>&1 | tail -2
bash v6/sprefa-engine-rs/grade.sh 2>&1 | tail -3   # must stay at its current count; you touched no engine file
```

## Style laws, inline
- No em dashes. Banned words in prose AND identifiers: provenance, substrate,
  load-bearing, regime. The word "refusal" is banned in prose.
- Comments state only constraints the code cannot show.
- Descriptive variable names, never single-letter.
- No `eprintln!` in src/**; `tracing` only.
- The 10-second law: any single operation over 10s is a defect to investigate.

## Report format
Zero-context coworker brief, every claim `path:line`. One focus per section.
Impossible step = STOP and report the throw site.
