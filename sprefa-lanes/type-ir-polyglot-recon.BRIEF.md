# BRIEF: the type IR as a polyglot door. Recon and price.

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `9d5cd3d3`.
- FIRST action: `git log --oneline -1`. Any other base = STOP AND REPORT.

## USER VISION, 2026-08-12, verbatim
"since we can ingest openapi json schema into our types, i want to lube up and
be able to codegen with the type ir into other stuff. the idea is that we will
allow the syntax for types of any major languages for maximum comfy and provide
a typespec most covering type system translations. imagine tree sitter of type
systems some how"

Read that carefully; it is bidirectional and it is N-to-M.

```
   TS type syntax  ─┐                        ┌─ TypeScript types
   Rust type syntax ┤                        ├─ Rust types
   OpenAPI / JSON Schema ├──> the TYPE IR ──>┤─ JSON Schema
   Go / Python / whatever ┤                  ├─ OpenAPI
   .dl6's own spelling  ─┘                   └─ .dl6
```

"Tree sitter of type systems" is the shape to take seriously: tree-sitter's
win is ONE declarative grammar per language, not a hand-written parser per
language. The analogue is one declarative description per type system, not a
hand-written converter per pair. N+M descriptions, never N*M converters. Price
whether that is achievable and where it breaks.

## This is recon. You build nothing.
Six lanes are editing the tree. You write two docs. Zero production code.

## What exists today. Verify every line; do not re-derive.
| piece | file | lines |
|---|---|---|
| the type plane, the IR as it stands | `v6/prolog/0_type_plane.pl` | 920 |
| OpenAPI/JSON Schema INTO our types | `v6/tsv2/scripts/openapi_to_dl6.ts` | 550 |
| our types OUT to OpenAPI | `v6/prolog/compile/5_emit_openapi.pl` | 103 |
| our types OUT to JSON Schema | `v6/prolog/compile/4_emit_jsonschema.pl` | 176 |
| driven from | `v6/prolog/sweep.pl:121` | |

So ONE input door and TWO output doors already exist. The ingest direction the
user names is real and shipping. Nothing emits a language's type syntax.

Type constructors the plane already carries, from `0_type_plane.pl`: `option(T)`,
`list(T)`, `json_list(T)` (a typed view, `:95`, `:118`), `list_interned_set`
(`:150`), relation references as column types, enum variants with payloads.
Ruling `list_spelling = list_of_type` at `:92`. Inventory the FULL set yourself
and put it in a table; that inventory is deliverable 1.

There is NO cross-language type generation anywhere: zero `ts-rs`, `schemars`,
`specta`, `typeshare` in any `Cargo.toml`. Hand-written type surfaces that a
codegen would eventually replace, measured:
`v6/sprefa-extract/src/types.rs` 1834, `v6/tsv2/runtime/types.ts` 1069,
`v6/dl/src/0_types.ts` 630, `v6/sprefa-store/js/src/engine/types.ts` 546,
`.../lower/types.ts` 170.

## Deliverable 1: the IR's expressive frontier
A table of type constructor -> what it means -> how it spells in `.dl6` ->
how it spells in TypeScript, Rust, JSON Schema, OpenAPI, TypeSpec.

The interesting cells are the ones that DO NOT map. Name them. Examples to
check, not an exhaustive list: Rust needs an explicit annotation where TS
infers; TS has structural typing and untagged unions where Rust needs a tagged
enum; `option(list(T))` versus `list(option(T))`; nullability versus absence
(the user already ruled JSON null IS the `none` atom); integer width; where our
`list_interned_set` has no counterpart at all.

A cell you cannot fill is the finding. Write "no counterpart" and say why.

## Deliverable 2: BUILD-VS-BUY, mandatory, no shortcuts
CLAUDE.md, non-negotiable at every agent level:

> never assert "write our own" for a common-shaped problem without library
> research + written candidate analysis first. No one-line dismissals.

Cross-language type generation is the most common-shaped problem there is.
Research and write a candidate table covering at minimum: **TypeSpec**
(the user named it), **JSON Schema + quicktype**, **protobuf**, **Cap'n Proto**,
**Smithy**, **ts-rs**, **specta**, **schemars**, **typeshare**, **serde-reflection**,
and whatever your research turns up. For each: which direction it goes
(one source to many languages, or one language to many), what type systems it
covers, what it CANNOT express from our table in deliverable 1, license, and
whether it can be driven from a Prolog emitter or forces a different toolchain.

Pay specific attention to whether any of them is genuinely "the tree-sitter of
type systems" (a declarative per-type-system description) or whether they are
all one fixed IR with hand-written backends. That distinction IS the user's
question.

A one-line dismissal of any candidate voids the deliverable.

## Deliverable 3: the two populations, priced separately
Do not let these merge; they have very different costs.

| population | source of truth today | what codegen means |
|---|---|---|
| program-derived types (rels from a `.dl6`) | the registry + lowered plan | a new emitter beside `4_emit_jsonschema.pl`, cheap, and the `emit-rust-sqlite` lane needs it within days |
| hand-written library types | none, maintained twice in TS and Rust | pick an IDL and rewrite 4000+ lines of maintained code |

Price each. Say plainly which one delivers value first.

## Deliverable 4: the input side
The user wants "the syntax for types of any major language for maximum comfy",
so a `.dl6` author could write a Rust-shaped or TS-shaped type and have it
land in the IR. Price that against the existing parser: `parse_dl_dcg.pl` is
the canonical parser and a concurrent lane touched it hours ago. Say whether
alternative type syntaxes are a parser change, a preprocessor, or a separate
front end, and what each costs.

Relevant measured fact from tonight: two clean-room agents independently built
DCGs for this language and BOTH hand-wrote their pretty printer because reverse
mode does not terminate on a character-level DCG. See
`plans/2026-08-12-cleanroom-dcg-bakeoff.md`. If your design needs a type
syntax to round-trip OUT as well as IN, that result is your constraint.

## Deliverable 5: the forks
The user rules on language design. Present cited forks; choose none. The
central one they already named without answering:

**Is `.dl6`'s type plane the IDL, or do we adopt an external IDL and generate
`.dl6` from it too?**

Each fork carries: one sentence of what it is, cost in files and lines, what it
forces later, what it forecloses, and citations that make the price real.

## Files you own
| path | permission |
|---|---|
| `plans/2026-08-12-type-ir-polyglot.PLAN.md` | create |
| `plans/2026-08-12-type-ir-polyglot.PLAN.visual.human.unga.md` | create |

Everything else READ-ONLY. Zero other paths in `git status`. Six lanes are live
in `lower.pl`, `analyze.pl`, `compile.pl`, `6_emit_dd_plan.pl`, `.github/`, and
a new `emit_rust.pl`.

## Anti-cheat
| rule | why |
|---|---|
| every "exists today" row carries `file:line` | the repo's own headers have been wrong four times about its own grammar |
| a type-system cell you cannot fill says "no counterpart" | a filled-in guess is worse than a hole |
| no candidate dismissed in one line | the build-vs-buy law |
| you choose no fork | the user rules on design |
| zero production code | six lanes are in the tree |

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
- Commit after each deliverable. Never spawn a subagent.
- Both docs required; the `.visual.human.unga.md` one is plain words, ascii or
  mermaid, ZERO citations. Docs open with a TOC.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; unbuilt is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Tables and lists over prose. Prose is a one-line caption under a table.
