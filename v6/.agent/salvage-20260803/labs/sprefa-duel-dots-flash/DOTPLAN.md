# PLAN: dotted access and namespacing in dl6 (recon + decision packet)

Merge check: `git merge --ff-only 2eceb836` in
`/Users/chrishafley/projects/sprefa-plan-dotaccess` returned `Already up to date`,
exit 0. HEAD is `2eceb836`. Proceed.

Every claim below cites a path and line I opened and read. Falsification probes I ran
are read-only (`swipl -q -l` against `parse_dl.pl` and `lower.pl`, scratch files outside
the worktree). No source file in this worktree was edited. No commits. No subagents.

Deviations recorded at the bottom (section 9), including two brief items that did not
survive verification.

---

## 1. Prior considerations, every one found, with what it concluded

### 1a. The v6 design plans

| where | what it says | status |
|---|---|---|
| `plans/2026-07-21-v6-runtime-decomposition.md:55-58` | section "Dot access <-> normalization". Thesis: the repo/rev/file/line chain is not a bespoke coordinate system, it is nested records plus dot access, and datalog is the normalization functor that flattens it | thesis, never banked as a ruling |
| same file `:64` | "`.` (dot access) COMPILES TO A JOIN through the normalization relation. `x.file.rev` is not stored nesting, it is two joins" | the join reading |
| same file `:81-90` | build-vs-buy prior-art table, "verified 2026-07-21": Souffle (records interned to one int, DESTRUCTURE only, no dot), DDlog (`x.field` + a full expression sublanguage), Datomic (navigate attribute = a join), Flix (records + row polymorphism), NF2 theory (nest/unnest lossless) | research done, cited |
| same file `:92-105` | DECISION: storage = Souffle record interning; SURFACE = ONE `Proj` (dot) operator, general, lowering dispatched by the field's KIND (functional column -> one join; record field -> record-table lookup; ADT branch -> match/guard; relation-valued -> a join that fans out). DDlog's expression sublanguage explicitly REJECTED | a decision inside a crate tree (`:130-160`) that v6 did not build. v6 is prolog compiler + TS runtime, not `sprefa-lang/::types`. Treat as superseded architecture, live argument |
| same file `:241` | `Proj(Box<Term>, SymId), // DOT ACCESS: term.field. ONE operator; lowering dispatched` | the Rust type sketch of the same |
| `v6/prolog/LANG.md:37` | "`x.field.sub` = nested pattern sugar" in the Operators bullet | the PATTERN reading, not the join reading |
| `v6/prolog/src/kernel.pl:38` | `sugar(dot_access, [ground_terms]). % x.f.g = nested pattern` | dot access is registered as sugar that grounds out on `ground_terms` alone |
| `plans/2026-07-27-surface-audit.md:738` | keep/kill table row: `` `x.field.sub` dot access | :36-37 | keep | `kernel.pl:38` has its sugar entry; cheapest construct in the spec `` | verdict KEEP, cost argued as lowest in the spec |
| `plans/2026-07-27-extraction-spellings.md:22` | dot access listed as T0, already in the AGGREGATE keep-list, alongside `Key(Type)`, `from world` / `->`, struct literals | assumed present, never built |
| `plans/2026-07-27-extraction-spellings.md:510-513` | ambiguity 8, `span.line` sugar. Argument AGAINST desugaring `at.line` to a view join: "it would be the only dot access that is a join rather than a projection, which breaks what dot access means everywhere else" | the PROJECTION reading |
| `plans/2026-07-30-lab-assimilation-sweep.md:51` | W4 dotted-path enumeration SUPERSEDED: "A dot path is a join chain over ref columns, which the types-as-rels verdict already listed as ordinary body atoms" | the JOIN reading again |
| `plans/2026-05-20-tags-via-tree-sitter.md:25` | "needed, dot-access handles read" | passing mention |

**The unresolved contradiction, and it is the whole design question.** Three documents
say a dot is a JOIN (`2026-07-21-v6-runtime-decomposition.md:64`,
`2026-07-30-lab-assimilation-sweep.md:51`, and the `Proj` lowering table at
`2026-07-21-v6-runtime-decomposition.md:95-105` whose first row is "functional column ->
one join"). Two say a dot is a PATTERN or a PROJECTION
(`v6/prolog/LANG.md:37` "nested pattern sugar", `v6/prolog/src/kernel.pl:38` same words,
`plans/2026-07-27-extraction-spellings.md:511` "breaks what dot access means everywhere
else"). Nothing reconciles them. Section 4 below shows the reconciliation is mechanical
once you look at what the ref column actually stores, but it has never been written down.

### 1b. The v4-era prior art in this repo

`chat_log/20260515.7.dot-access-audit.md` is a 396-line audit of dot access in the v4
template-hole grammar (`${X.field}`). Different construct (string interpolation holes,
not relation columns), same shape of problem, and it is the only place in this repo where
a dot grammar was ever written out:

- `:10` the semantic implementation was a flat-key shim: `${X.field}` resolved to
  `cursor.get("X.field")` which mangled to `X_FIELD` by
  `format!("{}_{}", stem, field.to_ascii_uppercase())`.
- `:93` "This is a flat-namespace CamelCase to SCREAMING_SNAKE translation, not
  structural navigation."
- `:14` a third inconsistent implementation existed: `FormatComponent`'s regex
  `\$\{([A-Za-z_][A-Za-z0-9_]*)\}` had no `.` in the character class, so `format`
  silently dropped every dotted hole.
- `:167-175` nested `${X.foo.bar}` was NOT supported: the parser consumed one `.IDENT`
  suffix, and the whole hole then fell through to a SubPipe that failed at walk time.
- `:191-200` the stated boundary: property access allowed (`a.b.c`), method calls
  (`X.length()`), indexing (`X[0]`), arithmetic and pipes all rejected, because
  "property access navigates structure without changing it".
- `:215-240` a four-case unambiguous grammar was written out.
- `:310-315` the trap: static `glob` REJECTED dot access (`ops.rs:710`), dynamic `glob`
  ACCEPTED it (`pipeline.rs:278`), so "a user adding a SubPipe to fix one thing
  accidentally unlocks dot-access on everything else".

Owner decisions on record, both from the v4 era:

- `chat_log/20260506.0.v4-editor-experience-and-language-finish.md:82` and `:121`:
  "User said skip dot-access for now." (twice, same session)
- `chat_log/20260507.2.v4-rule-engine-respec-and-memory-audit.md:58`, a canonical lock
  named `no-enum-flags-split-or-namespace`: "no enum-flag args; split into ops or
  namespace via dot-access (`write_cursor` over `write(:cursor)`)"; restated at `:119`
  as "no single-enum init args; multi op or dot-namespace".
  READING FLAG: the example given for "namespace via dot-access" is `write_cursor`,
  which is an UNDERSCORE name, not a dot. Either the lock means "namespace by name
  convention" and calls that dot-access loosely, or it means a real dot and the example
  contradicts it. I cannot resolve this from the log. Owner question B4 below.

### 1c. The adjacent open items the brief named

- **Key(Type) vs `->`**: `plans/2026-07-27-lab-consolidation.md:74-79` records three
  labs SPLIT (merge lab says Key wins; audit says merge them; astgrep lab says they are
  genuinely different because pattern types parameterized by a link-time grammar import
  do not fit a functional-dependency reading) and ends "Present both files' arguments; do
  not resolve by fiat." Ruled since: `v6/prolog/conformance/rulings.pl:50`
  `ruling(q8_key_vs_arrow, both_with_stated_law, user, 'AGGREGATE.md Q8 option (b)')`.
- **Q8 residual (left-of-arrow = demand key)**:
  `plans/2026-07-27-extraction-spellings.md:498-502`, ambiguity 6. The arrow is BUILT:
  `v6/prolog/compile/parse_dl.pl:886-912` parses
  `sh Name(inputs) -> (outputs) = \`template\`.`, live in the corpus at
  `v6/prolog/compile/dl_view/extraction_fork_span_line.dl6:1`. Relevance to dots: the
  arrow already splits a rel's columns into two named groups, which is the only
  grouping-inside-a-rel construct dl6 has. A dot on a rel value would be the second.
- **Type IR / SCIP package field as today's fake namespacing**:
  `/Users/chrishafley/projects/sprefa-plan-typeir/PLAN2.md:85-102`. That plan proposes
  synthetic SCIP symbol strings of the form
  `"typeir" " " "." " " <pkg> " " "dev" " " <shape>#`, and its table row for
  `package-name <pkg>` says the field "separates catalogs so a spine column never
  collides with a host signature id; mirrors SCIP's namespacing intent". So the type-IR
  plan's answer to namespacing is a STRING FIELD inside an opaque id, not a language
  construct. Same file `:87` records the SCIP descriptor grammar, where `#` is Type, `.`
  is Term, `(...)` is Method: SCIP itself uses THREE terminators to keep member kinds
  apart, which is evidence for option B in section 6.
  Confirmed against v5's real surface: `src/rels/scip.rs:61-88` declares
  `scip_def(symbol: Text, file: Path, repo: Text)` and eight siblings, every one with
  `group: "scip"` and an `scip_` name prefix. That pair (name prefix plus a `group`
  metadata field) is the v5 namespace.

---

## 2. The current surface: what a `.` would collide with

### 2a. Identifiers

`v6/prolog/compile/parse_dl.pl:402-418` is the whole identifier rule.

```
ident(Name, S0, S) :-                              % :408
    mark_furthest(S0),
    S0 = [C0 | Rest0],
    ( code_type(C0, alpha) ; C0 == 0'_ ), !,       % :411  start: letter or _
    ident_rest_codes(Rest0, RestCodes, S),
    atom_codes(Name, [C0 | RestCodes]).
ident_rest_codes([C | Cs], [C | More], S) :-       % :415
    ( code_type(C, alnum) ; C == 0'_ ), !,         % :416  rest: alnum or _
```

The header comment at `:403-406` states the governing law: "role (variable vs
relation-name vs label) is decided by grammar POSITION, never spelling". A qualified name
therefore cannot be distinguished from a plain one by case or shape; only by position.

The brief cited `:411-416`; the rule is `:408-418`. Minor, recorded.

### 2b. Every other lexical use of `.` in the DCG

| use | site | shape |
|---|---|---|
| statement terminator | `parse_dl.pl:573, 594, 756, 881, 907, 929, 987, 1003, 1054` (9 sites) | `lit_dcg(\`.\`, Sn, S)`, with NO requirement that whitespace follow |
| float fraction | `parse_dl.pl:461-463` `float_tail(Codes) --> \`.\`, digits_codes(Fraction), ...` | dot must be followed by at least one digit |
| array spread `...` | `parse_dl.pl:1676` `( lit_dcg(\`...\`, S2, S3) -> ...` | recognized ONLY immediately after `[` (`:1674`), so it cannot capture a general postfix chain |

The terminator is the hazard, not the float. `lit_dcg` at `:382-390` matches a bare `.`
with no lookahead. Prolog's own reader solves this by requiring the end token to be a dot
followed by whitespace, EOF, or a comment; dl6's DCG does not. Adding a dot chain to type
position (`typed_column_type/3` falls through to bare `ident` at `parse_dl.pl:641`) makes
`rel a(x: foo).rel b(y: int).` genuinely ambiguous: type `foo` then statement end, or
qualified type `foo.rel`. Fixing that is a change at all 9 terminator sites, not one new
production.

### 2c. What the parser does with a dot TODAY

Falsification probe, run read-only against `v6/prolog/compile/parse_dl.pl` with a scratch
loader (`swipl -q -l parse_dl.pl -l probe2.pl -g go -t halt`). Verbatim output:

```
dot_relname     THROW dl_parse_error(statement,position(1,6))     % rel a.b(x: int).
dot_body        THROW dl_parse_error(statement,position(3,23))    % ... Y := X.q.
dot_arg         THROW dl_parse_error(statement,position(3,4))     % f(X.q) <- s(X).
dot_col         THROW dl_parse_error(statement,position(1,8))     % rel a(x.y: int).
dot_type        THROW dl_parse_error(statement,position(1,11))    % rel a(x: b.c).
int_then_dot    OK prog([col_type(a/1,x,int)],[<-(a(1),true)])    % a(1).
underscore_collide OK prog([col_type('__delta_a'/1,x,int)],[])    % rel __delta_a(x: int).
```

Findings:
1. All five dotted spellings fail with the SAME generic `dl_parse_error(statement, ...)`.
   There is no named refusal for a dot anywhere. The position is the dot's own column.
2. `rel __delta_a(x: int).` parses clean. See 4b for why that matters.

### 2d. Round-trip printer

`v6/prolog/print_dl.pl:557-561`:

```
identifier_atom(Atom) :-
    atom(Atom),
    atom_codes(Atom, [First | Rest]),
    ( code_type(First, alpha) ; First == 0'_ ),
    forall(member(Code, Rest), ( code_type(Code, alnum) ; Code == 0'_ )).
```

An exact mirror of `parse_dl.pl:411/416`. Used at `:555` to decide bare-vs-quoted for a
json brace key. A dotted name reaching the printer through any bare-identifier path would
print unquoted and re-read as something else. The printer's own header at
`print_dl.pl:565-567` records why every atom is explicitly quoted: "bare identifiers are
always variables in this grammar".

### 2e. The langium grammar, and whether its demotion matters

`v6/prolog/ARCH.pl:663` (task `surface_dcg`, status `done`): "DCG is the CANONICAL parser
(langium demoted). NOT yet wired into compile_fixture (term form still the compiler
entry...)".

**The demotion does NOT retire the langium grammar.** `v6/dl/src/0_ast_bridge.ts:61-125`
still imports from `"langium"` and drives
`sharedServices.workspace.LangiumDocumentFactory.fromString<Gen.Program>(dlText, uri)`,
collecting `parseResult.lexerErrors` and `parseResult.parserErrors`.
`v6/dl/package.json:11` still runs `langium generate` and `:16` pins `langium ~4.3.1`.
So v6/dl parses `.dl6` with langium and v6/prolog parses `.dl6` with the DCG. Two
grammars, both live, 31 fixtures under `v6/dl/fixtures` and 281 under
`v6/prolog/compile/dl_view`.

The langium identifier is a DIFFERENT rule (`v6/dl/grammar/dl.langium:184`):

```
terminal ID: /[a-z_][a-z0-9_/]*/;
```

Slash-liberal, lowercase-only start, no dot. It already admits `/` inside a name, which
the DCG does not. Zero corpus files use a slashed rel name (grep over both fixture trees:
one comment hit, `v6/dl/fixtures/self-map.dl6:12`).

Two token-ordering hazards are already recorded in that grammar's own comments and both
would recur for a dot: `dl.langium:31-36` (a `'0'`/`'1'` keyword would beat the INT
terminal everywhere) and `:41-47` (`text`/`int` as keywords would make a column named
`text` unspellable), plus `:178-182` (`_` had to be declared a keyword to beat the ID
terminal). Verdict: any dot in the surface costs TWO grammar edits plus a
`langium generate` regeneration plus bridge work in `0_ast_bridge.ts`.

---

## 3. Name resolution today, pass by pass

The compile pipeline, from `v6/prolog/compile.pl:124-175` and `:253-281`:

```
parse (parse_dl.pl)  ->  prepare_program_for_compiler (:125, host pre-pass)
  ->  expand_program (:128, phases from 1_expansion.pl:23-40)
  ->  materialize_reference_target_rels (:129)
  ->  check_supported_subset_expanded (:134) / check_clock_program (:135)
      / check_world_shapes (:141)
  ->  program_column_types (:161, the typing fixpoint)
  ->  lower_program (:258)  ->  boot_statements (:263)  ->  emit (:269)
```

Expansion phase order, `v6/prolog/1_expansion.pl:23-40`: enum 10, decl_spread 20
(unwired), row_spread 30 (unwired), match 40, seq 42, coalesce 45, relation_edge 50.

### 3a. Rel names

Flat, global, one name to one rel. There is no scope, no import, no alias. The v5
statement of the same law is `plans/2026-07-06-magic-rel-audit.md:68`: "Rel **names are a
single flat global namespace** - one name -> one rel, no". `v6/prolog/compile/registry.pl`
is the compiler's construct inventory (`:1-8`), not a name resolver: it holds
`surface/5`, `expression/5`, `host_*` and `bind_*` rows keyed by `Functor/Arity`.

**dl6 has no module surface at all.** Zero `use` / `import` / `module` lines across all
360 `.dl6` files in `v6/`. v5 does have one, and it is a flat splice:
`src/frontend.rs:1-22` documents `use "path".` as "The smallest viable module system",
lexed as `Ident("use") Str(p) Dot` with no new keyword, expanded by recursively loading
each `Use` and splicing the core items in file order, deduping rel decls by name plus
cols, and hard-erroring on conflicting cols "so a typo never silently shadows a library
rel". Live in `std/entry.dl:25`, `.dl/git-graph.dl:32`, `examples/flow-services.dl:23`
and 7 more files.

### 3b. Columns

Positional. Names come from three places, in this order of authority:

1. A declaration: `rel a(x: int).` becomes `col_type(a/1, x, int)`
   (`parse_dl.pl:576-594`, `:615-620`).
2. Mined from the Prolog variable name at that argument position:
   `analyze.pl:288-294` `column_name_at/5` walks `ref_occurrence_args/3`, finds the
   binding whose variable is `==` to the argument, and passes the surface name through
   `snake_name/2` (`analyze.pl:297-301`, CamelCase to snake_case).
3. Fallback `col<N>` (`analyze.pl:293`).

There is no qualified column reference anywhere. `Rel.column` does not exist and never
has.

### 3c. What the type pass knows

`v6/prolog/analyze.pl:427-457`, `program_column_types/7`. A fixpoint over a small
lattice, header verbatim at `:427-441`: seed each column from literal witnesses keeping
"no witness" distinct from "text witness"; for each level rule build a variable-to-type
environment from positive body atoms plus left-to-right binds; a column's type is `text`
if any contributor says text, else `int` if any says int, else `none`; iterate to
stability, then `none` becomes `text`.

The authority rule is at `:441-442` and `:468-470`: `frozen(Type)` is a declared
`col_type/3` and "is never revised".

The column type vocabulary is `0_type_plane.pl:77-128`:

```
column_storage(_, int,  int).                                   % :77
column_storage(_, text, text).                                  % :78
column_storage(_, json, json).                                  % :88
column_storage(Types, list(Element), json) :- ...               % :115
column_storage(_, bool, bool).                                  % :124
column_storage(_, float, float).                                % :125
column_storage(Types, Name, ref(Name)) :-
    declared_type_name(Types, Name), !.                         % :126
column_storage(_, Name, _) :-
    throw(unsupported_construct(column_type_unknown(Name))).    % :127-128
```

`:126` is the whole nested-record door: a bare identifier in type position names a
DECLARED rel, and the column stores an integer endpoint into that rel's dictionary. The
file header `:3-28` states the model:

```
target(__id, key..., fields...)
parent(..., target_id INTEGER, ...)
```

Phase 5 of the roadmap (`plans/2026-07-29-v6-alpha-golden-plan.md:97-111`) adds
float/REAL plus `avg`, makes the `open(none)` fixpoint total, and gives refusal messages
`prolog:message//1` plus source location. Nothing in phase 5 touches name resolution.

### 3d. Where a qualified name would have to resolve, per pass

| pass | site | what it must do |
|---|---|---|
| parse | `parse_dl.pl:1501-1516` `factor/5`, plus `head_arg`/`atom_arg` positions | build a term for the chain; today there is no term shape for it |
| parse (langium) | `dl.langium:150-151` `ArgTerm`, `:145` `Member` | same, second grammar |
| expansion | a new phase in `1_expansion.pl:23-40`, or reuse of `relation_edge` at 50 | rewrite the chain into ordinary atoms, the way `0_relation_pattern.pl:32-38` rewrites relation-shaped terms |
| checks | `0_program_check.pl` | named refusal when the receiver is not a `ref(_)` column |
| typing | `analyze.pl:427-457` step 2 ("type each head argument expression") | give the chain a type |
| lowering, compiler door | `lower.pl:1026-1042` `expand_relation_pattern_rules/4` | emit the dictionary join |
| lowering, oracle door | `0_relation_pattern.pl:32-38` `expand_relation_values/2` | same rewrite for the reference engine |
| print | `print_dl.pl:550-561` | print the chain back byte-identically |

Eight sites. The last two doors are the structural cost: there are TWO engines and every
rewrite is written twice (`lower.pl:1000-1004` states the division of labour: capability
limits live in `lower.pl` "so they live here and not in `0_program_check.pl`", because
"the reference engine executes all of them").

---

## 4. Lowering doors

### 4a. There are already TWO reader spellings for a nested field, and they share ONE lowering

`v6/prolog/compile/dl_view/relation_depth2_construct_and_read.dl6`, verbatim:

```
rel repo(name: text).
rel fpath(name: text).
rel file(repo: repo, at: fpath).
rel span(file: file, start: int, end: int).
rel raw(repo_name: text, path_name: text, start: int, end: int).
rel coord(path_name: text, start: int, end: int).

repo(RepoName) <- raw(RepoName, _, _, _).
fpath(PathName) <- raw(_, PathName, _, _).
file(repo(RepoName2), fpath(PathName2)) <- raw(RepoName2, PathName2, _, _).
span(file(repo(RepoName3), fpath(PathName3)), Start, End) <- raw(RepoName3, PathName3, Start, End).
coord(PathName4, Start2, End2) <- span(file(_, fpath(PathName4)), Start2, End2).
```

Spelling 1, last line: a relation PATTERN destructures through the ref columns
(Souffle's camp).

`v6/prolog/compile/dl_view/relation_depth2_nested_decode_pattern.dl6:12`, same program,
different last line:

```
dcoord(PathName4, Start2, End2) <- span(File, Start2, End2), decode(File, {at: {name: PathName4}}).
```

Spelling 2: bind the ref column to a plain variable, then a `decode/2` brace pattern
(`registry.pl:83` `surface(decode/2, guard, no_refs, wrapper(expr_pair, lower), live)`).

And `relation_ref_column_fed_by_ref_variable_accepted.dl6:8` proves the third half:
`seen(At) <- loc(At, _).` A ref column binds to a bare variable and is passed whole.

Both spellings lower to the SAME thing. `lower.pl:775` `dictionary_table_name(TypeName,
Table) :- atomic_list_concat(['__ref_', TypeName], Table).`; `lower.pl:812-818`
`dictionary_render_expr/3` emits
`(SELECT d."__rendered" FROM <ref view> d WHERE d."__id" = <col>) AS <col>`, with the
EXPLAIN receipt recorded at `lower.pl:809-810`: "the inner query plans as `SEARCH d USING
INTEGER PRIMARY KEY (rowid=?)`, never a SCAN", count receipt
`v6/tsv2/tests/structPlane.test.ts` (file exists).

**Therefore: dot access is a THIRD surface spelling for a lowering that already ships.**
Its cost is entirely in the grammar and the two rewrite doors; the SQL is written. And
the join-versus-projection contradiction of section 1a resolves mechanically: the ref
column stores an INTEGER endpoint (`0_type_plane.pl:126`, `lower.pl:751`
`column_def(QuotedColumn, ref(_), Def) :- format(... '~w INTEGER NOT NULL' ...)`), so
`x.field` is a projection of a value that has to be fetched by a join. Both readings are
true at different layers. Nobody has written that sentence down anywhere in the repo.

### 4b. SQL: dots do NOT collide with `db.table`, but the underscore namespace has no guard

`lower.pl:195`:

```
quote_ident(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).
```

EVERY SQL identifier goes through it (call sites at `:186, 187, 312, 358, 688-690,
698-700, 706, 719-720, 791-793, 798, 813-815`, and throughout). A dot inside a
double-quoted SQLite identifier is one identifier, not a qualified reference. So the
naive "dots collide with db.table" worry is FALSE for this emitter, provided the name
reaches `quote_ident`.

The real SQL-side namespace is a set of underscore prefixes, `lower.pl:156-193`:

```
table_name(Name/_Arity, Name).                                        % :156
delta_table_name            -> '__delta_~w'                           % :159
frontier_table_name         -> '__frontier_~w'                        % :162
next_frontier_table_name    -> '__next_frontier_~w'                   % :165
pre_table_name              -> '__pre_~w'                             % :168
departure_frontier_table_name -> '__departure_frontier_~w'            % :178
ref_count_table_name        -> '__support_next_~w'                    % :193
dictionary_table_name       -> '__ref_<TypeName>'                     % :775-776
```

plus reserved COLUMN names `"_phase"`, `"_sequence"` (`lower.pl:189, 1681, 1685, 3014`)
and a reserved TABLE `"__tick"` (`lower.pl:587-594`).

**There is no guard on any of it.** Probe, read-only:

```
$ swipl -q -g "use_module(lower), lower:table_name('__delta_a'/1,T1),
               lower:delta_table_name(a/1,T2), lower:quote_ident(T1,Q1),
               lower:quote_ident(T2,Q2), format(...)" -t halt
user_rel_table=__delta_a delta_of_a=__delta_a quoted: "__delta_a" vs "__delta_a"
```

and `rel __delta_a(x: int).` parses clean (section 2c). Grep for a reserved-prefix check
across `0_program_check.pl`, `analyze.pl` and `parse_dl.pl` returns nothing. The
end-to-end consequence (two `CREATE TABLE` statements for one name in one program) is
UNVERIFIED: I did not run a full compile of such a program. The name collision itself is
proven above.

Style residue noticed in passing, one line: `lower.pl:192-193` renamed the predicate to
`ref_count_table_name/2` but the emitted table string is still `'__support_next_~w'`.
`CLAUDE.md` records the support-to-refCount rename as executed 2026-08-02; the SQL name
did not move.

Formerly-quadratic law applies to anything proposed here: a dot chain of length N must
carry a statement-COUNT test or an EXPLAIN SEARCH assertion, not end-state equality.
The precedent exists: `v6/prolog/ARCH.pl:795` (task `depth2_ref_fix`) records the exact
sabotage that only an EXPLAIN hop-count test caught, "disabling the memoization leaves
every fixture and the whole sweep GREEN while the depth-2 span insert grows 3->5
joins/arm; only the EXPLAIN hop-count test sees it."

### 4c. TS emit: a dot in a name is a hard break

`v6/prolog/emit_ts.pl:80-97`:

```
ref_name(Name/_Arity, Name).                                    % :80
upper_snake(Name/_Arity, Upper) :- upcase_atom(Name, Upper).    % :82
pascal_case(Name, Pascal) :-                                    % :90
    atomic_list_concat(Parts, '_', Name),
    maplist(capitalize_atom, Parts, CapitalizedParts),
    atomic_list_concat(CapitalizedParts, Pascal).
```

`pascal_case/2` splits on `_` only; `upper_snake/2` is `upcase_atom` and leaves
punctuation alone. A rel named `a.b` emits `A.b` as a JS function name and `A.B` as a SQL
constant name. Both are syntax errors in the emitted module. Any surface dot in a NAME
therefore needs a mangling step here; a surface dot in an EXPRESSION (option A/B section
6) never reaches these predicates and costs nothing at this door.

### 4d. rx lowering

The language law is that every construct shown carries a pure-rxjs lowering. The dot's
lowering is inherited, not new: a ref read is a join, and the join's rx form already
exists. `plans/2026-07-23-v6-reactive-datalog-isomorphism.md:70-77` gives the mapping
("A rel's value = `merge(seedFeed, rule1Feed, rule2Feed, ...)`. Each feed is a delta
stream; weights add in the Z-set. Multiple rules heading one rel = merge/union,
column-type-aligned (SQL UNION)"). A chain `X.a.b` is:

```
span$.pipe(
  withLatestFrom(refFile$),   map(([row, file]) => ...),   // one hop per dot
  withLatestFrom(refFpath$),  map(([row, fpath]) => ...)
)
```

which is the same operator sequence any two-atom body already lowers to. No new operator,
no new scheduler behaviour, no new teardown site. A NAMESPACE dot, by contrast, has no rx
form at all, because it is resolved at compile time and erased before any stream exists.
That asymmetry is itself an argument in section 6.

Same file `:74` also records that "v5's 'one rel = one rule kind' ban was a full-rebuild
artifact and is gone under incremental deltas", which matters for section 7.

---

## 5. TypeSpec's model-versus-namespace oddity, characterized precisely

Sources fetched today (2026-08-03) from the TypeSpec docs source tree.

**One `.` spells four different member relations.**

| relation | example | source |
|---|---|---|
| namespace to namespace | `namespace Foo.Bar.Baz { ... }` | `namespaces.md:47` |
| namespace to model | `sample: Foo.Bar.Baz.SampleModel;` | `namespaces.md:56` |
| enum to member | `alias North = Direction.North;` | `enums.md:79` |
| interface to operation | `op myWrite is MyReadWrite.write<int32>;` | `interfaces.md:76` |
| model to property | `Pet.name` | `models.md:237` |

**The escape hatch is a second symbol anyway.** `models.md:234-237`: "Some model property
meta types can be referenced using `::`", table row `| type | \`Pet.name::type\` |
Reference the type of the model property |`. So the "one symbol" design did not stay one
symbol; it grew `::` for the case where `.` had run out of meanings.

**The `using` hole: the dotted path and the lexical scope are two different resolvers.**
`namespaces.md:84-98`, verbatim:

```typespec
namespace One {
  model A {}
}

namespace Two {
  using One;
  alias B = A; // This is valid
}

alias C = Two.A; // This is not valid
alias C = Two.B; // This is valid
```

with the sentence at `:84`: "The bindings introduced by a `using` statement are local to
the namespace in which they are declared. They do not become part of the namespace
themselves." Inside `Two`, the name `A` resolves. From outside, `Two.A` does not. Same
identifier, same container, two answers, because `.` walks the DECLARATION tree and bare
lookup walks the LEXICAL scope chain.

**The downstream cost is name mangling.** `github.com/microsoft/typespec` issue #5532
("Decide how to handle conflicts between namespace and model"): a `Foo.Bar.Baz` namespace
containing a model named `Baz` collides in the C# emitter; the tentative decision is to
prepend an underscore to the conflicting namespace part, yielding `Foo.Bar._Baz`. An
alternative of appending a suffix was considered and not chosen.

**So the oddity, stated as an acceptance test:** a design reproduces the TypeSpec problem
if and only if a reader of `a.b.c` cannot say what `b` and `c` are without consulting the
symbol table, AND the language has more than one resolution rule reachable through the
same spelling. Section 6 grades each option against exactly that.

---

## 6. Decision packet: three options, costed, none picked

Common calibration for "parse cost in lines", against precedents in the same file:
`mul_expr_rest/6` (`parse_dl.pl:1483-1495`) is 13 lines for a three-way infix loop;
`list_term/5` (`:1673-1682`) is 10 lines for a bracket form with one special branch;
`print_brace_key/3` (`print_dl.pl:550-555`) is 6 lines for the printer inverse of a
three-shape key.

### Option A: ONE `.` for both namespace access and member access, resolution by inference

**Resolution rules, prolog-style sketch (not code).**

```
% resolve_chain(+Chain, +Env, -Kind, -Type)
resolve_chain(proj(Receiver, Field), Env, column, FieldType) :-
    resolve_chain(Receiver, Env, _, ref(TypeName)),
    type_definition(Types, TypeName, Columns, ColumnTypes),
    nth1(Position, Columns, Field),
    nth1(Position, ColumnTypes, FieldType).

resolve_chain(proj(Receiver, Name), Env, rel, Ref) :-       % namespace arm
    resolve_chain(Receiver, Env, module, ModuleId),
    module_exports(ModuleId, Name, Ref).

resolve_chain(var(V), Env, column, Type) :- memberchk(V-Type, Env).
resolve_chain(name(N), _, module, N) :- declared_module(N).
```

Note what this buys for free: the receiver's type is ALWAYS `frozen` in the typing
fixpoint, because a ref column type must be a DECLARED rel name
(`0_type_plane.pl:126-128` throws `column_type_unknown` otherwise). So the member arm is
a table LOOKUP, never an inference. Zero new lattice, zero new fixpoint iterations.

**Where ambiguity bites.** Between the two arms, at the FIRST segment only. Given
`rel a(x: int).` and a hypothetical module `a`, the chain `a.x` is either "the module
`a`'s rel `x`" or "column `x` of a value of type `a`". Today the ambiguity is VACUOUS,
because dl6 has zero module surface (section 3a: no `use`/`import`/`module` in 360
`.dl6` files). It becomes real the day a module surface lands. v5's module surface is a
flat splice with a hard error on conflicting decls (`src/frontend.rs:19-22`), which is
the shape that has NO namespace to dot into.

**Worked ambiguous example, and how it resolves or refuses.**

```
rel repo(name: text).
rel file(repo: repo, at: fpath).
rel hit(who: text).

hit(F.repo.name) <- file(F, _).      % member arm: F : ref(file), file.repo : ref(repo),
                                     % repo.name : text.  Resolves. Lowers to two
                                     % __ref_ dictionary joins.

hit(repo.name)   <- file(_, _).      % first segment is a bare name, not a bound
                                     % variable. Under option A this is EITHER a
                                     % module lookup OR a type error. With no module
                                     % surface: named refusal
                                     % dot_receiver_not_a_bound_value(repo).
                                     % With a module surface named `repo`: silently a
                                     % different program.
```

That second line is the whole risk of option A in one line. The refusal today is total;
the day modules land, the same text means something else with no diagnostic.

**Costs.**

| door | cost |
|---|---|
| DCG parse | postfix `(. ident)*` loop on `factor/5` (`parse_dl.pl:1501-1516`) ~10 lines; guard "no chain after a numeric literal" (else `1.q` parses as `proj(1,q)` because `float_tail` at `:461` needs a digit and `integer_lit` at `:422` then matches the `1`) ~3; guard "dot not followed by an ident-start char" ~3; head/atom-arg wiring ~6. **~22 lines** |
| terminator disambiguation | require whitespace/EOF/comment after the statement dot, at all 9 `lit_dcg(\`.\`)` sites. **~9 edits plus one helper**, and it is a behaviour change to every existing statement |
| langium parse | new postfix production, token-order review against the three recorded hazards (`dl.langium:31-36, 41-47, 178-182`), `langium generate`, bridge in `0_ast_bridge.ts`. **~10 grammar lines plus regeneration plus bridge** |
| print | inverse of the chain, calibrated on `print_brace_key/3`. **~8 lines** |
| resolution | one expansion phase in `1_expansion.pl:23-40` rewriting `proj/2` into ordinary ref atoms. **~40-60 lines**, mirroring `0_relation_pattern.pl` (102 lines total) |
| second door | the same rewrite for the oracle (`0_relation_pattern.pl`). **~40-60 lines** |
| typing | none beyond the lookup above (receiver types are always `frozen`) |
| SQL | **zero**: `__ref_` dictionary join already emitted (`lower.pl:775, 812-818`) |
| TS emit | **zero** for expression dots; names never carry a dot under this option |
| rx | **zero**: inherited from the join |
| checks | one named refusal `dot_receiver_not_a_relation(Var, Type, Field)` plus registration in `0_refusal_messages.pl` |
| migration | **zero forced**. 281 `dl_view` + 31 `v6/dl` fixtures contain no bare dot outside quoted atoms, `...` spreads, and 2 files with float literals (grep). Purely additive |

**TypeSpec-oddity verdict: REPRODUCES, conditionally.** Today the second arm has nothing
to resolve to, so `a.b.c` is unambiguously a member chain and a reader needs no symbol
table. The moment a module or namespace construct lands, `a.b.c` needs the symbol table
for segment 1, and the `using` hole reappears exactly: v5's `use "path".` splices names
into the flat space (`src/frontend.rs:4-6`), so a spliced name would be reachable bare
but not by a dotted path, which is `namespaces.md:96` verbatim.

### Option B: `::` for namespaces, `.` for members

**Costs, as a delta against A.**

| door | delta |
|---|---|
| DCG parse | `::` is CHEAPER than `.` in the lexer: no float collision, no terminator collision, no change to the 9 `lit_dcg(\`.\`)` sites. A `::` chain alone is ~12 lines. If BOTH symbols ship, add A's `.` cost |
| langium | one extra terminal; `:` is already a keyword in `ColumnDecl` (`dl.langium:27`) and `Member` (`:145`), so `::` must be lexed as one token before `:`, the same maximal-munch discipline the grammar already documents at `:31-36` |
| everything else | identical to A |

**Vocabulary cost, which is the real objection.** The construct-name law admits only
rxjs, prolog, or SQL words and spellings. Prolog's module qualifier is `Module:Goal`,
ONE colon, and the codebase writes it constantly (`1_expansion.pl:23-40`
`enum_expand:expand_enum_in_context`, `compile.pl:180` `emit_ts:emit_program`,
`lower.pl` throughout). SQL's qualifier is `.` (`schema.table`). rxjs has neither.
`::` is from NONE of the three. And single `:` is already spoken for twice in dl6:
column typing (`parse_dl.pl:617`) and json brace keys (`parse_dl.pl:1665`). So the
prolog-faithful spelling is unavailable and the vocabulary-faithful alternatives are
exhausted.

How it reads next to prolog's own `:`:

```
rel  file(repo: repo, at: fpath).          % `:` = column type
hit(Span::file.repo.name) <- span(Span, _, _).   % `::` = module, `.` = member
```

A reader who knows prolog will read `Span::file` as a module qualification of the goal
`file`, which is what it looks like and not what it means.

**TypeSpec-oddity verdict: AVOIDS it, by construction.** Two symbols, one resolution rule
each, so `a::b.c` reads left to right with no symbol table: `b` is a member of module `a`,
`c` is a column of `b`. This is Rust's answer (`::` paths, `.` fields) and it is precisely
why Rust never has the TypeSpec problem. It also matches SCIP's own instinct, which uses
three distinct terminators for member kinds (`#` Type, `.` Term, `(...)` Method, per
`/Users/chrishafley/projects/sprefa-plan-typeir/PLAN2.md:87`).

### Option C: no surface dots (today's answer)

**Cost: zero.** Nothing to build, nothing to migrate, the two existing reader spellings
stay (section 4a), and the underscore prefixes stay as the namespace.

**The ergonomic loss, stated honestly.**

1. Reading one leaf field of a depth-3 value costs a full destructure that names every
   intermediate. From `v6/prolog/compile/dl_view/relation_depth2_construct_and_read.dl6:12`:
   `coord(PathName4, Start2, End2) <- span(file(_, fpath(PathName4)), Start2, End2).`
   The dotted form would be
   `coord(F.at.name, S, E) <- span(F, S, E).` The tax is one wildcard and one wrapper
   per level skipped, growing linearly with depth. `relation_depth3_*` fixtures pay it
   at depth 3.
2. Namespacing is done with string surgery outside the language. `flow-panel.html:1062`
   `const edgeTable = nodeTable.replace(/_node$/, '_edge');` and `:1075`
   `nodeTable.replace(/^rel_/, '').replace(/_node$/, '')`, over a
   `SELECT name FROM sqlite_master WHERE ... name LIKE 'rel_%_node'` query at `:1056`.
3. The compiler's own `__`-prefixed namespace has no guard (section 4b), and nothing
   would give it one under option C except a bespoke check.
4. Two reader spellings already exist for one lowering (section 4a) and neither reads as
   navigation.

**TypeSpec-oddity verdict: AVOIDS it, trivially and permanently.** No dot, no resolver,
no ambiguity. The failure mode option C has instead is the one at points 2 and 3: name
conventions enforced by regex outside the compiler, with a hardcoded exception list
(`flow-panel.html:1011` `BUILTIN_LAYERS`, described at `:1077` as "fill in built-in graph
pairs that don't follow the `_node`/`_edge` convention").

### Summary grid

| | A: one `.` | B: `::` + `.` | C: no dots |
|---|---|---|---|
| DCG parse lines | ~22 plus 9 terminator edits | ~12 for `::` alone, ~34 for both | 0 |
| langium lines | ~10 + regen + bridge | ~12 + regen + bridge | 0 |
| resolution + second door | ~80-120 | ~80-120 | 0 |
| SQL / rx / TS emit | 0 / 0 / 0 | 0 / 0 / 0 | 0 |
| corpus migration | 0 forced | 0 forced | 0 |
| ambiguity today | none (no modules to collide) | none | none |
| ambiguity once modules land | segment 1 needs the symbol table | none | n/a |
| TypeSpec oddity | reproduces, conditionally | avoids | avoids |
| vocabulary law | clean (`.` is SQL's) | violated (`::` is nobody's) | clean |
| ergonomics at depth 3 | best | best | worst |

---

## 7. Interfaces and traits: do we need one?

### 7a. Every place the codebase already fakes one

| # | fake | receipt | shape |
|---|---|---|---|
| 1 | host executor contract | `registry.pl:334-338`: `host_executor_contract(sprefa_extract, [col(path,text), col(digest,text)])`, `sprefa_extract_repo` with 3 cols, `host_executor_contract(shell, _)` | a NAMED contract = a positional column list |
| 2 | host executor DISPATCH | `registry.pl:320-332` `host_execution/3` selects the contract by `sub_string` matching on the template TEXT: `"\"$DL_EXTRACT_BIN\" "` prefix plus a `"{repo}/{path}"` or `"{path}"` suffix | structural typing implemented by string matching |
| 3 | host input roles | `registry.pl:345+` `host_input_contract/3`, `host_input_roles/3` | per-host, positional, compiler metadata |
| 4 | world push source contract | `registry.pl:275-279` `bind_definition(watch, [col(glob,text), col(path,text), col(digest,text)])` + `bind_executor/2` | same shape as 1, for input binds |
| 5 | operator typeclass | `registry.pl:232-248` `expression/5`'s last column: `both_number`, `both_int`, `same_type`, `text_only`, 12 rows | an ad-hoc typeclass table over operators |
| 6 | graph-layer protocol | `flow-panel.html:1051-1090`: any table pair `rel_X_node` + `rel_X_edge` is a layer; discovery is `nodeTable.replace(/_node$/, '_edge')` (`:1062`) plus `PRAGMA table_info` column sniffing (`:1069, :1072`), plus `BUILTIN_LAYERS` (`:1011`) as the hardcoded exception list for pairs that break the convention | a structural interface faked by a NAME SUFFIX, checked outside the language |
| 7 | TS interface law | `sprefa-store/js/src/engine/types.ts`: 19 `I`-prefixed interfaces (`ISqlRunner:58`, `IRelStore:167`, `IStore:259`, `ITemporalStore:335`, ...) plus a `*Statics` twin per class (`IRelStoreStatics:212`, `IStoreStatics:299`, ...); `dl/src/0_types.ts`: `IBindRunner:268`, `IDlRuntime:526`, `IHostRunner:557`, `IRowCodec:630` | host-language interfaces, already first class in TS |
| 8 | v5 rust traits | `src/rels/mod.rs:96` `RelKind`, `src/rels/extract_family.rs:108` `ExtractFamily`, `src/ingest/mod.rs:41` `IngestLang`, `src/graph/typegraph/mod.rs:479` `TypeLang`, `src/storage.rs:52` `Storage`, `src/effect.rs:148` `EffectExec`, 13 total | host-language traits, already first class in Rust |
| 9 | watch source seam | `v6/prolog/conformance/rulings.pl:376-385` `ruling(watcher_dep, fs_watch_until_bench_regression, ...)`, "STAY on node fs.watch behind the IWatchSource seam" | one-adapter swap, TS interface |

The "one rel = one rule kind" law from `CLAUDE.md:93` was NOT found as an enforcement
site in v6. `plans/2026-07-23-v6-reactive-datalog-isomorphism.md:74` states the opposite
for v6: "v5's 'one rel = one rule kind' ban was a full-rebuild artifact and is gone under
incremental deltas." So it is not an interface the codebase fakes; it is a v5 law that
v6 dropped.

### 7b. The argument, from what the code needs today

Cases 1 through 5 are COMPILER metadata. A user program cannot declare them and there is
no attested case where it wants to: `registry.pl:1-8` states its own purpose as "the
compiler's surface construct inventory", and the four `expression/5` type rules exist so
the emitter can pick an SQL operator, not so a program can talk about types. Promoting
them to a user-facing `interface` construct moves the compiler's inventory into user
space, which is the opposite of what that file is for.

Cases 7, 8, 9 are already first class in their host languages. dl6 does not need to
re-declare them.

**Case 6 is the only one where a user program's rels must satisfy a shape that a
CONSUMER declares.** The consumer is a JS panel reading `sqlite_master`. It costs, today:
a regex per relation kind (`:1062`, `:1075`), a `PRAGMA table_info` probe per table
(`:1069`, `:1072`), and a hardcoded exception list (`:1011`, `:1077-1090`). The smallest
construct that would retire all three is not `interface`: it is a DECLARED fact the panel
queries instead of sniffing `sqlite_master`, which is a wiring change, not a type system
change.

**Case 2 is the one place an explicit contract would replace a string sniff, and the
failure it prevents has already been observed in the field.** `registry.pl:302-305`
records it verbatim: the clause order matters because a repo template "would otherwise be
claimed by the unscoped row and then thrown out by its contract (measured:
`host_executor_mismatch`, which is what a cold author saw before this row existed)".
The smallest construct that covers this ONE case is a contract word on the `sh`
declaration, not a general trait system:

```
sh span_scan(file_digest: text, query_digest: text) -> (line: int, text: text)
   = `span {file_digest} $query_digest`.                      % today
sh span_scan(...) -> (...) is sprefa_extract = `...`.         % the one-word version
```

Cost: one optional clause in `sh_decl_stmt/3` (`parse_dl.pl:886-912`), roughly 6 parse
lines plus the printer inverse; the check is `memberchk` against the existing
`host_executor_contract/2` rows; the payoff is deleting the `sub_string` dispatch at
`registry.pl:320-332`.

**The honest answer to "do we need interfaces or traits":** nothing in the code needs a
general one. One call site (host executor selection) needs a named contract instead of a
string sniff, and one consumer (the flow panel) needs a declared fact instead of a table
name regex. Both are one-word or one-fact changes. A trait system with subtyping,
bounds, or implementations would have no second customer in this repo. That is the
argument; the decision is not mine.

---

## 8. ARCH-style task rows and blocking questions

Shape is `task(Name, Status, Needs)`, per `v6/prolog/ARCH.pl:651` and rows `:655`, `:751`,
`:800`. All statuses `unbuilt`. Nothing here is dispatchable until question B1 is
answered.

```prolog
task(dot_meaning_ruling,      unbuilt, []).                        % write down that a ref column stores an INTEGER endpoint, so `.` is a projection whose fetch is a join; retires the LANG.md:37 vs 2026-07-21:64 contradiction without new code
task(statement_dot_end_token, unbuilt, []).                        % require whitespace/EOF/comment after the statement dot at all 9 lit_dcg(`.`) sites in parse_dl.pl; prerequisite for ANY surface dot; independently a correctness fix
task(reserved_table_prefix_guard, unbuilt, []).                    % named refusal for a rel name colliding with the __delta_/__frontier_/__pre_/__ref_/__tick namespace; fail-first receipt = `rel __delta_a(x: int).` compiling today
task(dot_chain_parse,         unbuilt, [statement_dot_end_token]). % postfix (. ident)* on factor/5 + numeric-literal guard + print_dl inverse; DCG only
task(dot_chain_langium,       unbuilt, [dot_chain_parse]).         % same production in dl.langium + langium generate + 0_ast_bridge.ts; second grammar, token-order review vs dl.langium:31-36/41-47/178-182
task(dot_chain_expand,        unbuilt, [dot_chain_parse]).         % expansion phase rewriting proj/2 into ref atoms, compiler door (lower.pl:1026) and oracle door (0_relation_pattern.pl:32)
task(dot_chain_refusals,      unbuilt, [dot_chain_expand]).        % dot_receiver_not_a_relation + unknown_field named refusals, registered in 0_refusal_messages.pl
task(dot_chain_hop_receipts,  unbuilt, [dot_chain_expand]).        % EXPLAIN SEARCH + join-COUNT tests per chain depth, per the formerly-quadratic law; precedent ARCH.pl:795
task(module_surface_dl6,      unbuilt, []).                        % dl6 has no use/import/module at all; v5's is a flat splice (src/frontend.rs:1-22). BLOCKS any namespace symbol: a namespace with nothing to scope is not a construct
task(namespace_symbol_choice, unbuilt, [module_surface_dl6]).      % `.` overloaded (option A) vs `::` (option B); the vocabulary law and the TypeSpec test disagree, so it is a user call
task(host_contract_word,      unbuilt, []).                        % optional `is <executor>` on the sh decl, retiring the sub_string dispatch at registry.pl:320-332; the ONLY interface-shaped need found
task(graph_layer_fact,        unbuilt, []).                        % a declared layer fact the flow panel queries, retiring flow-panel.html:1062/1075 regexes and the BUILTIN_LAYERS exception list
```

### Blocking questions, in the order they block

**B1.** Is a surface dot a projection over a ref column, or also a namespace qualifier?
(Answer "projection only" and every namespace row above dies; answer "both" and B2 binds.)

**B2.** Does dl6 get a module or file surface at all, or does the flat global rel
namespace with underscore prefixes stay the answer forever?

**B3.** If both a namespace and a member spelling ship, is the namespace symbol `.`
(TypeSpec's answer, ambiguous, vocabulary-clean) or `::` (Rust's answer, unambiguous,
outside the rxjs/prolog/SQL vocabulary)?

**B4.** The 2026-05-07 lock at
`chat_log/20260507.2.v4-rule-engine-respec-and-memory-audit.md:58` says "namespace via
dot-access (`write_cursor` over `write(:cursor)`)". Did that mean a real dot, or an
underscore name convention called dot-access loosely?

**B5.** May a dot chain be written in HEAD position (`hit(F.repo.name) <- file(F, _).`),
or is it body-only with an explicit `:=` bind?

**B6.** Does the dot spelling REPLACE either of the two existing reader spellings
(relation pattern, `decode/2` brace) or is a third spelling acceptable?

**B7.** Should `task(statement_dot_end_token)` and `task(reserved_table_prefix_guard)`
ship regardless of the dot decision, since both are correctness gaps today?

**B8.** Does the `sh` declaration get an explicit executor contract word, retiring the
template-text sniff at `registry.pl:320-332`?

---

## 9. Deviations and unverified items

| item | status |
|---|---|
| brief item "lang_ext printer-ignore seam" as an existing fake interface | **NOT FOUND.** `grep -rin "lang_ext\|langext"` across the whole worktree returns three hits: `brief.md` itself, `chat_log/20260519.1...:34` and `.agents/memory/project_cross_file_entity_graph.md:79`, both naming a v4-era Rust `LangExtract` trait. `grep -rn "LangExtract" src/ v6/` returns zero. Item STOPPED. Closest live analogues are `src/ingest/mod.rs:41` `IngestLang` and `src/graph/typegraph/mod.rs:479` `TypeLang`, both listed in section 7a row 8 |
| brief item "one-rel-one-rule-kind law" as a fake interface | **CONTRADICTED for v6.** `plans/2026-07-23-v6-reactive-datalog-isomorphism.md:74` says the ban "was a full-rebuild artifact and is gone under incremental deltas". No enforcement site found in `v6/prolog/0_program_check.pl` or `analyze.pl`. Recorded, not counted as a fake |
| brief cite `parse_dl.pl:411-416` for the identifier rule | rule is `:408-418`; `:411` and `:416` are the two character-class tests inside it. Minor, recorded |
| end-to-end consequence of the `__delta_a` name collision | **UNVERIFIED.** The name collision is proven (section 4b probe). Whether the emitter then issues two `CREATE TABLE` statements for one name, or fails earlier, was not tested; that needs a full compile, which I did not run |
| `plans/2026-07-21-v6-runtime-decomposition.md` `Proj` decision | recorded as a decision inside a Rust crate tree (`:130-160`) that v6 did not build. Its ARGUMENT is live; its architecture is not. Flagged, not asserted as binding |
| owner's 2026-05-07 "namespace via dot-access" lock | **AMBIGUOUS**, see B4. Quoted verbatim, not interpreted |
| rx lowering sketch in section 4d | the operator sequence is inferred from `plans/2026-07-23-v6-reactive-datalog-isomorphism.md:70-77` and the existing join lowering, not read off a written rx lowering for a dot. No such lowering exists to read. Flagged as a sketch |
| `plans/` sweep coverage | grepped `plans/` (290 files), `chat_log/` (277 files), `v6/prolog/ARCH.pl`, `v6/prolog/conformance/rulings.pl` (546 lines, ZERO dot or namespace mentions) for dot / namespace / qualified-name / member-access. Every hit is inventoried in section 1 or is a `jsonp` dotted-path-string hit belonging to the JSON arm, which is a different construct (`plans/2026-07-30-json-query-language-recovery.md:233, 287`) |
