# openapi-clap-uds-research

Epic card: issues/openapi-clap-uds-lab/item.md. Read it first; it holds the
use case, the research questions, and the prior-art paths. Read
CLAUDE.md standing laws (build-vs-buy, infra is bought, lang design is
Chris in the room, banned words, no em dashes, tables over prose).

## Your job: RESEARCH ONLY. No Rust code, no dl6 beyond rel declarations.
1. For each research question in the card, produce a candidate table:
   crate / latest version (check crates.io via `cargo search <name>` or
   `cargo info <name>`) / what it gives / what it lacks / verdict.
   Minimum 3 candidates per question. Read each crate's README/docs.rs
   (`curl -sL https://docs.rs/<crate>` or `cargo doc` is NOT needed; fetch
   the README from GitHub raw or crates.io). Quote the exact API you would
   call (type + fn names).
2. Read the prior art paths in the card and table what each already does.
3. Write the dl6 rel schema for an OpenAPI spec as rel declarations only:
   `rel operation(...)`, `rel parameter(...)`, `rel schema(...)`, etc, with
   descriptive column names and `: type` annotations, keyed by INTEGER ids
   (surrogate keys law; read .claude/skills/sql-relational-design/SKILL.md).
   For each rule you would need, state whether an existing construct
   compiles it (grep `v6/prolog/compile/out/manifest.json` and
   `v6/prolog/compile/CONSTRUCT-REFERENCE.md`); if not, name the gap and stop.
4. Mermaid pipeline: yaml -> json -> EDB rows -> dl6 rels -> emitters ->
   {clap tree, axum-uds router, client}. Under 24 shapes.
5. Minimal lab plan: `sprefa-lanes/pokeapi.openapi.yml` -> CLI verb
   `pokemon get <name>` over a UDS socket, with the exact commands a reader
   would run, and the receipt that proves the CLI mimics the API (byte diff of
   CLI stdout vs curl --unix-socket body).
6. Open forks for Chris as a table: fork / options / what each costs.

## Deliverables
`plans/2026-08-17-openapi-clap-uds.PLAN.md` (TOC, tables, citations) and
`plans/2026-08-17-openapi-clap-uds.PLAN.visual.human.unga.md` (plain words,
mermaid, zero citations, one page). Commit both on your branch, push, open a
PR with `gh pr create --fill`. Do not merge. Report the PR number.
