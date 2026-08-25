:- begin_tests(canonical_storage_projection).

:- use_module('../../compile', [program_plan/2]).
:- use_module('../parse_dl_dcg', [parse_dl/4]).
:- use_module('../../lower', [catalog_type_rows/6]).
:- use_module('../0_storage_projection', [storage_rows_from_decls/2]).
:- use_module('../../0_rel_record', [relplan_parts/6]).

:- op(1150, xfx, <-).

storage_plan(Source, Plan) :-
    string_codes(Source, Codes),
    once(parse_dl(Codes, Program, Bindings, [])),
    once(program_plan(fixture(storage_projection, Program, [], [], [])-
                      Bindings, Plan)).

test(canonical_ids_key_declared_physical_rows) :-
    Source = "rel pair(T)(first: T, second: T).\n\c
              rel status(ready(); failed()).\n\c
              rel address(city: text).\n\c
              rel holder(value: (a: int, b: text)).\n\c
              rel item(id: key(int), home: address, maybe: option(text), pair_value: pair(int), tags: list(text), state: status).\n",
    storage_plan(Source, plan(_, prog(Decls, _), _, _, _, _, _, _, _)),
    memberchk(semantic_type_rows(SemanticRows), Decls),
    storage_rows_from_decls(Decls, StorageRows),
    Item = named(local, relation, item),
    Address = named(local, relation, address),
    Pair = named(local, relation, pair),
    PairInt = application(Pair, [primitive(int)]),
    memberchk(derived_from(PairStorage, PairInt), SemanticRows),
    memberchk(storage_column(member(Item, 1, id), primitive(int)),
              StorageRows),
    memberchk(storage_column(member(Item, 2, home), reference(Address)),
              StorageRows),
    memberchk(storage_column(member(Item, 3, maybe), primitive(int)),
              StorageRows),
    memberchk(storage_column(member(Item, 4, pair_value),
                             reference(PairStorage)), StorageRows),
    memberchk(storage_column(member(Item, 5, tags), list(primitive(text))),
              StorageRows),
    memberchk(storage_column(member(Item, 6, state), primitive(int)),
              StorageRows),
    memberchk(storage_key(Item, member(Item, 1, id)), StorageRows),
    memberchk(storage_column(member(named(local, relation, holder), 1, value),
                             reference(Anonymous)), StorageRows),
    Anonymous = named(local, relation, AnonymousName),
    sub_atom(AnonymousName, 0, _, _, '__anon_holder_value_'),
    forall(member(storage_relation(Owner, _, _), StorageRows),
           memberchk(declaration(Owner, _, _, relation, materialized),
                     SemanticRows)),
    forall(member(storage_column(MemberId, _), StorageRows),
           memberchk(member(MemberId, _, _, _, _), SemanticRows)).

test(undeclared_idb_stays_on_the_relplan_compatibility_path) :-
    Source = "rel seed(value: int).\n\c
              derived(Value) <- seed(Value).\n",
    storage_plan(Source,
                 plan(_, prog(Decls, _), _, RelPlans, _, _, _, _, _)),
    once(( member(RelPlan, RelPlans),
           relplan_parts(RelPlan, derived/1, _, _, _, _) )),
    storage_rows_from_decls(Decls, StorageRows),
    \+ member(storage_relation(named(_, relation, derived), _, _), StorageRows).

test(catalog_type_rows_read_canonical_physical_rows) :-
    Source = "rel item(id: key(int), name: text).\n",
    storage_plan(Source,
                 plan(Name, prog(Decls, Rules), _, RelPlans, _, _, _, _, Mode)),
    catalog_type_rows(Mode, Name, Rules, RelPlans, Decls, Expected),
    maplist(sabotage_item_relplan, RelPlans, SabotagedRelPlans),
    catalog_type_rows(Mode, Name, Rules, SabotagedRelPlans, Decls, Actual),
    Actual == Expected.

sabotage_item_relplan(RelPlan0,
                      rel(item/2, ignored_table, log,
                          [col(wrong_a, inferred, json),
                           col(wrong_b, inferred, bytes)], none)) :-
    relplan_parts(RelPlan0, item/2, _, _, _, _),
    !.
sabotage_item_relplan(RelPlan, RelPlan).

:- end_tests(canonical_storage_projection).
