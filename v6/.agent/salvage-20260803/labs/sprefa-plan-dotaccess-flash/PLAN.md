# PLAN: dot access / namespacing recon (dl6)

Worktree /Users/chrishafley/projects/sprefa-plan-dotaccess-flash, branch lab/dotaccess-flash.
Merge check `git merge --ff-only 2eceb836`: exit 0, already up to date. Proceed.

Research and planning only. Two files written at worktree root: this one (receipts, every claim
cites path:line, verified by reading) and PLAN.visual.human.unga.md (plain words, zero citations).
No commits. No just/battery targets. No subagents.

Construct names here use only prolog / rxjs / SQL vocabulary.

---

## 1. Prior considerations (recon sweep item 1)

Every prior dot / namespace / qualified-name / member-access mention found, with what was
concluded or left open.

| # | receipt | what it says | concluded / left open |
|---|---|---|---|
| P1 | `v6/prolog/LANG.md:37` | `x.field.sub` = nested pattern sugar, listed in the language Spec | kept in the surface spec |
| P2 | `plans/2026-07-27-surface-audit.md:738` | `x.field.sub` dot access verdict "keep ... cheapest construct in the spec" (references `kernel.pl:38`) | keep, but never designed past a one-line sugar entry |
| P3 | `plans/2026-07-21-v6-runtime-decomposition.md:55-84` | dot access <-> normalization: `.` compiles to a JOIN through the containment relation; `call.loc.file.rev` is two joins; datalog normalizes and flattens nesting; a WELL-TYPED PROJECTION check proves every `.field` valid so the lowered join never dangles | design sketch, two camps (destructure vs dot-as-join), not ruled |
| P4 | `plans/2026-07-27-extraction-spellings.md:511-512` | ambiguity 8: `at.line` / `span.line` sugar; the waiver range join costs two `line_of` atoms; argued AGAINST because it "would be the only dot access that is a join rather than a projection, which breaks what dot access means everywhere else" | left open. This is the named tension: dot=join (P3) vs dot=projection (here). The two prior views never reconciled |
| P5 | `chat_log/20260727.1.v6-lang-lab-waves-ruling-queue.md:73` | user: "dot access/lists/tree-sitter autogen rad"; recorded as "dot=nested pattern sugar, list_each ... sugar rows committed" | recorded as sugar only; no grammar, no resolution rules |
| P6 | `plans/2026-07-06-magic-rel-audit.md:68` | "Rel names are a single flat global namespace; one name -> one rel, no [hierarchy]" | the standing law: rels are NOT surface-namespaced today |
| P7 | `v6/prolog/conformance/rulings.pl` q8_key_vs_arrow | Key = undirected uniqueness on state rels; `->` = program/world split on effect rels | ruled both-with-law; the left-of-arrow reading stayed a residual |
| P8 | `plans/2026-07-27-extraction-spellings.md:454` and `:472` | "left-of-`->` IS the demand key, in declaration order"; "Key() wrappers are dropped from the left of -> " | feeds ruling Q8; flagged, not settled (ambiguity 6) |
| P9 | `plans/2026-07-29-v6-world-health.md:232` | "Also still open and older: keep(count) per-rel vs per-key, Q8 residual, the ..." | Q8 residual still open as of 2026-07-29 |
| P10 | `plans/2026-07-27-lab-consolidation.md:74-83` | Key(Type) vs `->`: merge lab (Key wins), audit (merge them), astgrep lab (genuinely different). "Present both files' arguments; do not resolve by fiat." | unresolved at the time; later Key() wrappers killed by decl_column_spelling |
| P11 | `/Users/chrishafley/projects/sprefa-plan-typeir/PLAN2.md:85-93` and `:99` | SCIP symbol-string grammar: `<scheme> ' ' <package> ' ' (<descriptor>)+`, descriptors Type `<name>#` , Term `<name>.`, Method `<name>(...)`; package-name = catalog, "separates catalogs so a spine column never collides with a host signature id; mirrors SCIP's namespacing intent" | today's namespacing is carried OFF the surface, in the SCIP symbol id column and its pkg/catalog field |
| P12 | `chat_log/20260723.0.v6-store-namespace-landed-reconcile-parity-closed.md:85-86` | extraction-store namespace: "a NAME PREFIX, forced: SQLite TEMP working tables ... prefix is the only ns covering" | store-schema namespacing by prefix, a different plane than language namespacing |

Adjacent open items connected:
- Key(Type) vs `->`: P7, P8, P9, P10. Left-of-arrow = demand key (P8) is the residual. A dot accessor on the left of an arrow names demand-key columns; it interacts with the Q8 residual.
- SCIP pkg field = today's fake namespacing: P11. Whatever answers question (4) must be read against the fact that namespacing already exists in the SCIP id column, not in the surface.

---

## 2. Current surface (recon sweep item 2)

- Identifier rule: `ident/3` at `v6/prolog/compile/parse_dl.pl:408-418`. Start = alpha or `_`
  (`:411`); rest = alnum or `_` (`:415-416`). A dot is never part of an identifier today.
- The bare-identifier law: `v6/prolog/compile/SYNTAX.md:23-36`, "a bare identifier is always a
  variable; an atom-literal constant is always double-quoted". A dotted spelling must not break
  this: `a.b` must not become the value of a variable named `a` followed by a terminator.
- Where `.` already appears in the grammar:
  - Statement terminator: every decl/rule/match statement ends with `lit_dcg(`.`,...)`. Receipts:
    `parse_dl.pl:1054` (rule_stmt), `:1003` (match_stmt), and fact/decl terminators at
    `:573, :594, :756, :881, :907, :929, :987`.
  - Floats: `parse_dl.pl:463` reconstructs a float code list with `0'.` inside the literal.
  - Dot is not in the operator table (expr operators are `+ - * / = < > != <= >=` around
    `parse_dl.pl:1468-1504`; comp ops at `:1376-1385`).
- So a `.`-accessor design must disambiguate accessor-dot from (a) statement-terminator-dot and
  (b) float-dot. In the DCG, a terminator dot is only consumed after a parsed statement body
  (`ws0` then `.`); an accessor dot would be written immediately before an identifier start
  char. That context rule (dot directly followed by an identifier char) is the standard
  disambiguator, but it is fragile: `x . y` (spaces) becomes ambiguous, and a bare `a.b` at
  statement boundary still reads as two statements unless the terminator tries last.
- print_dl round-trip: `task(surface_dcg, done ...)` at `v6/prolog/ARCH.pl:663`, "round-trip
  109/109". The printer (v6/prolog/print_dl.pl) emits identifiers via `format("~w", ...)` (e.g.
  `decl_line` at `:254-260`). A dotted name printed bare would re-parse ambiguously against the
  terminator rule, so the round-trip gate forces a printed form that parses back to the same
  term. Any new dotted spelling must be printed and parsed identically.
- Printer-ignore seam: `print_dl.pl` `decl_ref_order/2` at `:200-239` plus the note at
  `:245-247`; the printer emits only the refs actually present in the term program and SHUTS
  OUT constructed entries (never fabricates a decl). So the printer already tolerates "unknown"
  shapes by ignoring them; it is not the seam a trait/interface construct would attach to.
- Langium demotion: `v6/prolog/ARCH.pl:179` and `:249` and `:663` ("DCG is the CANONICAL
  parser; langium was stopgap; dl.langium stays a spelling reference only"); also
  `v6/prolog/compile/SYNTAX.md:9-11`. Verdict on "does the demotion matter here": it matters in
  the direction of LOW cost. The canonical parser is the prolog DCG (parse_dl.pl + print_dl.pl)
  plus SYNTAX.md. A new surface token only needs those three to be authoritative; dl.langium is
  a spelling reference, not a parser gate, so a grammar change there does not gate a surface
  change and does not need a langium mirror to ship.
- Corpus size for migration cost: 360 `.dl6` fixtures (find, v6 tree); 376 `rel` decls in
  `v6/dl/fixtures/*.dl6` (grep count). The v5 `.dl` corpus is larger and lives in `std/`,
  `examples/`, `src/` (see LANG.md surfaces). Any surface change is a mechanical rename sweep
  over 360+ files, not a semantic rewrite.

---

## 3. Name resolution today (recon sweep item 3)

- Rel names bind by plain functor identity. The parser builds compounds `Term =.. [Name | Args]`
  (`parse_dl.pl:1067` head, `:1443` body). A free relation name is a user functor, NOT a
  registry row.
- registry.pl is the CONSTRUCT inventory, not the rel-name table: `surface/5` at
  `v6/prolog/compile/registry.pl:12-19` and `expression/5`. It lists the fixed surface words
  (not, combine, latest, coalesce, `:=`, bind_decl, hosts...). A rel name is a name/arity used
  as a functor; nothing in registry resolves user rel names hierarchically.
- One flat global name namespace is the law (P6, `plans/2026-07-06-magic-rel-audit.md:68`).
- Column binding: decl columns are `:` typed and source-order significant (ruling
  decl_column_spelling, `v6/prolog/conformance/rulings.pl` ~2026-07-29). At call sites, named
  args resolve to positional by column order: `resolve_named_args/4` at
  `parse_dl.pl:1070-1100` (all-named, all-positional, or a genuine MIX; the corpus case
  `proves_group_count(source, fanout: count(target))` at `:1069-1070`). So member/column access
  today = positional columns, or by-name at the call site. There is no `row.column` projection
  anywhere in the grammar.
- Type pass knowledge: `v6/prolog/0_type_plane.pl`. `type_decl/2` (declared rel/struct column
  lists, `:16`), `column_storage/3` (`:119-128`), struct-as-rows storage (compound_storage
  ruling; `0_type_plane.pl:81-82` says a defined struct value is "a rel with an index column,
  declared as such" and a pattern's lowering is a function of the source column's declared
  type). `analyze.pl` mines column types from variable identity (ARCH `:175`).
- Phase-5 roadmap: the `task(clock_check, active ...)` row at `v6/prolog/ARCH.pl` (the long
  comment) pins phase-5 value rulings: bool = INTEGER NOT NULL CHECK(value IN (0,1)); float =
  finite SQLite REAL/binary64. "type pass float/REAL+avg" = this row: the type plane already
  carries declared column types, and phase 5 extends the value types.
- Where each candidate dot/namespace must resolve, per pass:
  - Parser (parse_dl.pl): ident/3 and the factor/expr postfix slot (`factor` at
    `:1500-1517`, no postfix member rule exists).
  - Analyzer (analyze.pl): a dotted atom is a new ref/column-reference shape; column mining must
    learn it.
  - Type plane (0_type_plane.pl): a `row.column` accessor's type = the declared column type of
    the rel/struct type of `row`; a namespace-qualified name resolves to a declared rel/type.
  - Strata/lower (strat.pl, lower.pl) + emit (emit_ts.pl): the atom gets a ref; a member
    projection becomes a column select or a decode join.
- Q8 residual connection: the left-of-arrow = demand key reading (P8) already implies a
  projection over an effect rel's columns; a dot accessor there names those columns by member
  instead of position.

---

## 4. Lowering doors (recon sweep item 4)

- SQL (table naming): `table_name(Name/_Arity, Name)` at `v6/prolog/lower.pl:156`; every
  identifier is double-quoted by `quote_ident` at `v6/prolog/lower.pl:195`
  (`format(atom(Quoted), '"~w"', [Name])`). Confirmed in real emitted TS: rel `eprintln_baseline`
  becomes tableName `"eprintln_baseline"` (e.g. `v6/tsv2/gen_emitted/new_file_no_exceeded_diag.ts:339`).
  Consequence: a dotted rel name `spine.files` lowers to a quoted identifier
  `"spine.files"`, which does NOT collide with `db.table` (SQLite only splits the FIRST dot and
  only in unpainted identifiers; a fully quoted `"spine.files"` is one column/table name). No
  dot->underscore mapping is required. A `::` name also lowers to a quoted string
  (`"spine::files"`) or, if we choose the readable form, to `"spine_files"` via a one-time
  replace; the choice is cosmetic, not semantic.
- The COUNT-test law applies: `ruling n1_statement_budget` = statements/tick = f(rules,strata),
  never f(rows), graded by the statement-budget rail (`v6/prolog/conformance/rulings.pl`;
  `plans/2026-07-29-finish-the-job-epic.md:1190`, `plans/2026-07-30-ts-lowering-review.md:436`).
  Any member access that lowers to a per-row recompute is a violation. A dot-as-column-select is
  flat (it is part of an existing delta statement). A dot-as-join through struct-as-rows is
  also flat (one join per level, f(levels)); it does not become f(rows). So the lowing door is
  satisfied by projection and by join-the-normalization, and fails only if someone lowers a dot
  to a per-row loop.
- TS emit: emitted plans reuse the exact rel name as tableName/column keys (observed above). A
  dotted name in generated row interfaces (spine.ts / types.ts interfaces) would need quoting or
  renaming in the type name, but the renames are mechanical. The SCIP pkg field (P11) already
  keeps catalogs apart off-surface, so TS does not need surfaced dots for catalog separation.
- rxjs lowering (language law: every construct shown must carry a pure-rxjs lowering; a dot
  construct whose rx lowering cannot be written is a design defect):
  - dot-as-projection (a column of an already-materialized rel): lowers to selecting that column
    in the existing delta SQL; no new rx construct. Boxable.
  - dot-as-join (a member that is a struct ref, `row.inner.field`): lowers to the decode join
    through struct-as-rows, already implemented in the struct plane
    (`0_type_plane.pl:81-119`, `v6/tsv2/runtime/structPlane.ts` exists). Boxable, flat.
  - namespace `::` splitting: purely a name-resolution step BEFORE lowering; the resolved rel
    is lowered as today. No rx content. Boxable.
  - Inference-ambiguity cases (see design A) are NOT rx lowerings; they are parse/resolution
    refusals, which are fine (a refusal is a value, not an rx construct).
  Verdict: every candidate design has a pure-rxjs lowering; the only construct that could be a
  design defect is an inference rule with no decidable resolution, and that is refused, not
  lowered.

---

## 5. TypeSpec model-vs-namespace oddity (recon sweep item 5)

Characterization (from the public TypeSpec docs: website/src/content/docs/docs/language-basics/
namespaces.md and models.md, fetched):

- namespaces.md: namespaces group types, "merged across files"; `namespace Foo.Bar.Baz { }` is
  a nested declaration using `.`; a fully-qualified member reference is dotted:
  `SampleNamespace.SampleModel` (namespace.md "You can then use SampleNamespace from other
  locations: model Foo { sample: SampleNamespace.SampleModel; }").
- models.md: a model is a SCHEMA: `model Dog { name: string; age: uint8; }`; properties are
  named fields with source-order (models.md "Property ordering ... arranged in the order they
  are defined in the source").
- The oddity: ONE `.` token does THREE jobs, and TWO container kinds share the same dotted
  grammar.
  1. Nested namespace declaration: `namespace A.B.C` (segments).
  2. Namespace member type resolution: `A.B.SomeType` in TYPE position (the checker walks the
     namespace tree; the left side MUST be a namespace).
  3. Value property access: `value.property` at the value/emit layer (left side is a model
     value).
- The weirdness at the boundary: resolution is keyed on the KIND of the left side and the
  grammar position. In type position the left side of a dot must be a namespace; a model is not
  a valid qualifier (you cannot write `Dog.name` to get the type of a property: the checker
  looks for a namespace named Dog). In value position the left side is a model. A namespace and
  a model with the same name cannot coexist (one symbol table). So a dotted name's meaning
  flips on type-vs-value position, and the two container kinds never meet: you can dot a
  namespace to reach a type, or dot a model value to reach a property, but never mix.
- This is the failure shape the owner flagged: the reader cannot tell what `A.B` means from the
  spelling alone; it depends on whether `A` is a namespace or a model and whether the use site
  is a type or a value position.

How the design space below avoids or reproduces it (the acceptance test):

| design | reproduces? | why |
|---|---|---|
| One symbol `.`, two jobs (namespace + member), inference (design A) | REPRODUCES partially | `spine.files` is ambiguous between "namespace spine, member files" and "rel spine, column files" and "type spine, field files". The reader and the resolver both have to guess from the kind of the left symbol, which is exactly the TypeSpec flip. Some cases refuse cleanly (both readings exist), but the spelling alone never disambiguates. |
| `::` namespaces + `.` members (design B) | AVOIDS | the two containers get two glyphs. `spine::files` is always a namespace member; `row.column` is always a value/type member. No shared dotted grammar, no kind-of-left-side flip. This is the direct counter to the named failure mode. |
| No surface dots (design C) | AVOIDS trivially | no dot anywhere; namespacing already in the SCIP id column. |
| `::` only, postpone `.` member access (design D) | AVOIDS | same glyph separation as B; the member side is deferred, so the conflation cannot arise. |

The primary acceptance test: a design "works correctly and without oddities like how typespec
is odd with model vs namespace" iff a dotted/namespace name's meaning is decidable FROM THE
SPELLING (plus declared kinds), never from an ambiguous left-side kind. B and D pass; A passes
only where the reader can see the kind; C passes by having no dots.

---

## 6. Design packet (does NOT pick)

### A. One `.` for both namespace and member access, resolution by inference

- Parse cost: extend `ident/3` (`parse_dl.pl:408-418`) and add a postfix member factor in the
  `factor/5` slot (`:1500-1517`). Calibration: ident is 10 lines; `resolve_named_args/4`
  (`:1070-1100`) is ~30; the named-arg lookahead at `atom_arg/5` (`:1080-1084`) shows a
  precedent for a 2-token lookahead. Estimate 25-45 lines total, plus float/terminator
  disambiguation (a dot directly followed by an identifier char) and a print_dl clause that
  prints the dotted compound back identically (round-trip gate, ARCH:663).
- Resolution rules (prolog sketch, not code):
  - `qualified(X.field, member_of(Type, Column))` when `X` resolves to a rel/type with declared
    column `field`.
  - `qualified(Ns.Member, namespace_of(Ns, Member))` when `Ns` is a declared namespace.
  - `qualified(X.field, ambiguous)` when both readings are live.
  - Inference enters where the left side is a rule VARIABLE that could be a struct value OR a
    name. That is where the oddity bites (see ambiguity below).
- Worked ambiguous example: program declares rel `row(file: text, column: text)` and a
  namespace `row` containing a type `column`. Body goal `row.column`:
  - If `row` is a bound struct/rel variable, `row.column` = member projection (type = text).
  - If `row` is the namespace, `row.column` = the type `row.column` (a type ref).
  - The spelling is identical. Resolution must refuse `ambiguous_dot(row.column)` rather than
    guess. That refusal is honest but it is exactly TypeSpec's "meaning depends on the left
    kind" and it will fire on real corpus names (e.g. `spine.files` where a spine rel and a
    spine namespace could both be plausible).
- SQL: projection = column select (flat); namespace = quoted table name (lower.pl:156,195).
- TS: property/type rename for dots, mechanical.
- rxjs: boxable as shown (section 4).
- Migration: low token change, but ambiguous names in a ~376-decl fixture corpus would need
  disambiguation renames.
- TypeSpec verdict: reproduces the oddity (table above).

### B. `::` for namespaces + `.` for members

- Parse cost: add a `::` token (ident `::` ident)* for namespace-qualified names; `.` becomes a
  pure postfix member factor. `::` has NO collision: it is not a terminator, not part of any
  float, not an operator today. The `.` member still needs the terminator/float
  disambiguation, same as A. Estimate 30-50 lines + print_dl mirror + SYNTAX.md.
- Zero ambiguity: `spine::files` is always namespace-member; `row.column` is always
  member-access on a typed value. The resolution is decidable from spelling + declared kinds,
  no inference on the left-side kind.
- Reads next to prolog's module convention: prolog uses `:` for functor/module qualification
  (the repo uses prolog module `:` throughout parse_dl.pl/print_dl.pl/registry.pl). `::` is the
  adjacent doubled form; the vocabulary law admits prolog words, and `:` is the prolog module
  pun already familiar in this codebase. Note the contrast explicitly so it does not bite:
  prolog's `:` is a module-qualified CALL (`mod:pred`), while `::` here is a name separator for
  DL rels/types; both are namespacing, which is consistent with the vocabulary law.
- Resolution rules:
  - `ns_member(spine::files, ResolvedRel)` = namespace lookup: `namespace(spine)` and
    `member(spine, files)` declared; else refuse `unknown_namespace(spine)` or
    `unknown_member(spine, files)`.
  - `member_projection(Row, .column, Type)` = `declared_type(Row, Col)` and `col(Col, Type)`;
    else refuse `unknown_column(Row, column)`.
- SQL: `spine::files` -> `"spine::files"` (quote) or `"spine_files"` (replace) per a chosen
  convention; `.` member -> column select or decode join. Flat per COUNT-test (section 4).
- TS: mechanical rename of `::` to a flat id in generated interfaces; SCIP pkg already
  separates catalogs (P11).
- rxjs: boxable (section 4); `::` is resolved before lowering.
- Migration: mechaniical; the `::` names need a namespace decl or must be refused, so old
  flat names migrate by declaring a top namespace or by keeping flat (namespace optional).
- TypeSpec verdict: avoids (table above).

### C. No surface dots: namespacing stays in symbol strings / underscore prefixes

- This is today's answer (P6 flat law; P11 SCIP pkg field; the `files` / `files_at` naming
  rule in rulings.pl `files_naming`, 2026-07-31). Zero parse cost, zero migration.
- The ergonomic loss stated honestly: (a) rel names are flat globs, so `spine.files` must be
  written `spine_files` or reached by a SCIP-id join; (b) there is NO `row.column` projection;
  member read = named/positional args only (`resolve_named_args`, parse_dl.pl:1070-1100) or a
  `decode(body, {..})` join (0_type_plane.pl struct-as-rows); nested access on a struct value
  in expression position does not exist. The Q8 residual (P8) stays: no left-of-arrow
  member projection without a helper.
- TypeSpec verdict: avoids trivially (no shared dotted grammar).

### D. `::` for namespaces now, `.` member access deferred

- The recon's strongest finding is that the only real surface "namespace" need is already
  served OFF-surface by the SCIP pkg field (P11) and the flat-rel law (P6), while member access
  (`row.column`) is the genuinely missing surface. So option D = add `::` only if a surface
  namespace is actually wanted, and treat `.` member access as its own follow-on. Minimal:
  obtainable.

Cheat-sheet summary

```
            parse(L)  collision  resolution             TS     rx      typespec
A one `.`   25-45      term./flt  infer, refuses ambig   mech   box     REPRODUCES
B `::`+`.`  30-50      `.` only   decidable from spell   mech   box     AVOIDS
C none        0        none       n/a (no dots)          n/a    n/a     AVOIDS
D `::` only 15-25      none(::)   decidable              mech   box     AVOIDS
```
All four keep the COUNT-test law (f(rules,strata)). A is the only one that reproduces the
owner's named failure mode.

Blocking and open (see section 9 for the owner-facing questions).

---

## 7. Interfaces / traits

Find everywhere the codebase already fakes a contract/interface, then the verdict.

1. Host executor contracts: hand-maintained positional column tables.
   `host_executor_contract/2` at `v6/prolog/compile/registry.pl:334-338` (`sprefa_extract`,
   `sprefa_extract_repo`, `shell`); `host_input_contract/3` at `:345-399` (role split:
   identity vs freshness). These are the row-shape contracts for host executors, selected by
   `host_execution/3` at `:304-332`.
2. TS `I` interfaces law: structural host interfaces named `I...` in the TS runtime,
   e.g. `v6/tsv2/runtime/types.ts:42 ISqlSeam`, `:61 IRelDelta`, and ~20 more through `:442
   IServedProgram extends IGenProgram`. These live in the HOST, never surfaced into the DL
   language.
3. one-rel-one-rule-kind law: DEAD. `v6/prolog/ARCH.pl:67` "MIXED HEADS ARE SOUND (supersedes
   v5's one-rel-one-rule-kind law...)". The stale comment survives at `print_dl.pl:95` and
   `text_door_receipt.pl:30` as a leftover mention of a superseded law.
4. lang_ext printer-ignore seam: the `decl_ref_order/2` shutter at `print_dl.pl:200-239` plus
   the no-fabrication note `:245-247`; the printer ignores constructed/unknown shapes rather
   than typing them. This is a print tolerance, not a contract.

Verdict: NO first-class interface/trait construct pays for itself. Argued from what the code
needs, not taste:

- The only real "contract" facts are host row-shape contracts (registry.pl:334-399). They are
  ALREADY the right shape as positional column lists: a trait would add nothing but a name over
  a list of columns, and the rel `type_decl/2` / struct-as-rows mechanism (`0_type_plane.pl`)
  already provides the named column-list type. Wrapping the contract table in a trait type adds
  a type-with-members feature whose single consumer would be the same column lists.
- The structural `I` interfaces exist only in the host (TS) and are the host's own typing; a DL
  trait type does not reach them. Making them DL-level would move host concerns into the
  language for no resolvable need (structural typing has no subtyping counterpart in the
  datalog core, and no rule asks for one).
- The one-rel-one-rule-kind law (the only "one kind per rel" idea) is already superseded
  (ARCH.pl:67). There is no rule-kind-enforcement need left to satisfy.
- The smallest honest construct that WOULD cover the found cases, if one is ever wanted, is a
  named column-list alias: a `type`/struct declaration (which is what `type_decl/2` already
  is). Nothing larger is justified. So: no, the fakes are the right shape; add an interface or
  trait only when a concrete rule needs to constrain a rel/type by an ABSTRACT contract, and no
  such rule exists in the corpus today.

---

## 8. ARCH-style task rows

Statuses all unbuilt. Shape task(Name, Status, Needs), consistent with
`v6/prolog/ARCH.pl:651` and rows `:655/:751/:800`.

```
task(dotaccess_recon,              done,    []).
task(dot_parse,                    unbuilt, [dotaccess_ruling]).      % ident/3 extension or :: token + postfix member factor (parse_dl.pl:408-418, :1500-1517) + terminator/float disambiguation + print_dl mirror (round-trip 109/109, ARCH.pl:663)
task(dot_resolution,               unbuilt, [dot_parse]).             % namespace table + declared-column member lookup; refuse ambiguous_dot/1 (design A) or decidable spelling checks (design B/D)
task(dot_type_plane,               unbuilt, [dot_resolution]).        % row.column accessor type = declared column type; namespace-qualified name -> declared rel/type (0_type_plane.pl)
task(dot_lowering,                 unbuilt, [dot_type_plane]).        % SQL projection/join (lower.pl:156,195; COUNT-test flat), TS rename, pure-rxjs boxing per door (section 4)
task(dot_migration,                unbuilt, [dot_parse]).             % corpus sweep: 360 .dl6, 376 rel decls; flat->dotted/:: renames
task(namespace_symbol_ruling,      unbuilt, []).                      % owner answer to question (4): :: needed as a surface symbol or SCIP pkg (PLAN2.md:99) suffices
task(member_access_ruling,         unbuilt, []).                      % owner answer to question on `.` member access scope (projection only vs projection+join)
task(trait_construct_ruling,       unbuilt, []).                      % owner answer to question (5): fakes are the right shape (section 7)
```

---

## 9. Blocking questions (owner-facing, each answerable in one sentence)

In the order they block:

1. (blocks everything) Do you want a surface `.` at all, or is member access by call-site
   named/positional args (parse_dl.pl:1070-1100) plus the SCIP pkg id (PLAN2.md:99) the ceiling
   for now, so we take design C (no surface dots)?
2. (blocks the parse row) If a surface `.` is wanted, is `spine.files` meant to name a MEMBER
   of a typed `spine` value (projection, design B/D member side) or to qualify which rel/type
   (namespace, design A/B namespace side), given the recon shows the two readings are spelled
   identically today and that identity is the TypeSpec failure mode?
3. (blocks design A) If we keep ONE `.` for both jobs (design A), do you accept an
   `ambiguous_dot/1` named refusal whenever `row.column` resolves both as a member and as a
   namespace member, instead of silent inference?
4. (blocks the namespace row) Do you need actual namespace `::` symbols, or is the SCIP pkg
   field (P11, PLAN2.md:99) plus the flat-rel law (P6) already the namespacing you want, making
   `::` a pure ergonomic add with zero semantic need?
5. (blocks the trait row) Do you need interfaces/traits for any specific reason beyond the host
   row-shape contracts (registry.pl:334-399), or are the fakes already the right shape (section
   7)?
