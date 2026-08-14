# BRIEF: TypeSpec parity for the module and visibility IR. Selected parity, not all of it.

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha; that is your base. The ordering is not a gate. If a procedural
line in this brief seems to forbid otherwise-correct work, the work wins: note
the conflict in your report and keep going.

**Docs only. Write ZERO implementation code.** Two plan docs are the deliverable.
Language design is settled with the user in the room; your job is to price forks
so the user can rule cheaply.

## The user's words, verbatim, and the scope fence

> "yes these are also typespec things. its time to reach for parity with typespec
> and its codegen/type modeling. NOT ALL OF IT!!!!!! yes visibility (we should
> model all known common practically used ones (i hate private, its public or
> not really but we must parse all bc i want to import a rust file and just
> read(TM) its types into dl6 if i so choose one day). yea we need re-export
> expression and esm bare level, and path aliasing, and circular yes, and
> deferred/dynamic/async imports/loading in module system or codegen as ir
> somehow to say its late."

**NOT ALL OF IT** is shouted for a reason. You are chasing SIX named features and
nothing else. Every section of your doc maps to one of them. If you find a
seventh TypeSpec feature that seems essential, put it in a clearly labelled
"out of the named six, proposed" appendix and keep it under half a page.

The six named features, plus one more scope statement in section 2b:

| # | feature | the user's constraint |
|---|---|---|
| 1 | visibility | model the commonly used ones; PARSE all of them even where we do not model them |
| 2 | re-export expression | must be expressible |
| 3 | ESM bare specifiers | bare-level module names, not only relative paths |
| 4 | path aliasing | an alias resolving to a real location |
| 5 | circular imports | legal, must be representable |
| 6 | deferred / dynamic / async import | the IR must be able to say "this arrives LATE" |

On visibility, read the user's parenthetical carefully. Their opinion is that
`private` is not a real distinction ("its public or not really"), so the MODEL
may be small. The PARSER must still accept every visibility keyword a source
language uses, because the goal is to one day read a Rust file's types straight
into `.dl6`. Parsing breadth and modelling breadth are different budgets. Say
what each one costs separately.

On #6, "say its late" is the requirement. A deferred import is a property of an
edge in the module graph, and the IR needs a spelling for it. Whether that
property changes codegen (an `import()` expression in TypeScript, a lazy static
in Rust) is a second question. Separate them.

## What exists today. Measured, verify each row.

| fact | evidence |
|---|---|
| the catalog HAS module rows | `v6/prolog/lower.pl:1389`, `row(ModuleId, 0, 0, ModuleName, module, 0, 0, ModuleId, ModuleHash, '', '')` |
| modules spliced through `use "path".` are tracked | `catalog_spliced_module_rows/6`, `lower.pl:1391` |
| every rel maps to a module | `rel_module_map/3`, `lower.pl:1395` |
| one shared import resolver, both compiler doors | `v6/prolog/use_resolve.pl`, 333 lines |
| the JSON Schema emitter reads module rows | `v6/prolog/compile/4_emit_jsonschema.pl:37`, `:104`, `:107` |
| the TS and Rust type renderers DISCARD the module id | `compile/7_emit_ts_types.pl:17` and `8_emit_rust_types.pl:17` both bind `_ModuleId` |
| the naming/collision step is designed and unbuilt | `plans/2026-08-12-type-ir-polyglot.PLAN.md:120` |

So modules are half-built: identity and membership exist, everything the six
features need does not. Confirm that reading or correct it.

Read `plans/2026-08-12-type-ir-polyglot.PLAN.md` (250 lines) and its unga twin
BEFORE starting. It already priced the emitter shapes and the expressive
frontier. Do not re-derive it; cite it and extend it.

## Deliverable 1: what TypeSpec actually does, for the six only

Research the CURRENT TypeSpec, from its own documentation and source, and state
the version you checked. For each of the six:

- the exact TypeSpec surface syntax
- what its compiler represents internally
- how its emitters consume it
- what it deliberately does NOT do

TypeSpec's visibility model is the deepest of the six and is lifecycle-based
rather than a single keyword. Get it right: what the visibility classes are, how
they interact with operations, and what an emitter sees.

Note the connection and use it: **alloy-js is the TypeSpec team's code generation
framework**, and its `<Output>`/`<SourceDirectory>`/`<SourceFile>` model plus
refkey symbol resolution is how TypeSpec emitters place declarations into files.
The user asked about alloy separately. Say how TypeSpec's module model and
alloy's output model fit together, because that pairing is the actual prior art
for what is being built here.

## Deliverable 2: the source-language survey, for visibility and imports

The stated goal is reading a foreign file's types into `.dl6`. So survey what
must be PARSED:

| language | visibility keywords | import forms to survive |
|---|---|---|
| Rust | `pub`, `pub(crate)`, `pub(super)`, `pub(in path)`, bare | `use`, `pub use` (re-export), `mod`, path aliasing via `as` |
| TypeScript | `export`, `export default`, `declare`, `public`/`private`/`protected` on members | bare specifiers, relative, `export * from`, `export { x as y } from`, `import()`, `import type`, path aliases in tsconfig `paths` |
| Go | capitalisation as visibility | import aliasing, blank and dot imports |
| Python, Java, C# | as relevant | |

For each row say what the IR would have to hold to round-trip it, and what it
would lose. A table with a "lossy?" column.

## Deliverable 2b: nominal type kinds. Enum, interface, trait.

Second scope statement from the user, verbatim:

> "yes we need enums/interfaces/traits and be able to express those kinds, they
> are natural in all languages to a degree. go is the floor, rust is the
> ceiling, typescript is turing complete and can play doom so there is that"

That sentence names the design constraint precisely: **the IR must be
expressible at Go's level and must not squander what Rust can say.** Go is the
floor because its interfaces are structural, it has no sum type, and its enums
are untyped constant blocks. Rust is the ceiling because it has real sum types,
traits with associated types and coherence rules. TypeScript's type system is
Turing complete, so it can express nearly anything and is therefore useless as a
constraint; treat it as an unbounded target rather than a guide.

### What already exists here, PROBED not assumed

A top-level enum rel already IS a reusable named sum type, because it declares
no `key(...)` and is therefore a pure type declaration wearing the `rel` keyword.
Probed 2026-08-12, `rc=0`:

```
rel body(page(view: text) ; redirect(to: text)).
rel response(id: int, payload: body).
```

emits `body_page`, `body_redirect`, `body_tag` (the discriminator) and
`response.payload` holding the enum id. Verify this yourself before building on
it. A coordinator claimed the opposite this session and was corrected by the
user, so probe before pricing anything as impossible.

Also known: `option(<enum>)` is still stopped, at
`v6/prolog/0_option_expand.pl:43`, and it is a PHASE ORDER accident rather than
a design. `v6/prolog/1_expansion.pl:28-29` runs option at phase 5 and enum at
phase 10, so at phase 5 the enum is still `enum_decl/2` with no col_type rows.
Say whether that matters for this work.

Interfaces and traits have NO spelling, and the type IR has no notion of one
type implementing another.

### What to deliver for this section

| question |
|---|
| what is the FLOOR: what can Go express of enum, interface, trait, and what shape must the IR take to reach it |
| what is the CEILING: what Rust traits add (associated types, generic impls, coherence, default methods, blanket impls) and which of those the IR should carry |
| for each of the three kinds, the surface syntax in Go, Rust, TypeScript, and TypeSpec, side by side in one table |
| structural versus nominal: Go interfaces are structural, Rust traits are nominal with explicit impls, TypeScript is structural. Which does the IR pick, and what does the other cost to emulate |
| does `implements` become a relationship in the IR? Note the parallel SCIP lane is asking the same question from the other side, since SCIP already has `is_implementation` and `is_type_definition` relationship kinds. Say whether that spelling fits |
| enum: is the existing enum-rel form sufficient, or does it need a distinct declaration form now that it must also serve as a named exported type in three targets |
| exhaustiveness: match exhaustiveness already forces all variants here. What does each target do with it |

Keep the TypeScript column factual and short. Its type system being Turing
complete means it can encode anything; that is not a reason to design for it.

## Deliverable 2c: TWO USER RULINGS. These are settled. Do not re-open them.

### Ruling 1: the annotation plane is chosen

The user was presented three forks for carrying target-specific detail and RULED
for the annotation plane:

- A. one generic annotation surface; every rendering-only difference lives there;
  the core type system does not grow. **CHOSEN.**
- B. richer core types (ownership, mutability, bounds as first-class `.dl6`
  syntax). Rejected.
- C. per-target sidecar profile files keyed by rel name. Rejected.

Measured context you must verify: `.dl6` has NO annotation surface today. The
only `@` in the 397-file corpus is `@eprintln`, 16 occurrences, and that is a v5
source-comment waiver rather than a `.dl6` construct.

Also measured: every Rust type the renderer emits is OWNED
(`v6/prolog/compile/8_emit_rust_types.pl:48-58`: `i64`, `f64`, `String`,
`serde_json::Value`, `Vec<T>`, `Option<T>`). No borrow is ever emitted, so no
lifetime is reachable. Lifetimes are therefore absent by POLICY, not by limit.
`.dl6` needs no lifetime syntax; a renderer needs an ownership policy, and the
lifetime falls out of it. Treat lifetimes, `mut`, derive lists and struct tags
as annotation-plane concerns and price them there.

Design the annotation plane. Answer, with prices:
- the surface spelling, and whether annotations attach to a rel, a column, or
  both
- how a renderer declares which annotation keys it understands
- what happens to an annotation no renderer claims: dropped, warned, or an error
- whether annotation values are typed or opaque text, and what each costs
- whether annotations participate in the catalog `row/11` or need their own rows
- how this interacts with the existing `sh` host decl and `bind` surfaces, which
  are the closest existing thing to declaration-level metadata

### Ruling 2: `.dl6` gets interfaces

The user's words: "we do need interfaces in dl6 tho, like something that could
hope to turn into trait and interface in rs/ts".

So an interface construct is IN SCOPE and required. The design question is what
it constrains, because the three targets disagree:

| target | an interface is |
|---|---|
| TypeScript | STRUCTURE. A set of fields. |
| Go | BEHAVIOUR. A set of methods, satisfied structurally with no declaration. |
| Rust | BEHAVIOUR. A trait, nominal, needing an explicit `impl`. |

One candidate shape, offered as a starting point and NOT as a decision. A `.dl6`
interface as a column-shape constraint, since `.dl6` is a data language and a rel
is a set of rows:

```
interface addressable(path: text, digest: text).

rel file(path: text, digest: text, bytes: int) :- addressable.
rel blob(path: text, digest: text)             :- addressable.
```

lowering to a TypeScript interface directly, a Go interface of accessor methods,
and a Rust trait of accessor methods plus a DERIVED `impl` per conforming rel.

Price that shape and at least two alternatives. Answer specifically:
- structural conformance (a rel conforms by having the columns) versus declared
  conformance (a rel names the interfaces it satisfies). Which, and what does the
  other cost
- can an interface carry anything beyond columns, and should it
- how does an interface interact with the enum rel form that already exists
- what the emitted Rust `impl` looks like and who writes it
- whether `implements` becomes a relationship in the type IR. The parallel
  `plans/scip-as-ir` lane is examining SCIP's `is_implementation` and
  `is_type_definition` relationship kinds from the other direction. Consider
  whether that spelling fits rather than minting a new one
- what an interface means for a rel that is DERIVED rather than stored

## Deliverable 3: the fork table. This is what the user reads.

For each of the six, present forks with prices, not a decision:

| fork | what changes in the IR | what changes in `.dl6` surface | emitter cost per target | what it makes possible | what it forecloses |

Anchor every price against real numbers already in this repo: the JSON Schema
emitter is 176 lines, OpenAPI 103, and the new TS and Rust type renderers are 69
each. A fork claiming "small" next to those numbers should say how small.

Say explicitly for each fork whether it needs a new `.dl6` spelling. A fork that
needs no new surface is dramatically cheaper and the user should be able to see
that at a glance.

## Deliverable 4: circularity, treated properly

Circular imports being legal has consequences the other five do not:

- what breaks in a topological emitter when the graph has a cycle
- how TypeScript, Rust and Go each actually handle a cycle at runtime, and where
  each one fails
- whether the IR should permit a cycle, detect it, or annotate it
- how it interacts with feature 6, since a deferred import is the standard way
  to break a cycle

This section decides whether the emitter's "deterministic topological order"
from the recon doc survives at all. Say so plainly.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| summarising TypeSpec from memory | it moves; cite the docs and the version |
| covering all of TypeSpec | the user shouted NOT ALL OF IT |
| collapsing parse-breadth and model-breadth | they are two budgets and the user separated them |
| "circular imports are fine" | say what breaks, per target |
| picking a fork | you price, the user rules |
| skipping the unga doc | a plan without it is undelivered |

## Deliverables, exactly two files
1. `plans/2026-08-12-typespec-module-ir.RESEARCH.md` — TOC first, every claim
   carries a `file:line`, a URL, or a command and its output.
2. `plans/2026-08-12-typespec-module-ir.RESEARCH.visual.human.unga.md` — plain
   words, diagrams, zero citations. REQUIRED.

Form: tables, lists, mermaid. Prose is a one-line caption under a diagram. Use a
mermaid flowchart for the module graph with a cycle and a deferred edge in it.

## File ownership
YOURS: the two plan docs only. Everything else is READ ONLY.

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- The word "refusal" is banned in prose; an error for an unbuilt construct is
  "TODO" or "not built yet".
- No sycophancy, no negative parallelism ("not X, Y" / "this isn't X. it's Y").
- Construct names use ONLY rxjs, prolog, or SQL words.
- Docs open with a table of contents.
