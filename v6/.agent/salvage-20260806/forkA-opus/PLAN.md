# Branch A, contribute-only

Lane note: this lane ran in a read-only planning harness with no Write tool and
returned its deliverables inline. The coordinator transcribed them verbatim.

| § | Answers |
|---|---|
| 0 | Where the spec was wrong |
| 1 | What a dotted head does today, at every one of the five named sites |
| 2 | Type signatures, pseudo-code, storage layout, read/write sequence, uniqueness conditions, and where those four layers disagree |
| 3 | Fail-first test, sabotage receipt, existing gates, exact commands |

## 0. Where the spec was wrong

| Claim | Reality | Receipt |
|---|---|---|
| Read `plans/2026-08-03-module-catalog-ruling.md` in full | Absent from the worktree and from all of git history | `git log --oneline --all --diff-filter=A -- '*module-catalog*'` prints nothing; `find . -name '*module-catalog*'` prints nothing |
| (substitute) | Its 11 stances survive quoted with line refs inside `plans/2026-08-03-modscope-plan.md:85-88,127-144,169-179,190-240,282-284,516` | read in full |
| `v6/justfile:51` "expect: 269/269" | 351 tests, all passing | `cd v6/prolog/compile && swipl -q -l test/plunit_tests.pl -g run_tests -g halt` ends `[351/351] fact_seeding:rege..int_column_compiles ... passed` |
| fixtures=302 | Confirmed | `cd v6/prolog/conformance && swipl -q -l go.pl -g go -g halt \| grep -c '^PASS'` prints `302`, zero non-PASS lines |

The five refusals `module_name_collision`, `container_and_leaf`,
`non_static_rel_arg`, `growing_instantiation_cycle`, `unresolvable_path` appear
in exactly three files: `SPEC.md`, `plans/2026-08-03-modscope-plan.md`,
`.agent/salvage-20260805/GWORD-OPUS-VERDICT.md`. Zero code sites.

## 1. What a dotted head does today

### 1.1 Text door: every dotted spelling outside an argument is a parse error

`head_atom/5` at `v6/prolog/compile/parse_dl.pl:1062-1070` is
`ident(Name), ws0, "(", head_args, ")"`. No dot route. Same for
`relatom_item/5` at `:1538-1539`, the negation clause at `:1323`, and
`decl_a_stmt/2` at `:578` and `:590`.

Measured, all three positions:

```
orchard.tree(X) <+ src(X).      =>  THROWN dl_parse_error(statement,position(1,8))
out(X) <- orchard.tree(X).      =>  THROWN dl_parse_error(statement,position(1,23))
rel orchard.tree(tree_id: int). =>  THROWN dl_parse_error(statement,position(1,12))
```

Each position is the dot itself. A generic `dl_parse_error(statement, ...)`
carries no refusal name, so no fixture can pin it.

### 1.2 Term door: a dotted head passes through the dot phase unnamed

`rewrite_head/4` at `v6/prolog/0_dot_expand.pl:90-96` does
`Head0 =.. [Name | Args0]`, so a head that IS a `dot_get` is decomposed as
functor `dot_get` with two arguments, and neither argument holds a dot. Verified:

```
prog([],[<-(dot_get(orchard,tree),src(_,_))])
```

A rel literally named `dot_get/2` is minted, silently. The spec's anchor "an ATOM
root is refused by construction" holds for argument position only
(`check_dot_receiver/3`, `:171-177`), never for a head functor.

### 1.3 The rest of the pipeline is indifferent to a dot in a rel name

`rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).` at
`v6/prolog/conformance/body.pl:26` is the whole ref identity. Handing the
compiler a dotted ATOM functor through the term door compiles end to end:

```
CREATE TABLE "orchard.tree" ("col1" INTEGER NOT NULL, ... ) WITHOUT ROWID
CREATE TEMP TABLE "__delta_orchard.tree" (...)
```

`table_name(Name/_Arity, Name)` at `v6/prolog/lower.pl:162` quotes it into legal
SQL. Emission does not:

```ts
const relColumns: Record<string, readonly string[]> = {
  orchard.tree: ["col1"],
  src: ["col1"],
};
```

`orchard.tree:` is a TypeScript syntax error. `ref_name(Name/_Arity, Name)` at
`v6/prolog/emit_ts.pl:81` drops arity and `rel_columns_entry_line/2` at
`:661-664` writes an unquoted object key. The compiler emits a module that cannot
be imported, with no refusal anywhere on the path. That is the exact failure
branch A exists to prevent, and it already runs today.

### 1.4 Every site that changes

| Site | Receipt | Change |
|---|---|---|
| parse, head | `parse_dl.pl:1062` `head_atom/5` | accept `ident ('.' ident)*`, join with `.` into one atom functor |
| parse, body atom | `parse_dl.pl:1538` `relatom_item/5` | same |
| parse, negation | `parse_dl.pl:1323` | same |
| parse, decl | `parse_dl.pl:578`, `:590` `decl_a_stmt/2` | same, so a path has a declaration site |
| parse, query | `parse_dl.pl:999` `head_atom` call inside `query_stmt` | inherits the head change |
| named-arg table | `parse_dl.pl:95-98` `record_column_order/2`, read at `:1108` | untouched; keys stay the SURFACE dotted name |
| phase list | `v6/prolog/1_expansion.pl:27-52` | new `expansion_phase(43, modpath, modpath_expand:expand_modpath_in_context)` |
| ref inventory | `v6/prolog/compile.pl:157` | untouched; it now sorts FLAT names |
| arrival targets | `compile.pl:175` | untouched; a contributed child is a derived ref and drops out already |
| decl injection | `compile.pl:121`, `:131` | the pattern the new phase copies for its `catalog_parent/2` decls |
| table naming | `lower.pl:162` | untouched; arity is folded into the digest instead |
| catalog seed | `lower.pl:643` `catalog_row_ddl(_Decls, RelPlans, [Statement])` | first argument is already threaded and unused; branch A fills it |
| catalog rows | `lower.pl:669-673` `catalog_rel_rows/4` | parent_id from the path table, local_name = last segment |
| catalog gate | `analyze.pl:190-208` | untouched |
| refusal set | thrown from the new phase | both doors get them free: `engine.pl:548` and `compile.pl:148` both call `expand_program/3` |

## 2. The design

### 2.1 Signatures, each with its body as a comment

```prolog
% v6/prolog/0_modpath_expand.pl
:- module(modpath_expand, [expand_modpath_in_context/3]).

expand_modpath_in_context(+ExpansionContext, +prog(Decls0, Rules0), -prog(Decls, Rules)).
%   declared_paths(Decls0, PathTable),
%   check_path_uniqueness(PathTable),          % U2, U3, U4 below
%   maplist(rewrite_rule_paths(PathTable), Rules0, Rules),
%   path_catalog_decls(PathTable, ParentDecls),
%   append(Decls0, ParentDecls, Decls).
%   A program with no dotted name returns byte-identical, the discipline
%   0_dot_expand.pl:33-34 already states for itself.

declared_paths(+Decls, -PathTable).
%   PathTable = list of path_decl(SegmentList, Arity, FlatName, leaf|container).
%   findall over declared_refs/2 (analyze.pl:244, which already unions kind/2,
%   keyed/2, keep/2 and col_type/3), keep the refs whose name holds a dot, add
%   one container entry per proper prefix of each such SegmentList.

flat_name(+SegmentList, +Arity, -FlatName).
%   atomic_list_concat(SegmentList, '__', Stem),
%   atomic_list_concat(SegmentList, '.', DottedName),
%   format(atom(DigestInput), '~w/~w', [DottedName, Arity]),
%   crypto_data_hash(DigestInput, Sha, [algorithm(sha256), encoding(utf8)]),
%   sub_atom(Sha, 0, 8, _, Digest),
%   atomic_list_concat([Stem, '__', Digest], FlatName).
%   Measured: 'orchard.tree/1' -> f9fc8ea9, so orchard.tree/1 -> orchard__tree__f9fc8ea9.

resolve_path(+PathTable, +DottedRef, -FlatRef).
%   (  memberchk(path_decl(SegmentList, Arity, FlatName, leaf), PathTable)
%   -> FlatRef = FlatName/Arity
%   ;  throw(unsupported_construct(unresolvable_path(DottedName/Arity)))
%   ).
%   THE BRANCH A GATE, one clause. Arity-exact by construction.
```

### 2.2 Storage layout

| Plane | Path carrier | Key | Lives from / to |
|---|---|---|---|
| terms, phases 10 to 42 | atom functor with dots, `'orchard.tree'` | `'orchard.tree'/1` | parse to phase 43 |
| terms, phases 43 to 50 and all of analyze/lower/emit | flat atom functor | `orchard__tree__f9fc8ea9/1` | phase 43 to emission |
| sqlite tables | flat name, quoted | `"orchard__tree__f9fc8ea9"` and its siblings | boot to teardown |
| `__catalog_rel` | `parent_id` edge, `local_name` = `tree` | `rel_id` | boot seed, one `INSERT OR IGNORE` |
| named-arg table | dotted surface name | `rel_column_order_fact('orchard.tree', Cols)` | parse only |

Container rows carry `kind='rel'`, `ordinal=0`, no table and no relplan.

### 2.3 Uniqueness conditions

| # | Condition | Refusal on breach |
|---|---|---|
| U1 | `(SegmentList, Arity) -> FlatName` injective up to a sha256-8 collision | none; caught by U2 |
| U2 | No FlatName equals any declared root rel name, and no two FlatNames are equal | `module_name_collision` |
| U3 | Each SegmentList has exactly one Arity across the whole program | `module_name_collision` |
| U4 | A SegmentList is a container or a leaf, never both | `container_and_leaf` |
| U5 | Every dotted ref in a rule HEAD is a `leaf` entry of PathTable at that exact arity | `unresolvable_path` |

### 2.4 Where the four layers disagree

1. `flat_name/3` takes Arity; the catalog's `local_name` is the last segment and
   holds no arity. The catalog cannot represent `a.b/1` beside `a.b/2`. U3 closes
   the gap with a refusal instead of with a schema column.
2. The sequence mangles at phase 43, and `record_column_order/2` at parse keys on
   the surface dotted name. One rel is alive under two keys inside one compile.
   Mangling at parse time instead would break named arguments, so the split is
   deliberate.
3. U5 constrains rule heads. Decls create ancestors implicitly, so branch A is
   contribute-only for RULES and create-on-write for DECLS. `rel orchard().`
   cannot serve as the module declaration site today: it parses to `prog([], [])`,
   verified, because `typed_decl_entries(_, [], [])` at `parse_dl.pl:712` yields
   nothing.
4. Storage says containers have no table; `program_refs/2` at `analyze.pl:255`
   would still collect a container ref if a body atom named one bare.

## 3. The proof

### 3.1 Fail-first test

New group in `v6/prolog/compile/test/plunit_tests.pl`, beside `catalog_g1`:

```prolog
test(dotted_head_without_a_decl_refuses_by_name,
     [throws(unsupported_construct(unresolvable_path('orchard.tree'/1)))]) :-
    Rule =.. ['<-', 'orchard.tree'(TreeId), source_row(TreeId)],
    check_supported_subset(prog([], [Rule])).

test(declared_dotted_head_lowers_to_the_mangled_table) :-
    Rule =.. ['<-', 'orchard.tree'(TreeId), source_row(TreeId)],
    Prog = prog([col_type('orchard.tree'/1, tree_id, int)], [Rule]),
    once(( program_plan(fixture(modpath, Prog, [source_row(1)], [], [])-[], Plan),
           lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)) )),
    memberchk('CREATE TABLE "orchard__tree__f9fc8ea9" ("tree_id" INTEGER NOT NULL, "__refcount" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("tree_id")) WITHOUT ROWID', Ddl).

test(no_dot_reaches_emission) :-
    ... emit_ts:emit_program(...), \+ sub_atom(Text, _, _, _, 'orchard.tree').
```

Command: `cd v6/prolog/compile && swipl -q -l test/plunit_tests.pl -g run_tests -g halt`.
Current tally 351/351; the group adds 5.

### 3.2 Sabotage receipt

Already collected, before any code exists. Leaving the dotted atom in the term
stream makes the compiler produce an unimportable module rather than refusing:

```ts
const relColumns: Record<string, readonly string[]> = {
  orchard.tree: ["col1"],
  src: ["col1"],
};
```

The `no_dot_reaches_emission` test fails on that text and passes on the mangled
text, so the test is pinned to a real failure and not to itself.

### 3.3 Existing gates that catch a regression

| Command | Current expectation | What it catches |
|---|---|---|
| `swipl -q -l go.pl -g go -g halt` in `conformance` | 302 PASS (measured) | oracle drift; both doors call `expand_program/3`, so phase 43 changes both at once |
| `swipl -q -l test/plunit_tests.pl -g run_tests -g halt` | 351/351 (measured) | `catalog_ids_are_positional` pins 14 exact tuples; a container row at the wrong position fails immediately |
| `bash compile/scripts/text_door_receipt.sh` | 196/196/0 | a digest computed from anything the two doors spell differently |
| `bash compile/scripts/roundtrip.sh` | identity over 302 fixtures | printer loss on a dotted decl. NOTE: this script regenerates `dl_view/*.dl6`, so it writes files |
| `bash tools/prolog-lint.sh` | ratcheted baseline | `library(crypto)` appears only under `labs/` today; `variant_sha1/2` is an autoloaded builtin and avoids the new production dependency |
| `cd v6/tsv2 && bash scripts/sweep.sh` | total=196 identical=195 rejection=1 | a new refusal lands in the `unsupported` bucket |

Node-backed gates cannot run in this worktree; `node_modules` is absent.
