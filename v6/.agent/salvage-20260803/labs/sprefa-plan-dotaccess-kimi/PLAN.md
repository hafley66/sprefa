# PLAN: dot access / namespacing recon + costed design space (dl6)

Lane: lab/dotaccess-kimi, base 2eceb836. Research + planning only.
Every claim cites path:line, each verified by reading the file before writing
this plan. Typespec claims cite fetched doc URLs.

Merge receipt: `git merge --ff-only 2eceb836` -> "Already up to date", exit 0.
No commits made. Two files written, both at worktree root.

## Deviations from the brief (recorded per lane law)

1. Brief path `v6/prolog/rulings.pl` does not exist. The rulings file is
   `v6/prolog/conformance/rulings.pl`; the grep for dot/namespace/qualified/
   member/`::` ran there (result: zero dot rulings, see R1.13).
2. Brief's "identifier rule at parse_dl.pl:411-416 region": the clause spans
   `v6/prolog/compile/parse_dl.pl:408-418`. Substance matches the brief.
3. Brief's "lang_ext printer-ignore seam" does not exist under that name
   (grep for lang_ext across v6/ returns nothing). The real seam with that
   shape is the `sh` raw-template door; see section 7, fake 2.
4. `ARCH.pl:663` citation checks out exactly (langium demoted, DCG canonical).

## R1. Prior considerations: the full inventory

R1.1  v4 audit, `chat_log/20260515.7.dot-access-audit.md` (whole file). Dot
access in v4 was parsed everywhere but meant nothing structural: `Cursor::get`
(lib.rs:513) mangled `X.field` to the flat key `X_FIELD` (:10, :70-84, :121-125).
Three inconsistent implementations (parse, `FormatComponent` regex, `where_eval`
flat-name) at :14, :349-372. Nested `a.b.c` not parsed (:167-173). Verdict of
record: document `${X.field}` as an awk-style alias for `${X_FIELD}`; "the
cursor does not navigate structure" (:331-345). v4 conclusion: dot was a string
shim, never ruled as navigation.

R1.2  `chat_log/20260516.2.hafley-sprefa-inner-graph-audit-dot-path-resolve.md`
:18-24: owner asked for paths first-class with no new value type and no open
dispatch; proposed closed `Dot` trait on `CursorValue`, deleting the three v4
shims. Left open at :55: "Does `.field` stay a string-namespace pattern, or get
true `Path` variant nav now?" Never implemented; v4 was superseded by v6 before
this landed.

R1.3  THE design doc, `plans/2026-07-21-v6-runtime-decomposition.md:55-104`.
"Dot access <-> normalization": `.` COMPILES TO A JOIN through the
normalization relation (:64-67); storage is Souffle-style record interning
(:92-94); surface is ONE `Proj` (dot) operator whose lowering dispatches on the
field's KIND: functional column -> one join, record field -> record-table
lookup, ADT branch accessor -> match/guard, relation-valued -> fan-out join
(:95-102). "Nesting must make sense" = the checker proves every `.field` valid
on its receiver (:71-73, :104). IR sketch carries `Proj(Box<Term>, SymId)` at
:241. Status: written for the Rust-crate v6 that was not built; the prolog
compiler shipped instead, without the dot spelling.

R1.4  `plans/2026-07-27-surface-audit.md:738`: `x.field.sub` dot access,
verdict "keep", reason: "`kernel.pl:38` has its sugar entry; cheapest construct
in the spec."

R1.5  The sugar entry exists: `v6/prolog/src/kernel.pl:38`,
`sugar(dot_access, [ground_terms]).` with the comment "x.f.g = nested pattern".
Committed in 7c530cb2 (2026-07-27) alongside list_each and typed_decode. This
is a registry row in a grader kernel, not a compiler construct; parse_dl.pl has
no dot production (verified by reading the expr grammar, section R2).

R1.6  `plans/2026-07-27-extraction-spellings.md:510-513` (ambiguity 8): should
`at.line` desugar to the waiver view join? Argument against recorded verbatim
in the plan: it would be the only dot access that is a join rather than a
projection, "which breaks what dot access means everywhere else." Left open.
This is the sharpest prior statement of a semantics law for dot: dot means
projection (functional), and a fan-out reading needs a different spelling.
Note the tension with R1.3's relation-valued field kind (fan-out join); that
conflict was never ruled.

R1.7  `plans/2026-07-30-lab-assimilation-sweep.md:51`: dotted-path enumeration
(`orders[*].id`) marked SUPERSEDED; the recorded reason: a dot path is a join
chain over ref columns, which the types-as-rels verdict already covers as
ordinary body atoms. The bespoke path enumerators built in two labs were ruled
unreachable from the current design.

R1.8  `chat_log/20260727.1.v6-lang-lab-waves-ruling-queue.md:73`: owner said
"dot access/lists/tree-sitter autogen rad"; implementer line: "dot=nested
pattern sugar". Same file, :48, a prolog strain receipt that matters for term
form choice: "dict dot-access silently truncates chains on a space" (SWI dict
`.` syntax is not safe to lean on at the term door).

R1.9  Key(Type) vs `->`: `plans/2026-07-27-lab-consolidation.md:74-82` (labs
split three ways, user decision pending); ruling Q8 is the open question
(`plans/2026-07-27-aggregate-analysis.md:264`); the "left-of-`->` IS the demand
key, in declaration order" reading is at
`plans/2026-07-27-extraction-spellings.md:454`. Connection to dots: a dot
chain can start on a demand-key column, but projection is a read, so dot
resolution does not depend on Q8's outcome. Q8 blocks keyed-write spellings,
not reads.

R1.10 Today's fake namespacing, sibling lane:
`/Users/chrishafley/projects/sprefa-plan-typeir/PLAN2.md:99` puts namespacing
in the SCIP symbol package-name field ("separates catalogs so a spine column
never collides with a host signature id; mirrors SCIP's namespacing intent");
symbol format grammar at PLAN2.md:85-101 (scheme `typeir`, catalog as
package-name, constant version `dev`). Namespacing lives inside symbol
STRINGS, invisible to the dl6 parser.

R1.11 v5 row-level dotted names: `plans/2026-07-06-magic-rel-audit.md:68-71`:
rel names are a single flat global namespace; the `file::kind::name`
disambiguation lives at the row/sym level as DATA (`call_def.sym` is a dotted
string inside a row, not a qualified identifier). Scope-dotted syms also at
`plans/2026-07-21-sym-dict-correctness-proof.md:85`.

R1.12 Namespace deferrals on record: isomorphism spec open question 3, "Share
across scripts/namespaces: is a rel global to one program, or shared"
(`v6/plans/2026-07-23-v6-reactive-datalog-isomorphism.md:134`), still open in
`chat_log/20260723.1.v6-type-system-bootstrap-isomorphism-locked.md:106` (fork
3). Store-level prefix namespacing (GraphNs) exists in the TS engine
(`v6/plans/2026-07-23-opus-dispatch.md:26`) and is engine plumbing, never
surface syntax.

R1.13 `v6/prolog/conformance/rulings.pl`: grep for dot, namespace, qualified,
member, `::` returns zero rulings (only :18, the word "membership" in the set
comment). Dot access has never been ruled in the rulings file.

R1.14 `v6/prolog/ARCH.pl:663` (surface_dcg, done): the prolog DCG in
parse_dl.pl is the CANONICAL parser; langium demoted to a spelling reference.
Round-trip was 109/109 at that merge; current count is 136/136
(`v6/prolog/compile/SYNTAX.md:359`).

Summary of the inventory: dot access was designed once in full (R1.3),
kept in one audit (R1.4), registered as sugar in a grader kernel (R1.5),
given one semantics law candidate (R1.6), given one SUPERSEDED reading (R1.7),
and never ruled in rulings.pl (R1.13). Namespacing was deferred twice (R1.12)
and faked twice with strings (R1.10, R1.11).

## R2. Current surface: what `.` collides with

R2.1  Identifiers: `v6/prolog/compile/parse_dl.pl:408-418`. Start char is alpha
or underscore; rest is alnum or underscore. Case is not semantically
distinguished at the lexer (:402-406 comment). `.` is not an ident char.

R2.2  `.` IS the statement terminator. Decl statements end with
`lit_dcg(\`.\`)` at parse_dl.pl:573 and :594; rule/query statements likewise
(read at :538-547 dispatch plus the rule_stmt production). dl.langium agrees:
`'.'` terminates RelDecl (:30), DlRule (:67), QueryStmt (:126). Collision
verdict: none for a postfix dot chain, because the chain only starts after an
ident inside an expression, and the terminator only appears where a statement
ends. The one adjacency, `... P = x.file.` (chain then terminator), resolves by
backtracking: the postfix clause needs an ident after `.`, fails on the
terminator, and the terminator is then consumed by the statement production.
Precedent that this exact greedy-then-backtrack boundary already works: float
literals, where `1.5.` parses as float then terminator
(float_lit at parse_dl.pl:435-441, tried before integer_lit in factor at
:1507-1508).

R2.3  Operator tables: prolog ops declared at parse_dl.pl:76-78 (`<-`, `<+`,
`:=`); the surface expression inventory is registry `expression/5` rows at
`v6/prolog/compile/registry.pl:232-248` (arithmetic, comparisons, norm/1).
No `.` operator anywhere. print_dl takes parenthesization precedence from
those same registry rows (`v6/prolog/print_dl.pl:501-503`), so one new row is
the whole printer-precedence change.

R2.4  `:` is taken, `::` is free. Named args use `:` with `:=` and `::`
explicitly excluded by lookahead (parse_dl.pl:1081-1084). Typed captures use a
second `:` (parse_dl.pl:1640-1643, with the note that `:` is 600 xfy in SWI at
:1631). `::` appears in no fixture (grep over v6/dl/fixtures/*.dl6 and
v6/prolog/compile/dl_view/*.dl6: zero hits) and has no production; it is
unclaimed surface.

R2.5  Expression grammar shape: expr -> add_expr -> mul_expr -> factor
(parse_dl.pl:1462-1516). A dot postfix attaches as a loop over factor output,
same pattern as `add_expr_rest` (:1468-1477) or `brace_value_type`
(:1640-1643). Calibrated cost: 10 to 15 lines.

R2.6  Term-door constraint (the one that kills naive designs): the compiler's
other entry door consults real Prolog, so every surface form needs a
Prolog-readable term shape. Precedents: `[... P]` is a Prolog syntax error, so
the term form is `spread/1` (parse_dl.pl:1667-1671); `$name` works because
`$/1` is a standard SWI prefix operator (:1654-1662); `Tag{...}` and `_{...}`
read as SWI dicts and are REFUSED for that reason (:1539-1547). For dot: the
functor `'.'/2` is list cons in Prolog and cannot be reused, and SWI dict dot
syntax silently truncates chains on a space (R1.8). So the term form must be a
plain compound, e.g. `dot(Var, [file, rev])`, which reads at both doors with
zero reader work.

R2.7  Round-trip law: G1 is `parse_dl(print_dl(Term)) =@= Term` over 136
fixtures (`v6/prolog/compile/SYNTAX.md:359`). Erasing a spelling at parse time
is a recorded defect class: ARCH.pl:836 F1 documents that combine/next were
desugared at parse time, print_dl erased the spelling, and the two doors held
different terms for one source; the fix was real oracle clauses, and the
correction "erasure corrected" is in that row. Consequence for dot: parse must
KEEP a dot marker term (R2.6), and expansion to joins must happen in a later
phase, not in the parser.

R2.8  Does the langium demotion matter here? Yes, and it helps. One canonical
parser means one parse change, not two (SYNTAX.md:10-16; ARCH.pl:663). The two
real programs (ghcacher.dl6, conformance.dl6) still enter through the langium
bridge, which rejects unknown constructs by having no production for them
(dl.langium:5-7 comment); dot landing DCG-first cannot break them, and the
langium grammar gains the spelling only if the bridge programs ever need it.

## R3. Name resolution today

R3.1  Rel names: flat atoms keyed Name/Arity. `rel_ref/2` (consulted from
conformance/body.pl, used at `v6/prolog/analyze.pl:63-64,133`). Undeclared
unheaded refs are refused (defect-wave D4, `v6/prolog/ARCH.pl:818`). One flat
global namespace (R1.11). Compiler-internal families carry `__` prefixes:
`__delta_`, `__frontier_` (`v6/prolog/lower.pl:158-161`), `__ref_<type>`
(lower.pl:772-776), `__dict_<type>` (lower.pl:935-942); users are kept out of
that prefix by the suffix/prefix guard convention recorded at
`plans/2026-07-02-source-rule-body-join-desugar.md:149`.

R3.2  Columns: declared with name and type in the `rel` decl; the parser
records column order per rel (parse_dl.pl:588, `record_column_order`). Named
call-site args (`col: term`) resolve to positional AT PARSE
(parse_dl.pl:1098-1112), mixed named/positional allowed
(parse_dl.pl:1091-1096). The term door is positional throughout; there is no
runtime column-name binding to extend. A qualified name would therefore resolve
in exactly one place per pass: parse (named args, already done), analyze (rel
refs by Name/Arity), lower (decode -> dictionary join selection by declared
column type), oracle (body.pl json_decode, `v6/prolog/conformance/body.pl:144-177`).

R3.3  What the type plane knows: one relation model, one checker
(`v6/prolog/0_type_plane.pl:1-27`). A column typed by another rel means
`target(__id, key..., fields...)` with the parent holding the integer endpoint
(:4-10). `type_decl/2` is a compiler IR record: "It does not create a language
type, hidden dictionary, second table family, or second checker" (:14-21).
Storage kinds dispatch through `column_storage/3` (:71-76 region): int, text,
json (own storage kind), rel name (ref). Phase-5 float/REAL/avg landed
(`v6/prolog/ARCH.pl:693` rulings, `:844` float_avg_arc done). Known residue:
struct-typed arrival columns are ungradeable through the text door
(`v6/prolog/ARCH.pl:798`, golden_flex_residue).

R3.4  decode is the shipped navigation spelling. `decode(Where, {file: File})`
over a struct-typed column lowers to an ordinary positive atom over the type's
dictionary rel (`v6/prolog/lower.pl:888-942`, SLOT-DECODE-SURFACE ruling:
decode STAYS on the surface as sugar and lowers to a join, :890-894).
Relation-valued TERMS (`span(file(repo(N), fpath(P)), S, E)`) lower to one
`__ref_<type>` atom per level, memoized by term identity, children post-order
(lower.pl:944-997). Both directions are one indexed join per hop (:982-985).
decode is refused in edge bodies today (:931-933), and a missing struct type
reaches a named refusal (:1221-1226).

R3.5  The point the whole plan turns on: everything R1.3 said a dot must lower
to is SHIPPED. Dictionary rels, `__ref` views, field-kind dispatch by declared
column type, json's own storage arm, named refusals for bad receivers. What is
missing is only the spelling: the parser has no dot production and the printer
no dot clause.

## R4. Lowering doors

R4.1  SQL door: every emitted identifier is double-quoted
(`v6/prolog/lower.pl:195`, `v6/prolog/emit_ts.pl:978`). A `.` inside a quoted
SQLite identifier is legal and names ONE identifier, so even a dotted rel name
would survive the SQL door textually. Rel names pass raw through
`table_name/2` (lower.pl:156); derived table names are `format/3`-built
(lower.pl:158-161).

R4.2  TS door: `ref_name/2` passes the rel name raw into JS positions
(`v6/prolog/emit_ts.pl:80`); `pascal_case` splits on underscore only
(:91-97). A `.` in a rel name would leak into JS identifier positions and
break the emit. Consequence: any design that creates dotted REL NAMES (only
design B) owes a dot-to-underscore mapping at the TS door; designs that keep
dots inside expressions (A) or off the surface (C) owe nothing here.

R4.3  rx door, the snippet law: every .dl construct shown must carry its
pure-rxjs lowering (`plans/2026-07-28-consumption-arms-lab-header.md:52`).
decode's rx lowering is already written: combineLatest of source stream and
dictionary stream, one keyed `Map.get` per row, distinctUntilChanged on row
sets (`v6/prolog/lower.pl:904-915`). A dot construct that is decode sugar
inherits this lowering verbatim; a dot construct whose rx lowering could not
be written would be a design defect, and none was found in any candidate.

R4.4  COUNT-test law: formerly-quadratic paths get a statement-count or
EXPLAIN assertion, never end-state equality alone
(`plans/2026-07-29-finish-the-job-epic.md:1190`; restated
`plans/2026-07-30-ts-lowering-review.md:189`). The per-arm row-source
assertion standard for join chains already exists (defect-wave D5,
`v6/prolog/ARCH.pl:818`, receipts in v6/tsv2/tests/relationDepth.test.ts).
Applies to dot: a chain of N segments lowers to N dictionary joins, so the
fixtures must assert statement counts flat in chain length and row sources by
name per delta arm.

## R5. The typespec oddity, precisely

Sources fetched 2026-08-03:
- Namespaces: https://typespec.io/docs/language-basics/namespaces/
- Models: https://typespec.io/docs/language-basics/models/
- Emitter conflict: https://github.com/microsoft/typespec/issues/5532

The facts:
1. Namespace members spell `A.B`: `Foo.Bar.Baz.SampleModel` (namespaces doc).
   `namespace Foo.Bar.Baz { ... }` is itself dotted declaration syntax.
2. Model properties, the thing that looks MOST like a member, are NOT
   reachable with `.`. Property meta types are referenced with `::`:
   `Pet.name::type` (models doc, "Model property meta types can be referenced
   using ::").
3. `using` bindings are not re-exported: `Two.A` is invalid where `Two.B` is
   valid, same doc page, so the dotted path's meaning depends on where the
   binding was introduced, not on the spelling.
4. When a namespace and a model collide on a name segment, the emitters mangle
   (`_` prepend) because `Foo.Bar.Baz` can be a namespace path and a model
   name at once (issue 5532).

The oddity, stated as a rule: in typespec, `A.B` resolution dispatches on the
DECLARATION KIND of A (namespace vs model vs enum vs union), one member space
per kind, and the kind is invisible at the use site. The spelling alone never
tells you which space you are in, and the most intuitive space (record
property) is the one the dot does not serve. This is the owner's named failure
mode, and it is the acceptance test applied to each candidate below: does
`a.b` resolve from what `a` IS at the use site (value plane), or from how `a`
was DECLARED (declaration-kind dispatch)?

## R6. Design space, costed (packet; does not pick)

### Design A: one symbol `.`, member access only, resolution by plane + type

Rule: `.` appears only in expression position, postfix on a factor. The left
operand is always a VARIABLE, because a bare identifier is always a variable
in dl6 (`v6/prolog/compile/SYNTAX.md:36` region, the central superseding
decision: atom literals are single-quoted, so no bare ident is ever a rel or
namespace name in expr position). There is no namespace reading of `a.b` at
all; the namespace question is answered separately (R7).

Inference sketch (prolog-style, not code):

    dot_meaning(Var, Field, Goal) :-
        column_type_of(Var, Type),               % from the enclosing atom + decls
        field_goal(Type, Var, Field, Goal).

    field_goal(RelName, Var, Field, DictAtom) :-
        type_def(RelName, Cols, _),              % struct/rel-typed column
        memberchk(Field, Cols),
        DictAtom =.. [DictRel, Var | Outs].      % one __dict_<type> atom, R3.4
    field_goal(json, Var, Field, JsonGoal) :-    % untyped json column
        JsonGoal = json_path(Var, Field).        % json1 arm, R3.3 storage kinds
    field_goal(Scalar, _, Field, refusal(dot_on_scalar(Scalar, Field))) :-
        scalar_storage_kind(Scalar).             % int/text/bool/float

Unbound receiver at the dot site: refusal `dot_source_unbound(Var)`, mirroring
the decode pass's missing-type refusal path (lower.pl:1221-1226). Chain
`a.b.c`: fold left, each hop re-enters `dot_meaning` with the previous hop's
output variable and its resolved type. Ambiguity does not compound because
every hop's type is declared.

Worked ambiguous example (the acceptance case):

    rel file(path: text).
    rel diag(at: file, message: text).
    hit(P) <- diag(At, _), P = At.path.        % fine: At is file-typed, one dict join

    rel scan_out(path: text).
    hit2(P) <- scan_out(file), P = file.path.  % `file` binds as a VARIABLE (text),
                                               % and `file` is also a rel name.

Resolution: `file.path` is var-projection on a text variable; text has no
fields, so the refusal is `dot_on_scalar(text, path)`. The rel reading
(project column `path` of rel `file` as a fan-out) is REFUSED BY THE PLANE
RULE, not by disambiguation cleverness: a bare ident in expr position is a
variable, full stop. A rel-projection surface (`rel.col` as a derived rel) is
a different construct in atom position, out of scope here (blocking question
2). This is exactly where typespec would silently pick a declaration kind;
dl6 refuses with a named message.

Parse cost: factor postfix loop, 10 to 15 lines (R2.5 calibration), term form
`dot(Var, [f1, f2])` (R2.6, spread/1 precedent), one registry `expression/5`
family row (registry.pl:232-248 pattern), one print_dl clause with precedence
from the registry (print_dl.pl:501-503). Total parser+printer+registry: about
25 lines.

Resolution/lowering: one expansion phase module, `0_dot_expand.pl`, rewriting
`dot(Var, Chain)` goals into decode dictionary atoms BEFORE both doors
(precedents: `v6/prolog/0_coalesce_expand.pl`, `0_match_expand.pl`,
`0_seq_expand.pl`; the rewrite-of-the-rule discipline is lower.pl:917-922 so
every level-statement family sees the join). Edge bodies inherit the decode
refusal (lower.pl:931-933). Oracle cost: zero if expansion runs ahead of the
oracle too, same as coalesce (registry.pl comment: the one module both doors
consult).

Doors: SQL inherits the dict join (R3.4). TS inherits the dict join. rx
inherits the written snippet (R4.3). Law satisfied.

Migration: zero. decode stays on the surface (SLOT-DECODE-SURFACE,
lower.pl:890-894); the 72 decode call sites across 42 fixture files (grep
count, section R3.4 fixtures plus v6/dl/fixtures) keep working; respelling is
optional and cosmetic.

Typespec verdict: AVOIDS the oddity. Resolution runs on the value plane from
the variable's declared column type; there is no declaration-kind dispatch
because there is only one member space (columns of the receiver's type). The
boundary case refuses with a named message instead of silently switching
spaces.

### Design B: `::` for namespaces + `.` for members

Rule: `ns::rel(args)` names a rel in a namespace; `x.f` stays design A's
member access. Two symbols, zero overlap, no ambiguity by construction.

Parse cost: `::` is unclaimed and already lexer-distinguished
(parse_dl.pl:1083 excludes it from named-arg `:`). New qualified-name
production on every rel-name position (head, body atom, query, probe):
about 10 lines parser, term form `ns(Ns, Name)` compound (Prolog-readable;
`:` 600 xfy would also read but spends the named-arg colon, R2.4), printer
clause, about 30 lines total. Then the real cost: the rel table's keys become
(Ns, Name) pairs, touching rel_ref/2 (R3.1), analyze refs, lower table_name
(R4.1), emit ref_name and pascal_case, which breaks on `:` in JS identifier
positions and needs a mapping (R4.2). SQL door is free (quoted idents, R4.1).

Resolution rules: two-part lookup, no inference needed; that is the design's
entire point. Vocabulary cost: dl6 surface would carry `:` (named arg, type
marker), `::` (namespace), `.` (member): three colon-family spellings. Next to
prolog's own `Module:Goal` convention this reads fine; next to typespec it
INVERTS typespec's split (typespec: `.` for namespaces, `::` for property
meta), which is worth one honest line: the two-symbol world is the world
typespec lives in, and the owner named that world as the failure mode.

Migration: zero today (no fixture uses `::`, R2.4; there is no `use`, so no
program has anything to qualify, R1.12).

Typespec verdict: avoids ambiguity by construction, reproduces the two-sigil
ceremony. Passes the letter of the acceptance test (use site says which space)
and fails its spirit (two spellings for "containment" is the oddity's other
half).

### Design C: no surface dots (today's answer)

Namespacing stays in strings (SCIP symbol pkg field, R1.10; dotted sym data,
R1.11) and in reserved prefixes (`__` families, R3.1). Navigation stays
`decode(X, {f: Y})` and relation-term destructuring (R3.4).

Cost: zero, by definition. Ergonomic loss, stated honestly with receipts: the
chained-coordinate read costs one decode atom and one temp variable per hop;
the shipped fixture `relation_depth2_chained_decode.dl6` writes
`decode(File, {at: At}), decode(At, {name: PathName4})` for what A writes as
`File.at.name`; flagship-flow.dl6 carries six decode sites in one rule family
(v6/dl/fixtures/flagship-flow.dl6:75-105); and R1.3's reading
`call.loc.file.rev.repo` (:61-79) stays unspellable.

Typespec verdict: avoids the oddity by absence. Also avoids the feature.

## R7. The two explicit answers

### (4) Do we need actual namespace `::` symbols?

No, not from anything the code needs today. Every candidate consumer was
checked:

- Compiler-internal rel families (`__dict_`, `__ref_`, `__delta_`,
  `__frontier_`) are already namespaced by the reserved `__` prefix
  (R3.1, lower.pl:158-161, :772-776), and the guard that keeps users out of
  that prefix is on record (plans/2026-07-02-source-rule-body-join-desugar.md:149).
  A `::` symbol adds nothing these families lack.
- Multi-program sharing does not exist: there is no `use` or import in the
  surface (surface-audit :762 lists module import as absent, "add", 16 corpus
  files), and the share-across-namespaces question is explicitly OPEN in the
  isomorphism spec (R1.12). There is nothing to qualify yet, so the qualifier
  cannot pay for itself.
- Cross-language symbol identity lives in SCIP symbol strings (R1.10,
  PLAN2.md:99): data-plane namespacing, resolved by string grammar, not by
  surface syntax.

The need arrives exactly when module import lands. At that point design B's
cost table (R6.B) is the price sheet, and the task row `namespace_revisit`
below is armed. Until then, `::` is vocabulary with no consumer.

### (5) Do we need interfaces/traits for any specific reason?

No first-class construct. The codebase already fakes interfaces in four
places; examining each says the fakes that work work because they are rels or
facts, and the one that leaks is not a dl6 problem:

1. Host executor contracts: `host_executor_contract/2`,
   `v6/prolog/compile/registry.pl:334-338`, exact positional column contracts
   as compiler facts, checked at compile time. This IS an interface, encoded
   as data, and it works.
2. The `sh` raw-template door: `sh_decl_stmt` (parse_dl.pl:543 dispatch) and
   the TEMPLATE terminal (dl.langium:187): raw text the compiler carries and
   never interprets, checked only at the host boundary. (This is the real
   seam the brief's "lang_ext printer-ignore seam" points at; deviation 3.)
   The contract is the decl's column list, again rel-shaped data.
3. The TS `I`-interface law, `plans/2026-07-30-ts-lowering-review.md:233`:
   important functions bind to an `I`-prefixed header interface because
   TypeScript cannot conformance-check a bare function. It LEAKS: eight free
   functions carry real contracts with zero header entries (:236-253), six
   types are declared 138 times (:258-268), and the emitted `IBootStatement`
   already drifted from the header (:275-278). The fix for this is not a dl6
   construct; it is the type-ir lane's facts-plus-emitter-plus-staleness-gate
   (PLAN2.md:19-26 steps a/b, :79 arrows step e), which turns the TS contracts
   into the same kind of checked facts as fake 1.
4. The one-rel-one-rule-kind law: SUPERSEDED, not a live fake
   (chat_log/20260727.1:7, merge f91b9dbb: mixed heads sound under
   count-IVM). Nothing to cover.
5. The Extractor/Effect trait sketch (R1.3,
   plans/2026-07-21-v6-runtime-decomposition.md:116-130): in the shipped
   prolog compiler this became registry facts (`host_executor/2`,
   `host_execution/3`, `bind_executor/2`, exports at registry.pl:18-24).
   The trait became a table, and the table is checked.

Verdict: a rel declaration plus registry contract facts is already the
smallest construct that covers every found case, and it exists. The one
leaking fake (TS side) needs the staleness gate the sibling lane is already
building, not new dl6 syntax. Said plainly: no interfaces, no traits; the
language's interface construct is the `rel` decl.

## R8. ARCH-style task rows (shape task(Name, Status, Needs); all unbuilt)

```
task(dot_surface_ruling,  unbuilt, []).                       % owner picks A, B, or C from this packet
task(dot_parse_print,     unbuilt, [dot_surface_ruling]).     % factor postfix chain + dot(Var, Chain) term form + registry expression row + print_dl clause; G1 round-trip 136/136 must hold (R2.7)
task(dot_expand_phase,    unbuilt, [dot_parse_print]).        % 0_dot_expand.pl: dot goals -> decode dictionary atoms ahead of both doors; rewrite-of-rule discipline (lower.pl:917-922)
task(dot_refusals,        unbuilt, [dot_expand_phase]).       % dot_on_scalar, dot_unknown_field, dot_source_unbound; edge-body refusal inherited from decode (lower.pl:931-933)
task(dot_count_tests,     unbuilt, [dot_expand_phase]).       % statement count flat in chain length + per-arm row sources by name (R4.4)
task(dot_rx_snippet,      unbuilt, [dot_expand_phase]).       % rx lowering written in the phase module header per the snippet law (R4.3)
task(namespace_revisit,   unbuilt, []).                       % armed only when module import lands; design B cost table is R6.B
```

## R9. Blocking questions, in the order they block

1. A, B, or C? (Answerable as one letter; everything else hangs off it.)
2. If A: is `rel.col` projection (dot in atom position over a rel name) in
   scope now, or refused until module import lands?
3. If A: does dot over untyped `json` columns (the json1 arm) ship in the
   first cut, or struct/rel-typed columns only?
4. The 72 decode call sites: respell single-field decodes to dot, leave them,
   or lint toward dot? (Cosmetic; blocks only doc wording.)
5. Does the langium spelling reference get the dot row now, or only when the
   two bridge programs need it? (Doc timing only, per R2.8.)
