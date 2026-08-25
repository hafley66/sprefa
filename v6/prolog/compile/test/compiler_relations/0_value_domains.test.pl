:- begin_tests(compiler_relation_value_domains).

:- use_module('../../../0_compiler_relations',
              [ partition_compiler_relations/3 ]).
:- use_module('../../../1_expansion/0_generic_expand',
              [ expand_generic_program_with_bindings/3 ]).
:- use_module('../../../1_expansion/1_expansion',
              [ expand_program_with_bindings/4 ]).
:- use_module('../../parse_dl_dcg', [parse_dl/4]).

parse_value_domain_source(Source, Program, Bindings) :-
    string_codes(Source, Codes),
    once(parse_dl(Codes, Program, Bindings, [])).

expand_value_domain_source(Source, Expanded) :-
    parse_value_domain_source(Source, Program, Bindings),
    once(expand_generic_program_with_bindings(Program, Bindings, Expanded)).

test(enum_values_are_elaborated_inside_compiler_relations) :-
    Source = "rel policy(all(); count(value: int)).\n\c
              rel mark(Target: type, Policy: policy).\n\c
              rel Item(id: int).\n\c
              mark(Item, count(2)).\n",
    expand_value_domain_source(Source, prog(Decls, [])),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    memberchk(mark(named(local, relation, 'Item'), count(2)), Closure),
    \+ member(col_type(mark/2, _, _), Decls),
    \+ member(enum_decl(policy, _), Decls).

test(enum_value_shape_is_checked,
     [throws(unsupported_construct(
                 compiler_relation_argument_type(policy, forever)))]) :-
    Source = "rel policy(all(); count(value: int)).\n\c
              rel mark(Target: type, Policy: policy).\n\c
              rel Item(id: int).\n\c
              mark(Item, forever()).\n",
    expand_value_domain_source(Source, _).

test(enum_domain_shared_with_runtime_storage_is_retained) :-
    Decls = [ enum_decl(policy, (all ; count(value:int))),
              col_type(mark/2, target, type),
              col_type(mark/2, policy, policy),
              col_type(config/1, policy, policy) ],
    partition_compiler_relations(
        Decls,
        compiler_relations([compiler_relation(mark/2, 2, [])], []),
        RuntimeDecls),
    memberchk(enum_decl(policy, (all ; count(value:int))), RuntimeDecls),
    memberchk(col_type(config/1, policy, policy), RuntimeDecls).

test(nested_compiler_enum_erasure_retains_runtime_shared_inner_domain) :-
    Decls = [ enum_decl(inner, (left ; right)),
              enum_decl(outer, wrap(value:inner)),
              col_type(mark/2, target, type),
              col_type(mark/2, policy, outer),
              col_type(config/1, policy, inner) ],
    partition_compiler_relations(
        Decls,
        compiler_relations([compiler_relation(mark/2, 2, [])], []),
        RuntimeDecls),
    \+ member(enum_decl(outer, _), RuntimeDecls),
    memberchk(enum_decl(inner, (left ; right)), RuntimeDecls),
    memberchk(col_type(config/1, policy, inner), RuntimeDecls).

test(compiler_only_enum_does_not_materialize_runtime_variant_relations) :-
    Source = "rel policy(all(); count(value: int)).\n\c
              rel mark(Target: type, Policy: policy).\n\c
              rel Item(id: int).\n\c
              mark(Item, all()).\n",
    parse_value_domain_source(Source, Program, Bindings),
    once(expand_program_with_bindings(Program, Bindings, prog(Decls, _), _)),
    \+ member(col_type(policy_all/_, _, _), Decls),
    \+ member(col_type(policy_count/_, _, _), Decls),
    \+ member(kind(policy_all/_, _), Decls),
    \+ member(kind(policy_count/_, _), Decls).

:- end_tests(compiler_relation_value_domains).
