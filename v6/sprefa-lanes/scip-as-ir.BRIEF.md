# BRIEF: SCIP as a data model, not as storage. What can its types express for us.

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha; that is your base. The ordering is not a gate. If a procedural
line in this brief seems to forbid otherwise-correct work, the work wins: note
the conflict in your report and keep going.

**Docs only. Write ZERO implementation code.** Two plan docs are the deliverable.

## The user's words, verbatim. Read them twice, the framing is the whole task.

> "i mean scip as a data type format or ir for us to express what it can express,
> i barely know scip, i dont mean literal or really shit storage of it we would
> dense id the world but idea is same, what can we use scip data types for in
> this system matrix of features"

Three things this is NOT:
1. NOT a proposal to store SCIP indexes. The user explicitly rejects that
   ("i dont mean literal or really shit storage of it").
2. NOT a re-run of the v5 SCIP ingestion work, which is already done and priced.
3. NOT an argument for or against SCIP as a tool.

What it IS: **SCIP is a data model for describing code symbols across languages.
The question is which of its modelling ideas this system should adopt.** The
user says "we would dense id the world but idea is same", which is exactly
right: our version of SCIP's global symbol string would be an interned integer
id, per this repo's surrogate-key law. The IDEA transfers; the encoding does not.

The user also says "i barely know scip". So your document must TEACH the model
before it evaluates it. A reader who has never opened the SCIP spec must be able
to follow your doc.

## What this repo already measured. Verify every row.

| fact | evidence |
|---|---|
| SCIP is already a dl rel family in v5: `scip_def`, `scip_ref`, `scip_edge` | `.dl/gen-skill-ref.dl:39-41` |
| four v5 modules implement ingestion | `src/scip_setup.rs`, `src/scip_import.rs`, `src/rels/scip.rs`, `src/engine/extract/scip_narrow.rs` |
| field coverage went 17/43 to 43/43 | `v6/prolog/ARCH.pl:832` |
| the cost that stopped it being a standing feed | same row: 204 docs went 150,640 rows / 32.1 MB to 177,967 rows / 59.4 MB; ~17 MB of the 21.5 MB growth is CONSTANT columns; `syntax_kind` is 0 in 123,655 of 123,655 rows; verdict is it cannot hold 100% at 291k rows per 1,000 files against 74.2 files/s ingest |
| narrowing already exists as a mechanism | `--scip-record KINDS` filters AT PRODUCTION, same row |
| SCIP vs a cheap syntactic resolver, measured | scip 764 edges / 755 agree / recall .992 / precision .988; diet 761 / 761 / 1.000 / 1.000 |
| **the 9-edge finding, which is the crux** | the 9 scip-only edges reach a declaration through an INFERRED TYPE with no import statement, which no syntactic resolver closes without a type checker (`ARCH.pl:832`) |
| a pinned SCIP index was named as the fix for a real blocker | `ARCH.pl:777`, Rust call-target resolution, V5 200 / V6 168 targets, matched 113 |

That 9-edge line is the most valuable sentence anyone in this repo has written
about SCIP. It says the unique value is type-checker-derived facts. Build your
evaluation around it.

## Deliverable 1: teach the model

From the SCIP specification and its protobuf schema, state the version you
checked, then explain each concept plainly with a worked example in code the
user would recognise:

| concept | teach |
|---|---|
| the symbol string | its grammar, why it is globally unique, why it is a STRING and what that costs |
| descriptors | how a symbol names a nested thing: package, type, method, parameter, type parameter, meta |
| local symbols | why locals are cheap and non-global, and what that implies |
| Document / Occurrence / SymbolInformation | the three-level shape |
| roles on an occurrence | definition, reference, write, read, import, generated, test, forward-definition |
| relationships | `is_implementation`, `is_reference`, `is_type_definition`, `is_definition` |
| syntax kinds | and why ours were 0 in 123,655 of 123,655 rows |
| the package/moniker layer | how a symbol crosses a package boundary, which is the cross-repo story |
| what SCIP deliberately omits | no full type expressions, no bodies, no call arguments |

For each: one sentence on what it MODELS, and one on what it CANNOT say.

## Deliverable 2: the feature matrix the user asked for

The user asked "what can we use scip data types for in this system matrix of
features". Build that matrix. Rows are SCIP modelling ideas, columns are places
in THIS system they could serve:

| SCIP idea | type IR | module/visibility IR | extraction (`sprefa-extract`) | LSP features | codemod / write planning | cross-repo |
|---|---|---|---|---|---|---|

Fill every cell with `yes/no/partial` plus one clause of reason. Then a second
table with the top candidates expanded: what we would adopt, what our version
would look like under the surrogate-key law, and what it would cost.

Explicit questions to answer inside that matrix:

- The type IR now models a rel, its columns, and their types. SCIP models a
  SYMBOL and its relationships. Are those the same graph at different
  resolutions, or different graphs? Answer with an example that exists in this
  repo.
- The TypeSpec/module work happening in a parallel lane needs visibility,
  re-export, aliasing and circular imports. Does SCIP's symbol + relationship
  model already express any of those? Which, and how faithfully?
- `is_implementation` and `is_type_definition` are relationship kinds. Our type
  IR has no notion of one type implementing another. Is that a gap to close, and
  would SCIP's spelling be the right one?
- Local symbols are non-global and cheap. Is there an analogue for a rel's local
  bindings, and would it help?
- SCIP's symbol string is a parseable structured name. Our surrogate ids are
  opaque integers. What is LOST by interning, and does a dictionary table with
  the parsed descriptor columns recover it? Price both.

## Deliverable 3: the narrow adoption fork

Given the measured cost, the plausible shape is narrow adoption. Price at least
these, and find others:

| fork | shape |
|---|---|
| A. adopt the symbol/descriptor MODEL, none of the wire format | our own interned symbol table with descriptor columns |
| B. adopt relationship kinds only | add `is_implementation` and friends to the type IR, ignore everything else |
| C. narrowed on-demand SCIP as a fact source for inferred types only | the 9-edge case; `scip_narrow.rs` and `--scip-record KINDS` are the existing precedent |
| D. nothing; the syntactic scanner is enough | the 1.000-vs-1.000 reading, and what it costs to be wrong |

For each: what it buys, measured where possible, what it costs, and what it
forecloses. The user rules. You price.

Build-vs-buy applies: `scip-typescript`, `scip-go`, `rust-analyzer`'s SCIP
output and the `scip` crate are existing tools already invoked by this repo's
ratchets (`ARCH.pl:846`). Say which of them we would consume rather than
reimplement, per candidate.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| explaining SCIP from memory | cite the spec and its version; the user says they barely know it and is relying on you |
| proposing SCIP storage | explicitly rejected in the ask |
| ignoring the 59.4 MB measurement | it is why the last attempt stopped |
| ignoring the 9-edge finding | it is the only measured unique value |
| a matrix with empty cells | every cell gets a verdict and a reason |
| picking a fork | you price, the user rules |
| skipping the unga doc | a plan without it is undelivered |

## Deliverables, exactly two files
1. `plans/2026-08-12-scip-as-ir.RESEARCH.md` — TOC first, every claim carries a
   `file:line`, a spec URL, or a command and its output.
2. `plans/2026-08-12-scip-as-ir.RESEARCH.visual.human.unga.md` — plain words,
   diagrams, zero citations, nothing left undefined. REQUIRED.

Form: tables, lists, mermaid. Use a mermaid diagram for the
Document/Occurrence/SymbolInformation shape, and a worked example showing one
real symbol string decomposed into its descriptors.

## File ownership
YOURS: the two plan docs only. Everything else is READ ONLY.

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- The word "refusal" is banned in prose.
- No sycophancy, no negative parallelism ("not X, Y" / "this isn't X. it's Y").
- Surrogate keys law: stored rels key on INTEGER ids; natural keys live once in a
  dictionary table. Read `.claude/skills/sql-relational-design` and
  `.claude/skills/sqlite-costs` before pricing any storage shape.
- Docs open with a table of contents.
