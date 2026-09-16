# Branch B, create-on-write

Lane note: this lane ran in an enforced read-only harness with no Write tool, so
it returned its three deliverables inline and the coordinator transcribed them
into `PLAN.md`, `PLAN.visual.human.unga.md` and `COST.md` verbatim. Nothing in
the worktree was changed by the lane itself.

| section | answers |
|---|---|
| 1 | what a dotted head does today, with run output |
| 2 | every site branch B touches |
| 3 | signatures, bodies, storage, sequence, uniqueness |
| 4 | where the four layers disagree |
| 5 | fail-first, sabotage, existing gates |
| 6 | where the spec was wrong |

## 1. What a dotted head does today

Three probes, run against the worktree modules.

| probe | input | result |
|---|---|---|
| text door | `a.b(X) <- c(X).` | `dl_parse_error(statement, position(1,2))` |
| text door, body | `d(X) <- c(X), e(a.b).` | parses to `e(dot_get(_G, b))`, `Bindings = ['X'=_, a=_G]` |
| that program, phase 44 | | `unsupported_construct(unresolvable_member(b))` |
| term door | head `dot_get(a,b)` | passes phase 44 unchanged, `program_plan` succeeds, emits `CREATE TABLE "dot_get" ("col1" TEXT NOT NULL, "col2" TEXT NOT NULL, "__refcount" ...)` |
| printer | that same rule | prints `'a'.b <- c(_).`, which reparses as `dl_parse_error(statement, position(1,1))` |

A dotted head is a parse failure at the text door and a silent miscompile at the
term door. It is refused nowhere.

Cause of the parse failure: `v6/prolog/compile/parse_dl.pl:1062` `head_atom//5`
is `ident(Name), ws0(...), lit_dcg("(")`. `dot_chain//4` at `:1689` is reachable
only from `compound_or_var//5` at `:1679`, an expr-position route.

## 2. Sites branch B changes

| site | today | branch B |
|---|---|---|
| `parse_dl.pl:1062` `head_atom//5` | ident then `(` | add a segment loop before `(`, producing `rel_path(Segments, PositionalArgs)`; zero dots keeps the shipped bare compound byte for byte |
| `parse_dl.pl:1679` `compound_or_var//5` | interns lowercase root as a variable via `get_or_make_var` | a path in body position must keep the root NAME, so it emits `rel_path/2` too |
| `0_dot_expand.pl:90` `rewrite_head/4` | `Head0 =.. [Name\|Args0]`, walks args only | untouched, because phase 43 removes every path before phase 44 runs |
| `0_dot_expand.pl:171,176` | var root required, else `unresolvable_member` | untouched for member access; a path is a different term now |
| `0_dot_expand.pl:183` `dot_path_atom/2` | atom-root arm is dead at the text door, so the payload drops the root | stays dead, and the comment at `:179-182` needs correcting |
| `compile.pl:157` `sort(AllRefs0, AllRefs)` | ref order fixes catalog ids | mangled names enter the sort; ids shift whenever any file adds a path |
| `compile.pl:193` `memberchk(Ref-ColumnTypes, RefTypes)` | drops refs `program_column_types/7` has no entry for | module rel/0 rows are minted outside `RelPlans`, since this drop is why they vanish |
| `compile.pl:175` `subtract(AllRefs, [Catalog\|Derived], ArrivalTargets)` | catalog excluded | module rel/0 refs excluded too, they have no table to write |
| `lower.pl:162` `table_name(Name/_Arity, Name)` | arity dropped | arity folded into the path digest, so paths are immune; root rels keep the shipped hazard |
| `lower.pl:643` `catalog_row_ddl/3` | every `parent_id` is 0 | child rel rows carry `parent_id` = their module row id |
| `lower.pl:637` `catalog_table_ddl/1` | index `(parent_id, local_name)` | already correct for the child walk, no change |
| `analyze.pl:190` `program_uses_catalog/2` | arity-6 mention gate | widen to "program has any path head", else a module program emits no catalog |
| `print_dl.pl:493,519` | prints `dot_get` chains in term position | add a path-head arm, or `roundtrip` breaks |
| refusal set | `grep -rn "unresolvable_path\|module_name_collision\|container_and_leaf" --include=*.pl --include=*.ts v6/` returns nothing | branch B never mints `unresolvable_path` for a HEAD; it keeps it for a body path that names nothing |
| `v6/tsv2/serve/0_compile.ts:98` | `compile(source: string)`, one source string | multi-file compile does not exist, so branch B is intra-file until that widens |

Two supporting measurements from the probes:

- `relation_kind(_, _, set)` at `v6/prolog/0_program_check.pl:56` is the default
  for an undeclared head. Create-on-write therefore needs no new kind inference;
  the compiler already mints tables from bare rule heads. That is the cheapest
  part of branch B.
- `kind(a/0, set)` in `Decls` yields `declared_refs = [a/0]` but
  `program_column_types(...)` returns no entry for `a/0`, and the resulting seed
  jumps `12 -> 13='a__b'`. An arity-0 rel emits no table and no catalog row today.

## 3. The design

```prolog
%! path_head(-HeadTerm, +Vars0, -Vars)// is semidet.      parse_dl.pl, beside head_atom//5
%  ident(First), then zero or more (dot_then_ident, ident) segments, then '(' head_args ')'.
%  Zero segments  -> Term =.. [First | PositionalArgs], the shipped shape, byte-identical.
%  One or more    -> HeadTerm = rel_path([First | Segments], PositionalArgs).
%  Reuses dot_then_ident//2 at :1697, so the glued-dot and terminator-dot rules stay one rule.

%! path_flat_name(+Segments:list(atom), +Arity:integer, -FlatName:atom) is det.
%  atomic_list_concat(Segments, '__', Stem),
%  format(atom(CanonicalText), '~w/~d', [DottedPath, Arity]),
%  crypto_data_hash(CanonicalText, Sha, [algorithm(sha256), encoding(utf8)]),
%  sub_atom(Sha, 0, 8, _, Digest),
%  atomic_list_concat([Stem, Digest], '__', FlatName).      % a__b__c__<digest>, M5 spelling
%  ARITY IS INSIDE THE DIGEST. That is the whole repair for lower.pl:162.

%! path_expand_in_context(+Context, +prog(Decls0,Rules0), -prog(Decls,Rules)) is det.
%  expansion_phase(43, path, ...), AFTER seq(42), BEFORE dot(44).
%  For every rule head that is rel_path(Segments, Args):
%     path_flat_name(Segments, Arity, FlatName), Head =.. [FlatName | Args],
%     for every proper prefix Prefix of Segments, add module_edge(Prefix, Segments, FlatName)
%     to Decls if absent.                                    % the CREATE half of create-on-write
%  For every BODY atom that is rel_path(Segments, Args): mangle identically, add NO module_edge.
%  A body path reads; a head path creates. One sentence, one branch.
%  A rule carrying no rel_path is returned byte-identical, the 0_dot_expand.pl:33 discipline.

%! module_catalog_rows(+Decls, +StartId, -NextId, -Rows) is det.      lower.pl, beside catalog_rel_rows/4
%  collect every distinct path prefix from module_edge/3, sort by segment list,
%  fold the SAME positional counter over them:
%     row(Id, ParentId, 0, LocalSegment, rel, 0), ParentId = the enclosing prefix's Id, 0 at root.
%  Emitted BEFORE catalog_rel_rows/4 so a child's parent_id is already assigned when the child row lands.
```

**Storage layout.** Zero new tables. `__catalog_rel(rel_id, parent_id, ordinal,
local_name, kind, type_id)` at `lower.pl:630` already carries `parent_id`, and
`catalog_table_ddl/1` at `:637` already indexes `(parent_id, local_name)`. A
module is a row with `kind='rel'`, `ordinal=0`, `type_id=0`, and no table, which
matches the measured behavior of arity-0 rels. A rel's `parent_id` space holds
both its columns and its child rels; the `kind` column separates them, which is
exactly module-catalog rule 7's one-namespace-per-parent.

**Sequence of reads and writes.**

| step | reads | writes |
|---|---|---|
| 1 parse | source text | `rel_path/2` terms, root name retained |
| 2 phase 43 | `prog(Decls0, Rules0)` | flat mangled heads plus `module_edge/3` decls |
| 3 phases 44-50 | flat rels only | unchanged, so every dots-land fixture stays byte-identical |
| 4 `program_plan/2` | `AllRefs` over mangled names | `RelPlans`, module refs absent by the `RefTypes` drop |
| 5 `catalog_row_ddl/3` | `module_edge/3` decls, then `RelPlans` | primitives 1..5, catalog 6..12, module rows, rel and column rows |
| 6 `rel_ddl/6` | `RelPlans` | one `CREATE TABLE` per mangled child, zero for module rows |

**Uniqueness conditions.**

| id | condition | why it holds |
|---|---|---|
| U1 | identity is the full segment list | `a.b` and `c.b` mangle to different stems and different digests |
| U2 | `a.b/1` and `a.b/2` are different flat names | arity sits inside the digest text |
| U3 | two rules with the same path head union with no new machinery | after phase 43 they are literally the same functor and arity, the shipped multi-clause route |
| U4 | a prefix that is already a leaf rel with columns is refused as `container_and_leaf` | one row cannot be both a table and a parent of rel children |
| U5 | module rows dedupe by the full six-column primary key | `catalog_table_shape` pins `PRIMARY KEY ("rel_id", "parent_id", ...)`, so a row with the same path but a different positional `rel_id` INSERTS rather than IGNOREs |

## 4. Where the layers disagree

- The signature layer makes `path_flat_name/3` content-stable. The storage layer
  assigns `rel_id` positionally. Those two disagree. Decision: the mangled NAME is
  the stable identity and `rel_id` is a per-compile handle, so any cross-program
  read joins on `local_name` and never on `rel_id`.
- The sequence layer interleaves module rows into a counter that
  `catalog_rel_rows/4` currently folds over one list. Threading it through two
  lists shifts every id after 12, which breaks `catalog_ids_are_positional`.
  Decision: let it break, because that test is the id-shift alarm.
- U5 wants digest-derived ids, which contradicts `catalog_ids_are_positional`
  outright. Positional stays in v1 and U5 stays a known open collision.

## 5. The proof

**Fail-first test.** In `v6/prolog/compile/test/plunit_tests.pl`, a new group
`module_path_g1`:

```prolog
test(path_head_parses) :-
    string_codes("a.b(X) <- c(X).\n", Codes),
    parse_dl(Codes, prog([], [(Head <- _)]), _, []),
    Head = rel_path([a,b], [_]).
```

Verified failing today with `dl_parse_error(statement, position(1,2))`.

**Sabotage receipt.** Drop arity from the digest text in `path_flat_name/3`, then
compile a program holding `a.b(X) <- s(X).` and `a.b(X,Y) <- s(X), s(Y).`. The
sabotaged build emits two `CREATE TABLE "a__b__<digest>"` with different column
lists, and `isAlreadyExists` at `v6/tsv2/serve/3_engine.ts:224` swallows the
second, so rows land in the wrong shape with no error. The same shape is already
demonstrable on root rels: `prog([], [(edge(X) <- source_row(X)), (edge(X,X) <-
source_row(X))])` compiles clean and emits `CREATE TABLE "edge"` twice, once with
`col1`, once with `col1, col2`.

**Existing gates, exact commands.**

| command | catches |
|---|---|
| `just -f v6/justfile roundtrip` | a path head the printer cannot re-emit. Verified live today: `'a'.b <- c(_).` reparses as `dl_parse_error(statement, position(1,1))` |
| `just -f v6/justfile text-door` | term door and text door diverging, 196/196 byte-identical |
| `just -f v6/justfile plunit` | `catalog_ids_are_positional` pins `"(13,0,0,'rel_named','rel',0)"`, so any id shift fails loudly |
| `just -f v6/justfile conformance` | 281 PASS over the fixture corpus |
| `just -f v6/justfile prolog-lint` | ratcheted organization gate over the new phase module |

## 6. Where the spec was wrong

- The anchor for `0_dot_expand.pl:169,176` says an ATOM root is refused by
  construction. Atoms are indeed refused, and the text door never produces one:
  `compound_or_var//5` calls `get_or_make_var`, so `e(a.b)` yields
  `dot_get(_G, b)` with the name recoverable only from `Bindings`. The refusal
  payload is `unresolvable_member(b)`, with the root dropped. The comment at
  `0_dot_expand.pl:179-182` describing an atom root spelling the whole path is
  unreachable from the text door.
- The spec calls the dotted head "refused". It is a PARSE ERROR at the text door
  and an unrefused silent miscompile at the term door, producing
  `CREATE TABLE "dot_get"`.
- `plans/2026-08-03-module-catalog-ruling.md` does not exist in this worktree.
  The matching documents are `plans/2026-08-03-modscope-plan.md` (rule 8 at its
  section 1.4, phasing step 5 at `:481`) and `plans/2026-08-03-modscope-rework.md`.
  `v6/prolog/conformance/rulings.pl:608` is `block_lowering_first`; `:613` is
  `catalog_universe`.
