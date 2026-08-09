# smell scan s3

Base: merged `9b2f8b0f` clean (ff-only, confirmed ancestor). Scope covered per
SMELL-BRIEF.md priority order: `lower.pl` (5047 lines), `emit_ts.pl` (2809),
`analyze.pl` (1742), `0_type_plane.pl` (899), `0_relation_edge_expand.pl` (92),
`compile.pl` (targeted read for `relplan`/`type_decl` construction). `strat.pl`
(114) read in full, clean. `0_enum_expand.pl`, `0_program_check.pl`,
`0_dot_expand.pl`, `0_match_expand.pl`, `0_coalesce_expand.pl`,
`1_expansion.pl`, `compile/parse_dl.pl`, `print_dl.pl` got a targeted grep pass
for the same themes (col_type/type_decl walks, type-atom dispatch), not a
full line-by-line read at this effort budget. Note: `strat.pl` and
`print_dl.pl` live at `v6/prolog/strat.pl` and `v6/prolog/print_dl.pl`, not
under `v6/prolog/compile/` as the brief's path suggested.

Verification method for the named target: ran the actual compiler under
`swipl` against two synthetic and one corpus fixture and printed the real
`relplan/5` and `Decls` terms it produces. Scripts left at
`/private/tmp/claude-501/.../scratchpad/inspect_relplan*.pl`, not in this repo.

## 1. Findings table

| rank | file | line span | predicates | merged form | lines removed |
| --- | --- | --- | --- | --- | --- |
| 1 | `lower.pl` | 865-895 (+775-784, 736-763 threading) | `catalog_column_type_id/7`, `declared_column_type/5`, `resolve_declared_column_type/7`, `catalog_list_types/4` | one `catalog_column_type_id/4` that switches on the `ColumnType` already sitting in the `relplan/5` it is called with, no `Decls`/`Types` re-derivation | ~20-24 lines, plus `Decls`/`Types` dropped from the parameter lists of `catalog_rows/5`, `catalog_rel_rows/9`, `catalog_column_rows/13` |
| 2 | `analyze.pl` | 795-803 | `merge_type/3` (ref/json/list triples) | `closed_type/1` fact list (3 facts) + 3 generic `merge_type/3` clauses keyed on `closed_type/1` instead of one triple per functor | 9 -> ~6 lines; real yield is a single list a new closed type must join, not three clause triples that can drift independently |
| 3 | `0_type_plane.pl` / `analyze.pl` / `emit_ts.pl` / `0_relation_edge_expand.pl` | `0_type_plane.pl:384-385`, `analyze.pl:297-306`, `emit_ts.pl:802-805`, `0_relation_edge_expand.pl:45-51` | `relation_column_types/4` (unexported), `rel_columns/4`, `declared_column_types/3`, `missing_head_target_atoms/5` | export `relation_column_types/4` from `0_type_plane.pl` and call it from the other three instead of re-walking `col_type(Ref, _, Type)` locally | ~5-8 lines; main yield is one definition instead of four, since three of the four sites were already single `findall/3` lines |

Two more clause-shaped observations, not costed because they involve genuinely
different output domains (see "what I checked and found clean" below):
`lower.pl` carries five separate type-atom dispatch tables (`column_def/3`,
`ir_column_storage/5`, `catalog_type_id/2`, `canonical_column_expr/3`,
`json_capture_json_type/2`) plus `emit_ts.pl:gate_column_type/2` and
`0_type_plane.pl:column_storage/3`/`column_value_shape_error/4`. All map a
type atom to something, but each maps to a *different* target (DDL text,
dict-vs-direct encoding, a catalog integer id, a quoted SQL expression, a JS
boundary word, a JSON1 affinity, a refusal reason). None of these pairs
produce the same output for the same input, so they are not findings.

## 2. The type_decl / relplan / type_def question

**Which fields does each shape hold that the others do not?**

- `type_decl(TypeName, [col(Column, Type), ...])` (`0_type_plane.pl:16`,
  built by `compile.pl:127-134` from a `rel` declaration reached in *column*
  position) only exists for types that are referenced as a column's type
  elsewhere. Most rels in a program have none.
- `type_def(Name, Columns, ColumnTypes)` (`0_type_plane.pl:62-67`) is not a
  second storage: `type_definitions/2` computes it fresh from `type_decl`
  every time it is called, by unzipping `col(Column, Type)` into two parallel
  lists. It carries **no field `type_decl` lacks**.
- `relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes)`
  (`compile.pl:213-219`) exists for **every** `Ref` in the program
  (`AllRefs` = rule-derived + declared + seeded, `compile.pl:190-193`), not
  just struct-typed ones. It carries `Kind` (log/set) and `KeyOrNone`, which
  `type_decl`/`type_def` have no slot for at all, and its `ColumnTypes` is
  the compiled **storage kind** (via `column_storage/3`), not the source
  spelling.

**Where does relplan's ColumnTypes hold a COLLAPSED type while type_decl
holds the declared one?**

Verified empirically, not by reading a comment. Compiling
`col_type(batch/2, id, int), col_type(batch/2, payloads, list(json))`
produces:

```
relplan(batch/2, set, [id,payloads], none, [int, list(json)])
```

`list(json)` survives intact into `relplan`'s `ColumnTypes` -- it is **not**
collapsed to bare `json`. `column_storage/3` (`0_type_plane.pl:115-122`) has
its own `list(Element)` clause and `lower.pl:column_def/3` has its own
`list(_)` DDL clause (`lower.pl:1371-1374`), so the migration the
`0_type_plane.pl:89-114` header comment describes as in-progress
("today the storage kind collapses to json") appears to already be done for
the general `column_storage/3` path.

The claim IS live in one specific place: `lower.pl:865-866`'s comment
("a list column's resolved type has already collapsed to json") and the
`catalog_column_type_id/7` -> `declared_column_type/5` ->
`resolve_declared_column_type/7` chain built on it (`lower.pl:864-895`) still
route around a collapse that, per the test above, does not happen anymore.
That chain re-fetches `DeclaredType` from `Decls` specifically so it can
dispatch on `list(Element)`/`ref(Name)` shapes it assumes `ColumnType`
(relplan's own value, already in hand as the predicate's own argument) no
longer has. A second test on a struct column confirms the one place they
*do* differ: a `holder(item: span)` declaration keeps `item`'s declared type
as the bare atom `span` in `Decls`/`type_def`, while `relplan`'s storage
`ColumnTypes` holds `ref(span)` for the same column -- a strictly *more*
informative wrapping of the same name, not a lossy collapse.

**Sites that have to consult both because of it**: the call chain
`catalog_rows/5` (`lower.pl:747-763`) -> `catalog_list_types/4`
(`lower.pl:775-784`, discards `relplan`'s own `ColumnTypes` at line 777 with
`_` and re-derives an equal value from `Decls` at line 778) ->
`catalog_rel_rows/9` (`lower.pl:830-846`) -> `catalog_column_rows/13`
(`lower.pl:848-863`) -> `catalog_column_type_id/7` (`lower.pl:867-873`) ->
`declared_column_type/5` + `resolve_declared_column_type/7`
(`lower.pl:892-895`, `875-884`). Seven predicate definitions across
`lower.pl:747-895` thread `Decls` and `Types` for this one reason.

**Smallest merged record that serves every current reader**: not a merge of
`type_decl`/`type_def`/`relplan` into one shape -- the domains are too
different (102 vs 113 sites, and only a minority of `relplan` rows reference
a struct type at all; in the `holder/span` test above, 1 of 3 `relplan` rows
did). The smallest real fix is narrower: compute `type_definitions(Decls,
Types)` **once**, in `compile.pl:program_plan/3` where `RelPlans` is already
built (`compile.pl:213-219`), and thread `Types` on `Plan` alongside
`RelPlans` instead of recomputing it. `type_definitions/2` is called 22 times
across the in-scope files on the same `Decls` (list: `0_type_plane.pl:334,
343, 464`; `lower.pl:750, 1464, 4923, 5009`; `0_relation_edge_expand.pl:31`;
`0_program_check.pl:225, 331, 343, 367, 410, 460, 505, 517, 715, 778`;
`sweep.pl:132`; `analyze.pl:389, 1553`), each paying a fresh `findall/3` over
the same list for the same answer. Separately, `catalog_column_type_id/7`
should dispatch on the `ColumnType` it is already passed instead of
re-deriving `DeclaredType` from `Decls` (finding 1 above).

**How many of the 113 + 102 sites would actually have to change?** Zero, if
"merge type_decl/relplan into one record" is the fix -- that fix is wrong per
the falsification above, so it costs nothing to not do it. The real fix
(thread `Types` once, simplify `catalog_column_type_id`) touches on the order
of 10-15 sites: the 22 `type_definitions/2` call sites (most collapse to
reading `Plan`'s new field instead of recomputing, a handful can't because
they run before a `Plan` exists -- e.g. `0_program_check.pl`'s pre-plan
checks), plus the 7 `lower.pl:747-895` predicates in finding 1. This is
`UNVERIFIED` as an exact count: I did not trace which of the 22 call sites
run before vs. after `program_plan/3` executes; that trace is what I would
run next (`grep -n` each call site's enclosing predicate against
`compile.pl`'s call order) before touching code.

## 3. The top three

### Rank 1 -- `lower.pl:865-895`, catalog type-id resolution

Current:

```prolog
% The declared type is the authority for ref and list resolution, since a list
% column's resolved type has already collapsed to json.
catalog_column_type_id(Decls, Types, Ref, ColumnName, ColumnType,
                       RelIdMap, ListIdMap, TypeId) :-
    (   declared_column_type(Decls, Types, Ref, ColumnName, DeclaredType)
    ->  resolve_declared_column_type(Decls, Types, DeclaredType, ColumnType,
                                     RelIdMap, ListIdMap, TypeId)
    ;   catalog_type_id(ColumnType, TypeId)
    ).

resolve_declared_column_type(_, _, list(Element), _, _RelIdMap, ListIdMap, TypeId) :- !,
    list_row_id(ListIdMap, list(Element), TypeId).
resolve_declared_column_type(_, _, DeclaredType, _, _RelIdMap, _ListIdMap, TypeId) :-
    primitive_type(DeclaredType), !,
    catalog_type_id(DeclaredType, TypeId).
resolve_declared_column_type(_, Types, DeclaredType, _, RelIdMap, _ListIdMap, TypeId) :-
    column_storage(Types, DeclaredType, ref(Name)), !,
    rel_row_id(RelIdMap, Name, TypeId).
resolve_declared_column_type(_, _, DeclaredType, _, _RelIdMap, _ListIdMap, TypeId) :-
    catalog_type_id(DeclaredType, TypeId).

declared_column_type(Decls, Types, Ref, ColumnName, DeclaredType) :-
    relation_columns_and_types(Decls, Types, Ref, DeclaredColumns, DeclaredColumnTypes),
    nth1(Position, DeclaredColumns, ColumnName),
    nth1(Position, DeclaredColumnTypes, DeclaredType).
```

Proposed (verified `ColumnType` already carries `list(Element)`/`ref(Name)`
intact, so no `Decls`/`Types` lookup is needed):

```prolog
catalog_column_type_id(ColumnType, RelIdMap, ListIdMap, TypeId) :-
    ColumnType = list(_), !,
    list_row_id(ListIdMap, ColumnType, TypeId).
catalog_column_type_id(ref(Name), RelIdMap, _ListIdMap, TypeId) :- !,
    rel_row_id(RelIdMap, Name, TypeId).
catalog_column_type_id(ColumnType, _RelIdMap, _ListIdMap, TypeId) :-
    catalog_type_id(ColumnType, TypeId).
```

`declared_column_type/5` and `resolve_declared_column_type/7` delete
entirely (14 lines). `catalog_column_type_id/7` shrinks to `/4` and 4 lines,
saving ~5 more. `Decls`/`Types` drop from the two callers
(`catalog_rel_rows/9`, `catalog_column_rows/13`) and their recursive-call
argument lists.

### Rank 2 -- `analyze.pl:795-803`, merge_type triples

Current:

```prolog
merge_type(ref(Type), none, ref(Type)) :- !.
merge_type(none, ref(Type), ref(Type)) :- !.
merge_type(ref(Type), ref(Type), ref(Type)) :- !.
merge_type(json, none, json) :- !.
merge_type(none, json, json) :- !.
merge_type(json, json, json) :- !.
merge_type(list(X), none, list(X)) :- !.
merge_type(none, list(X), list(X)) :- !.
merge_type(list(X), list(X), list(X)) :- !.
```

Proposed:

```prolog
closed_type(ref(_)).
closed_type(json).
closed_type(list(_)).

merge_type(Type, none, Type) :- closed_type(Type), !.
merge_type(none, Type, Type) :- closed_type(Type), !.
merge_type(Type, Type, Type) :- closed_type(Type), !.
```

9 lines to 6. The `merge_type/3` clauses that follow (`analyze.pl:807-818`,
`text`/`float`/`bool`/`int` widening) are genuinely different per-pair rules,
not triples, and are not part of this finding.

### Rank 3 -- the col_type(Ref, _, _) walk family

Current (four independent walks of the same `Decls` shape):

```prolog
% 0_type_plane.pl:384-385 (unexported)
relation_column_types(Decls, _, Ref, ColumnTypes) :-
    findall(Type, member(col_type(Ref, _, Type), Decls), ColumnTypes).

% emit_ts.pl:802-805
declared_column_types(Decls, Ref, Types) :-
    Ref = _/Arity,
    findall(Type, member(col_type(Ref, _, Type), Decls), Types),
    length(Types, Arity).

% 0_relation_edge_expand.pl:48 (inline, inside missing_head_target_atoms/5)
findall(Type, member(col_type(Ref, _, Type), Decls), ColumnTypes),
```

Proposed: export `relation_column_types/4` from `0_type_plane.pl`'s module
list and call it from the other two sites (`declared_column_types/3` becomes
a one-line wrapper kept only for its arity-check postcondition,
`0_relation_edge_expand.pl:48` becomes a call). `analyze.pl:297-306`'s
`rel_columns/4` keeps its own body (only 3 of its 10 lines, 300-302, are the
duplicate walk; the rest is a genuinely different inferred-vs-declared
fallback) but could route those 3 lines through the same call too.

## 4. What I checked and found clean

- `lower.pl:column_def/3` (1354-1392): one clause per storage kind
  (int/bool/float/ref/list/json/interned/text), each emitting distinct DDL
  text with a distinct CHECK constraint. No overlap with the other four
  type-atom dispatch tables in the same file -- each answers a different
  question (DDL text vs. dict encoding vs. catalog id vs. quoted SQL
  expression vs. JSON1 affinity).
- `print_dl.pl:395-400`: `print_body_item(cst(Path,Digest,Language,Query,_),
  ...)` and the 4-arity `cst(Path,Digest,Language,Query)` clause both cut
  straight into a shared `print_cst_body/6` (`print_dl.pl:407-412`). This is
  the pattern rank 3 is missing elsewhere in the scope: two surface arities,
  one body.
- `strat.pl` (114 lines, read in full): `stratum_groups/2` and
  `sql_rule_order/2` are two genuinely different algorithms (strata
  relaxation vs. per-stratum Kahn topological sort) sharing no duplicated
  clause shape.
- `0_type_plane.pl:list_element_type/2` (135-140): a flat five-fact closed
  set with one recursive case for nested lists. No sibling table repeats it.
- `0_type_plane.pl:column_storage/3` and `analyze.pl:column_type_at_decl/6`
  look like a pair worth merging at first read (both answer "what does this
  declared type store as") but are correctly split: `column_storage/3` is
  the pure type-level function (`Decl type -> Storage kind`), and
  `column_type_at_decl/6` is what layers the literal-witness cross-check and
  the `float`/`int` affinity-widening ruling on top of it, for one column of
  one rel. Collapsing them would put program-specific witness data inside a
  predicate the type plane's other 20+ callers use with no witnesses in
  scope.

## 5. The two d2 numbers

```
viewBox="0 0 4248 775"
```

Shape count per the brief's grep (includes attribute/style lines, not just
shapes): **30**. Actual shape count as designed and compiled (nodes +
containers, counted before compiling): **20** -- `agent: s3` (1) + finding 1
container and its 4 children (5) + finding 2 (5) + finding 3 (6, container +
4 before + 1 after) + finding 4 verdict container and its 2 children (3).
Under the 24 max.
