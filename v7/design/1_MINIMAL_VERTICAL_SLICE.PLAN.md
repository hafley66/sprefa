# DL7 minimal programmable kernel

Date: 2026-08-28

Status: blocked on the declared-node module identity ruling in
[Blocking identity ruling](#blocking-identity-ruling).

## Context

Build the smallest `.dl7` kernel that proves one relational evaluator can run
both compiler rules and runtime rules.

```text
.dl7 terms
    -> bind and application lowering
    -> one normalized fact/rule IR
    -> one SWI-Prolog evaluator
         | compiler seeds -> type graph
         | runtime seeds  -> runtime rows
    -> later backend adapter
    -> existing v6/sprefa-engine-rs
```

The Rust engine remains in place. V7 adds no Rust runtime and copies no engine
source. ProgramJson adaptation follows the kernel proof.

Userland helper types are proof goals in dependency order:

```text
Partial
    -> Pick
    -> Exclude
```

They must be authored as `.dl7` rules. Their names and behavior cannot appear
inside the reader, graph kernel, or evaluator.

## Decisions

The first executable slice uses these contracts:

1. Reader output is a node-identified prefix tree. Reader node IDs and logic
   variable IDs last one compilation.
2. Semantic identities use `named/3`, `primitive/1`, and `application/2`.
   The first argument of `named/3` remains blocked between the two forms in
   the identity ruling below.
3. The public graph edge is `':'(Owner, Name, Target, Index)` with functional
   keys `(Owner, Name)` and `(Owner, Index)`.
4. A callable has zero or more input edges followed by exactly one `return`
   edge. Its relation row contains the output in that final tuple column.
5. Value calls and type calls use one saturation operation. The declared
   return target decides whether the result is a value or a semantic type ID.
6. `intern/3` is the evaluator's sole operation for adding a semantic identity
   outside the authored declaration and primitive seed domains.
7. Construction requests are ordinary ground closure rows. The compile driver
   owns the bounded outer request loop.
8. Compiler and runtime rule sets use the same normalized rule IR and the same
   `evaluate/4` predicate body.

Rejected alternatives for this slice: surrogate semantic IDs, content hashes
as semantic IDs, a second type-application AST, a compiler-specific evaluator,
synthetic public edge IDs, and kernel clauses for `Partial`.

## Receipts from saved design discussions

The plan incorporates Boop favorites 26 through 37, especially:

- favorite 37: reader, binder resolution, core lowering, and execution are
  distinct compiler phases;
- favorite 36: `?x` is a variable, `x` is a reference, `'x` is symbol data;
- favorite 34: a type expression is an ordinary expression returning `type`;
- favorite 33: prefix lists expose one application tree;
- favorite 31: return values remain relation tuple columns;
- favorite 29: value and type calls share application lowering;
- favorite 28: canonical specialization identity is kernel interning;
- favorite 27: compile time and runtime use one relational fixpoint algorithm.

## Overnight code ceiling

- Four production modules maximum: reader, graph/lowering, evaluator, driver.
- One standard-library `.dl7` file.
- One fixture.
- One end-to-end SWI test file containing one test.
- One optional engine smoke command after the kernel is green.
- No V6 full suite, generated corpus, TS V2 command, benchmark, or formatter.
- No imports, effects, ticks, retention, history, macros, body reopening,
  recursive types, partial application, higher-kinded types, or emitter rewrite.

An out-of-scope dependency stops its card and produces a Boop hail with the
missing signature or ruling.

The complete production-file budget is:

```text
v7/0_READER/0_reader.pl       read_dl7/5
v7/1_KERNEL/0_kernel.pl       lower_dl7/7 and graph/application lowering
v7/2_EVALUATOR/0_evaluator.pl evaluate/4 and intern/3
v7/3_COMPILE/0_compile.pl     compile_dl7/4 and the request loop
```

The only executable support files in the slice are:

```text
v7/4_PRELUDE/0_types.dl7
v7/5_TEST/0_kernel.dl7
v7/5_TEST/0_kernel.test.pl
```

`0_types.dl7` contains `Option` and `Partial`. `0_kernel.dl7` is the one
fixture. `0_kernel.test.pl` contains the one test. Adding another production
module or test file requires a later plan.

## Syntax kernel

The reader accepts one prefix tree:

```ebnf
term := atom | variable | literal | "(" term* ")"
```

The lexical contract is bounded to:

```text
name          [A-Za-z_][A-Za-z0-9_-]* or one of :, *, +, ->, <-
?name         logic variable
'name         symbol literal
"text"        string literal with \n, \t, \r, \\, and \" escapes
-12           decimal integer
1.25          finite decimal float
# text         comment through the next newline
(...)         form
```

Whitespace separates adjacent terms. `(` and `)` delimit forms. A quote
immediately followed by an identifier produces symbol data and does not enter
name resolution. Booleans, byte literals, dotted paths, brackets, braces,
reader macros, and quote forms over lists remain outside this slice.

Canonical reader terms are:

```prolog
node(+NodeId, atom(+Name)).
node(+NodeId, variable(+VariableId, +Name)).
node(+NodeId, literal(+Value)).
node(+NodeId, form(+Nodes)).

source(+NodeId, +Path, +StartOffset, +EndOffset,
       +StartLine, +StartColumn, +EndLine, +EndColumn).
```

`NodeId = reader_node(Path, PreorderIndex)`, where `PreorderIndex` is zero
based over every term in one `read_dl7/5` call. For a named variable in a
top-level form, `VariableId = variable(TopNodeId, Name)`. Repeated `?Name`
terms in that form share the ID. The same spelling in another top-level form
gets another ID. `?_` receives `variable(NodeId, '_')`, so each occurrence is
fresh. `Forms` contains the top-level `node/2` terms in source order and
`SourceMap` is the complete source-row list sorted by `NodeId` preorder.
Offsets are zero-based Unicode code-point offsets with an exclusive end. Lines
and columns are one-based, and each end line and column names the position
immediately after the term. `Path` is the exact path value supplied to
`read_dl7/5` and has source-location lifetime only.

Kernel spellings:

```lisp
(: Name Target)                 named edge in the current owner
(* Binding...)                  product
(+ Binding...)                  sum
(-> Inputs Output)              callable signature
(<- Head Body...)               rule
(F Argument...)                 application in expression position
?x                              logic variable
x                               reference
'x                              symbol data
```

The compact spelling `Name: *(...)` is outside this slice. Every accepted
source construct reads through the one prefix-tree path.

## Type and edge graph

All type-like values occupy one semantic identity domain. Product, sum,
primitive, namespace, and specialization are ordinary classifications of
those identities.

The public edge fact follows the settled order:

```prolog
':'(+Owner, +Name, +Target, +Index).
```

Its functional keys are `(Owner, Name)` and `(Owner, Index)`. Indices are
zero-based and contiguous within one owner. Duplicate names, duplicate
indices, and gaps are lowering diagnostics. The complete ground colon term is
the canonical edge reference when an annotation or compiler rule refers to
the edge row. No synthetic public edge ID and no `member` vocabulary enter
V7.

The semantic identity constructors retained from the audited donor are:

```prolog
primitive(+Name).
named(+ModuleIdentity, +Kind, +Name).
application(+Constructor, +Arguments).
```

Declared relation, product, sum, namespace, and callable nodes use
`named(ModuleIdentity, relation, Name)` and separate `class(Node, Kind)` rows.
This slice does not mint a second shape identity for a declared node. A generic
specialization uses `application(Constructor, Arguments)`. Generated artifact
names do not replace the complete semantic terms.

The file module path supplies the implicit outer owner:

```lisp
(: User
  (*
    (: id int)
    (: name text)))
```

```prolog
ModuleOwner = module(ModuleIdentity),
UserType = named(ModuleIdentity, relation, 'User').

':'(ModuleOwner, 'User', UserType, 0).
class(UserType, product).
':'(UserType, id, primitive(int), 0).
':'(UserType, name, primitive(text), 1).
```

## Blocking identity ruling

The stop condition fired because the donor and V7 reconciliation leave two
coherent declared-node identities. The parent directed this card to select
neither form.

### Option A: donor module hash

```prolog
ModuleIdentity = ModuleHash,
ModuleOwner = module(ModuleHash),
UserType = named(ModuleHash, relation, 'User').
```

Required inputs and construction:

```text
EntryBaseDirectory
AbsoluteModulePath
    -> path relative to EntryBaseDirectory
    -> remove the filename extension
    -> join directory and stem with "/"
    -> SHA-256 over that text
    -> first 8 digest bytes as 16 lowercase hexadecimal characters
    -> ModuleHash
```

This is the current DL6 `use_resolve:module_hash/3` contract. Equal relative
module paths produce equal IDs across checkout locations. A relative module
rename changes every declared ID in that module. The 64-bit truncated digest
permits a collision to alias two modules unless V7 adds an explicit
`(ModuleHash, ModuleStem)` collision check. Portability requires identical
relative-path, separator, Unicode, and extension normalization in every
compiler implementation.

### Option B: structural module path

```prolog
ModulePath = [Segment0, Segment1, ...],
ModuleIdentity = module(ModulePath),
ModuleOwner = module(ModulePath),
UserType = named(module(ModulePath), relation, 'User').
```

Required inputs and construction:

```text
EntryBaseDirectory
AbsoluteModulePath
    -> path relative to EntryBaseDirectory
    -> remove the filename extension
    -> split into a nonempty ordered atom segment list
    -> ModulePath
```

Structural equality supplies the collision contract, so distinct normalized
segment lists cannot alias. A relative module rename changes every declared ID
in that module. Portability still requires identical relative-path, separator,
Unicode, dot-segment, and extension normalization. The full path remains
present in semantic rows and serialized compiler artifacts unless a later
adapter replaces it only at the artifact boundary.

### Affected contracts

The ruling changes these signatures and outputs:

- `compile_dl7/4` must define the entry base directory and derive either
  `ModuleHash` or `ModulePath` for the source and prelude modules.
- `lower_dl7/7` currently accepts `ModulePath`; Option A requires that argument
  to carry a resolved module hash or requires a separate module-identity input.
- Every `named/3` declaration ID, module-owner `':'/4` row, callable constructor
  ID, `application/2` specialization ID, `construction_request/3` row, and
  `CompilerRows` result contains the eventual module identity transitively.
- `RuntimeProgram` contains relation metadata projected from those identities,
  so compile-twice equality and later ProgramJson adaptation grade the choice.

`read_dl7/5` and `evaluate/4` do not derive module identity. Their signatures
remain unchanged. Implementation of all four production modules waits for this
ruling because the reader oracle snapshots semantic output through
`compile_dl7/4`.

## Bind and scope

A scope is a node whose outgoing colon edges form its symbol table. Name
resolution reads those edges. A product is also a node whose outgoing colon
edges form its ordered fields. The same relation supports both uses.

```prolog
resolve(+Owner, +Name, -Target).

% Read ':'(Owner, Name, Target, Index).
% Search Owner, then each scope_parent/2 row in order.
% Return one target or one deterministic diagnostic.
```

`scope_parent(Child, Parent)` is keyed by `Child`. Top-level declarations are
reserved as colon edges before any target expression is resolved, so a group
can refer to its own declared names. Nested products create owners but do not
create implicit parent storage columns. Missing and ambiguous resolution never
mint a node.

The first slice has a file module owner and nested product owners. Imports,
reopening, and lexical closures remain outside the slice.

## Relation application

A declared callable is a product containing input edges and one `return` edge.
The return remains a column in the underlying relation tuple.

```lisp
(: Identity
  (->
    (* (: value any))
    any))
```

The lowered signature rows are:

```prolog
Callable = named(ModuleIdentity, relation, 'Identity').

class(Callable, callable).
':'(Callable, value, primitive(any), 0).
':'(Callable, return, primitive(any), 1).
callable(Callable, 'Identity'/2, 1).
```

`callable(Callable, RelationRef, InputCount)` has functional keys `Callable`
and `RelationRef`. The relation arity is `InputCount + 1`. Input column indices
are `0` through `InputCount - 1`; the one output column is the `return` edge at
`InputCount`. Several outputs and non-final outputs remain outside this slice.

Application in expression position:

```lisp
(Identity 1)
```

lowers to the saturated relational goal:

```prolog
'Identity'(1, Result).
```

The fresh result variable is `lower_var(CallNodeId, return)`. The surrounding
form receives that variable. A relation atom in a rule head or a top-level
relational body goal supplies all declared tuple columns. An application nested
in an expression supplies the input columns and lowering appends the output.
After lowering there is no call or application wrapper in the rule IR.

The same lowering applies when the return edge targets `primitive(type)`:

```lisp
(Option int)
```

```prolog
'Option'(primitive(int), ResultType).
```

Lowering a rule that defines a type-returning callable inserts the kernel goal
`intern(Constructor, Inputs, ResultType)` before user body goals. A
value-returning callable receives no inserted construction goal. Both call
sites still lower by appending the one declared output column.

Required application laws:

- the callable must be declared;
- one output column is supported in this slice;
- zero rows means no result;
- one row means one result;
- several distinct rows violate deterministic expression application;
- unsaturated calls are refused;
- ordinary body goals remain relational and may produce several bindings.

The normalized evaluator input is one ordered list containing:

```prolog
relation(+Name/Arity, +KeySets).
rule(+HeadAtom, +Goals).

Goals := [PositiveAtom | not(PositiveAtom) | intern(Constructor, Arguments, Result)]
```

`HeadAtom` and positive atoms are ordinary Prolog compounds such as
`'Identity'(Value, Result)`. Each member of `KeySets` is one list of zero-based
tuple positions. `[]` means no functional key beyond complete-row set identity.
For example, `relation(':'/4, [[0, 1], [0, 3]])` declares both public edge
keys. Facts enter through `Seeds`, not as bodiless rules. Rule order is retained
for diagnostics; relation rows are sets and closure output uses standard term
order.

## One evaluator

```prolog
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).

% Split relation/2 declarations from rule/2 clauses.
% Validate declared arities, seed groundness, authored-order rule safety,
% stratified negation, and ground constructor inputs to intern/3.
% Allocate one EvalId and install copied rules and ground seeds under EvalId.
% Compute dependency strata from positive and negative relation reads.
% Close each positive recursive stratum with the one tabled proof predicate.
% Let not(Atom) read only the completed rows of lower strata.
% Let intern(Constructor, Arguments, Result) return the canonical structural ID
% and add one ground construction_request/3 row to the active row set.
% Sort and deduplicate all ground rows, then validate declared functional keys.
% Copy Closure and ordered Diagnostics out of the EvalId namespace.
% On success, diagnostic return, or exception, abolish EvalId tables and
% retract every EvalId rule, seed, lower-row, and request fact.
```

The evaluator contains no compile-time or runtime branch.

```prolog
evaluate(CompilerRules, TypeSeeds, TypeClosure, TypeDiagnostics).
evaluate(RuntimeRules, RuntimeSeeds, RuntimeClosure, RuntimeDiagnostics).
```

Phase selection, effects, persistence, and backend emission live outside the
evaluator.

The evaluator recognizes one constructive goal, `intern/3`. All other
positive goals read declared relations. Aggregates, scalar host calls, effects,
ticks, and arbitrary Prolog calls remain outside the first evaluator.

Rule safety is authored-order safety:

- every head variable is bound by a preceding positive relation goal or by the
  `Result` position of a preceding ground `intern/3` goal;
- every variable read by `not/1` is bound before that goal;
- `Constructor` and every member of `Arguments` are ground when `intern/3`
  executes;
- a rule dependency path from its head back through one of its own
  `intern/3` constructor inputs is rejected as recursive construction.

## Canonical construction

Plain Datalog cannot create a new domain value. The kernel therefore exposes
one deterministic construction relation:

```prolog
intern(+Constructor, +Arguments, -Result).
```

Functional dependency:

```text
(Constructor, ordered Arguments) -> Result
```

The exact result representation is:

```prolog
intern(Constructor, Arguments, application(Constructor, Arguments)).
```

`Constructor` must be a ground semantic identity and `Arguments` must be a
proper ground list in declared input order. A supplied `Result` must unify with
that structural term. Interning has no counter, registry, mutable cache, hash,
or collision case. `application/2` is a semantic TypeId and is not a source or
normalized-call wrapper.

Authored `.dl7` uses ordinary calls such as `(Option int)` and `(Partial User)`.
The inserted `intern/3` goal is the sole evaluator primitive that can add a
semantic identity beyond authored `named/3` and `primitive/1` seeds. Reader
node IDs, rule variables, evaluator IDs, and artifact encodings are outside
the semantic domain.

Every successful ground intern goal adds this ordinary row to closure:

```prolog
construction_request(
    application(Constructor, Arguments),
    Constructor,
    Arguments
).
```

The request key is `(Constructor, Arguments)`. `Result` is functionally
determined. Lowering returns the same row shape in `Requests` for ground
type-returning applications visible before evaluation. Variable-bearing calls
remain `intern/3` goals and produce ground request rows only when evaluation
binds their inputs.

The driver may run an outer request loop:

```text
evaluate
    -> union lowered and closure construction_request/3 rows
    -> discard keys already present as specialization/3 plus argument/3 rows
    -> verify each Result with intern/3
    -> add specialization(Result, Constructor, Arity) and
       argument(Result, Index, Argument) seeds
    -> evaluate again
    -> stop when semantic rows and request keys both stop changing
```

The request loop has the donor cap of 16 evaluations and reports
`type_apply_round_limit_exhausted(16)` if stability is not reached. Request
rows and `intern/3` rows are compiler transport and are removed before the
runtime program is returned. Recursive construction and arbitrary fresh values
are refused.

## Userland proof goals

### Partial

`Partial` is an authored `.dl7` callable and two authored rules:

```lisp
(: Partial
  (->
    (* (: source type))
    type))

(<-
  (Partial ?Input ?Output)
  (specialization ?Output Partial 1)
  (argument ?Output 0 ?Input))

(<-
  (: ?Output ?Name ?OptionalType ?Index)
  (Partial ?Input ?Output)
  (: ?Input ?Name ?MemberType ?Index)
  (Option ?MemberType ?OptionalType))
```

The type-returning callable rule receives the ordinary lowering-time
`intern(Partial, [Input], Output)` construction goal. `specialization/3` is
keyed by `Output`; `(Constructor, Arity)` plus all contiguous `argument/3`
rows reconstruct the ordered interning key. `argument/3` is keyed by
`(Output, Index)`. `Partial` copies the public edge name and index exactly and
obtains each target through the ordinary type-valued `Option/2` relation.

### Pick

```prolog
':'(Output, Name, MemberType, OutputIndex) <-
    specialization(Output, pick, 2),
    argument(Output, 0, Input),
    argument(Output, 1, Names),
    ':'(Input, Name, MemberType, InputIndex),
    contains(Names, Name),
    selected_rank(Input, Names, InputIndex, OutputIndex).
```

### Exclude

```prolog
':'(Output, Name, MemberType, OutputIndex) <-
    specialization(Output, exclude, 2),
    argument(Output, 0, Input),
    argument(Output, 1, Names),
    ':'(Input, Name, MemberType, InputIndex),
    not contains(Names, Name),
    remaining_rank(Input, Names, InputIndex, OutputIndex).
```

`Names` is a closed symbol collection before Exclude's stratum runs. Pick and
Exclude preserve relative order and produce dense output indices.

The overnight kernel may stop after Partial. Pick and Exclude remain separate
open cards over the proven evaluator.

## Type signatures by module

```prolog
read_dl7(+Path, +Text, -Forms, -SourceMap, -Diagnostics).

% Initialize a call-local preorder counter and top-level variable table.
% Scan Text using the pinned comment, atom, variable, literal, and form syntax.
% Build node/2 terms and source/8 rows in one left-to-right pass.
% Reuse a VariableId for repeated ?Name inside one top-level form and mint a
% fresh VariableId for every ?_ occurrence.
% Return Forms and SourceMap in preorder plus source-ordered Diagnostics.
% Release scanner buffers and the variable table before returning.

lower_dl7(+ModulePath, +Forms, -Rules, -Seeds, -Requests, -Diagnostics).

% Accept the ruled module identity derived from ModulePath and seed primitives.
% Reserve every top-level name as a ':'/4 edge before resolving any body.
% Construct named/3 owners, class/2 rows, callable/3 rows, and ordered ':'/4
% edges while checking both edge functional keys.
% Resolve every atom reference by reading ':'/4 from the current owner and its
% scope_parent/2 chain; unresolved or ambiguous names become diagnostics.
% Lower facts to ground Seeds and rules to relation/2 plus rule/2 terms.
% For every nested expression call, append its callable's final return column.
% For every type-returning callable rule, insert intern/3 before user goals.
% Return ground visible construction_request/3 rows in Requests; leave
% variable-bearing requests represented by intern/3 goals in Rules.
% Return no partially lowered rule or seed when Diagnostics is nonempty.

evaluate(+Rules, +Seeds, -Closure, -Diagnostics).

% Split relation/2 declarations from rule/2 clauses and validate their shapes.
% Allocate one EvalId; copy and install rules and ground seeds under EvalId.
% Validate safety, stratification, construction recursion, and declared arity.
% Close positive recursive strata with one tabled proof predicate.
% Read not/1 only from completed lower strata and evaluate ground intern/3.
% Add canonical intern/3 and construction_request/3 rows to the same closure.
% Sort rows, check functional keys, and copy results out of the EvalId scope.
% Through setup_call_cleanup/3, abolish tables and retract all EvalId facts on
% success, diagnostic return, and exception.

compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics).

% Read Path and the one prelude file through read_dl7/5.
% Lower both modules through lower_dl7/7 and combine their diagnostics.
% Partition compiler and runtime rules from callable declarations: colon,
% class, callable, specialization, argument, construction_request, and a
% callable returning primitive(type) are compiler-owned; other callables are
% runtime-owned.
% Union compiler Seeds with lowered Requests and call evaluate/4.
% Drain unseen ground construction_request/3 rows, verify each with intern/3,
% add specialization/3 and argument/3 seeds, and evaluate again.
% Stop when compiler semantic rows and request keys are unchanged; report the
% donor exhaustion diagnostic after 16 evaluate/4 calls.
% Erase intern/3 and construction_request/3 transport rows.
% Return sorted semantic CompilerRows and
% runtime_program(RuntimeRules, RuntimeSeeds) in the same relation/rule IR.
% Release all read, lowering, and request-loop state before returning.
```

The reference proof then calls `evaluate/4` on the returned runtime program.
The later ProgramJson adapter consumes that same normalized runtime program.

## Instance timeline

One `compile_dl7(Path, CompilerRows, RuntimeProgram, Diagnostics)` call has this
timeline:

```text
1. Driver reads prelude text and Path text.
2. read_dl7/5 returns two node trees and two source maps.
3. lower_dl7/7 reserves names, resolves references, and returns normalized
   declarations, rules, seeds, and initial ground requests.
4. Driver partitions compiler-owned and runtime-owned declarations and rules.
5. Driver calls evaluate(CompilerRules, CompilerSeeds0, Closure0, Diags0).
6. The evaluator allocates EvalId0, closes all compiler strata, copies Closure0,
   and clears every EvalId0 table and temporary fact.
7. Driver reads construction_request/3 rows from initial requests plus Closure0,
   adds unseen specialization/3 and argument/3 seeds, and calls the same
   evaluate/4 predicate again.
8. Steps 6 and 7 repeat until the semantic row set and request-key set are
   unchanged. The stable rows become CompilerRows.
9. Driver removes compiler transport rows and returns
   runtime_program(RuntimeRules, RuntimeSeeds).
10. Driver releases reader trees, source maps, partition rows, and request-loop
    state. Semantic CompilerRows and RuntimeProgram cross the call boundary.
```

The one reference runtime call has this timeline:

```prolog
RuntimeProgram = runtime_program(RuntimeRules, RuntimeSeeds),
evaluate(RuntimeRules, RuntimeSeeds, RuntimeClosure, RuntimeDiagnostics).
```

```text
1. evaluate/4 performs the same declaration, safety, and stratum validation.
2. It allocates EvalIdR and installs copied runtime rules and runtime seeds.
3. The same tabled proof predicate closes positive groups; the same lower-row
   lookup implements not/1; the same intern/3 clause applies if a runtime rule
   contains one.
4. It copies the sorted runtime closure and diagnostics.
5. It abolishes EvalIdR tables and retracts EvalIdR temporary facts.
```

Compiler and runtime calls differ only in the supplied rule and seed lists and
in which rows their caller retains. `evaluate/4` receives no phase option and
tests no compiler or runtime tag.

## Storage and uniqueness

The kernel declares these evaluator key sets exactly:

| Relation | `KeySets` |
|---|---|
| `class/2` | `[[0, 1]]` |
| `':'/4` | `[[0, 1], [0, 3]]` |
| `scope_parent/2` | `[[0]]` |
| `callable/3` | `[[0], [1]]` |
| `intern/3` | `[[0, 1]]` |
| `construction_request/3` | `[[1, 2]]` |
| `specialization/3` | `[[0]]` |
| `argument/3` | `[[0, 1]]` |

Authored relations contribute their declared functional keys. A relation with
no declared functional key uses `KeySets = []`; complete-row set identity still
deduplicates exact rows.

| Row family | Exact key | Lifetime and cleanup |
|---|---|---|
| `node/2` reader term | `NodeId = reader_node(Path, PreorderIndex)` | One `compile_dl7/4` call; released after lowering. |
| `source/8` span | `NodeId` | One `compile_dl7/4` call; released after diagnostics are finalized. |
| logic variable | `VariableId = variable(TopNodeId, Name)`; `?_` uses its `NodeId` | One lowering call; replaced by normalized rule variables. |
| module owner | pending `ModuleIdentity` ruling | Semantic compile rows and returned compiler artifact. |
| `primitive/1` type | primitive `Name` | Seeded semantic domain; survives in compiler rows and runtime type metadata. |
| `named/3` node | `(ModuleIdentity, Kind, Name)` | Semantic compile rows and returned compiler artifact. |
| `class/2` row | `(Node, Kind)` | Semantic compile rows and returned compiler artifact. |
| `':'/4` edge | `(Owner, Name)` and `(Owner, Index)` | Semantic compile rows and returned compiler artifact. |
| `scope_parent/2` row | `Child` | Resolution and compiler rows; erased from a later runtime plan unless reflection retains it. |
| `callable/3` row | `Callable` and `RelationRef` | Compiler rows and normalized runtime metadata. |
| `relation/2` declaration | `Name/Arity` | One normalized compiler or runtime program. |
| `rule/2` clause | source `RuleNodeId` before lowering; complete normalized term after lowering | One normalized compiler or runtime program. Duplicate complete rules collapse. |
| seed row | declared relation key, otherwise complete ground term | Input lifetime of one evaluation; copied into the EvalId namespace. |
| closure row | declared relation key, otherwise complete ground term | Copied out of one evaluation; EvalId copy is retracted during cleanup. |
| `application/2` identity | `(Constructor, ordered Arguments)` | Semantic compile rows and returned compiler artifact. |
| `intern/3` row | `(Constructor, Arguments)`; `Result` is determined | One evaluation transport row; removed before `RuntimeProgram`. |
| `construction_request/3` row | `(Constructor, Arguments)`; `Result` is determined | One compile request loop; removed before `RuntimeProgram`. |
| `specialization/3` row | `Result`; the join with ordered `argument/3` rows also makes `(Constructor, Arguments)` functional | Semantic compile rows and returned compiler artifact. |
| `argument/3` row | `(Result, Index)` | Semantic compile rows and returned compiler artifact; indices are zero-based and contiguous. |
| evaluator table namespace | `EvalId` | One `evaluate/4` call; abolished through `setup_call_cleanup/3`. |
| evaluator temporary rule, seed, lower-row, request facts | `(EvalId, local ordinal or complete row)` | One `evaluate/4` call; all retracted through the same cleanup. |
| diagnostic row | `(Phase, Path, NodeId, Code)` | One compile or evaluation result; sorted by phase, path, span, and code. |
| runtime row | declared runtime relation key, otherwise complete ground term | One reference `evaluate/4` call. Durable state later belongs to `sprefa-engine-rs`. |

Canonical construction therefore has one identity sequence:

```text
named module constructor + ordered semantic arguments
    -> intern/3
    -> application(Constructor, Arguments)
    -> construction_request/3
    -> specialization/3 + contiguous argument/3 rows
    -> user .dl7 rules derive ':'/4 shape rows
```

## Donor policy

DL6 contributes audited predicates and laws:

- reader atoms, literals, escapes, comments, spans, and diagnostics;
- name collision and path resolution algorithms;
- semantic identity and canonical encoding laws;
- authored-order rule safety;
- strata, tabling, anti-join, aggregate separation, and functional-key checks;
- ProgramJson inventory and engine phase laws.

DL6 declaration terms, parser dispatch, `rel`, braces, dotted syntax, `plan/9`,
`lowered/8`, TS V2, and runtime source stay outside the kernel.

## Verification

One focused test compiles one fixture and snapshots:

```text
reader forms
compiler type closure
normalized runtime program
runtime reference closure
second compile equality
```

This is one test invocation and one exact expected term. Any additional test
requires a concrete defect that cannot be represented in the same snapshot.

The contract-writing card runs read-only checks only. It runs no SWI, V6,
Rust, generated-corpus, formatting, or lint suite. Implementation cards later
run the single `v7/5_TEST/0_kernel.test.pl` command after all four modules and
the fixture exist.

## Staffing and task DAG

The contract card runs with Sol in Boop worktree
`chore/dl7-kernel-contract` from base `a8bcda72c67d`. Later cards use their
model routes below in separate Boop worktrees after blocker commits land. The
contract card has a zero-suite budget. The implementation arc has one focused
SWI test invocation and one optional engine smoke command.

```text
kernel-contract [Sol, heavy]
        |
        v
contract-critique [Opus 5, review]
        |
        +-----------------------------+
        v                             v
prefix-reader [GLM53F xhigh, medium]  shared-evaluator [Sol, heavy]
        |                             |
        v                             |
symbol-graph [GLM53F xhigh, medium]   |
        |                             |
        +-------------+---------------+
                      v
              partial-goal [GLM53F xhigh, medium]
                      |
                      v
              kernel-oracle [Flash 4, small]
                      |
                      v
              luna-review [Luna, low]
                      |
          +-----------+------------+
          v                        v
pick-exclude [GLM53F, goal]   engine-seam [Terra, medium]
                                   |
                                   v
                           engine-smoke [Flash 4, small]
```

Each lane begins from main after all blocker commits land. No lane pushes.
Issue updates and commit receipts are written from main by the coordinator.
