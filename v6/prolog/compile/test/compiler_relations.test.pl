:- begin_tests(compiler_relations).

:- use_module('../../0_compiler_relations',
              [ partition_compiler_relations/3,
                partition_compiler_program/5,
                evaluate_compiler_relations/3 ]).
:- use_module('../../0_generic_expand', [ expand_generic_program_with_bindings/3,
                                          type_relation_rows/2 ]).
:- use_module('../../1_expansion', [expand_program/3]).
:- use_module('../../compile', [program_plan/2, compile_dl6/2]).
:- use_module('../parse_dl_dcg', [parse_dl/4]).

:- op(1150, xfx, <-).

compiler_decls([
    col_type(codec/2, self, type),
    col_type(codec/2, format, type),
    keyed(codec/2, [1]),
    col_type(runtime/1, value, text)
]).

test(partition_erases_compiler_declarations_from_runtime) :-
    compiler_decls(Decls),
    partition_compiler_relations(Decls, compiler_relations(Relations, []), Runtime),
    Relations = [compiler_relation(codec/2, 2, [1])],
    Runtime == [col_type(runtime/1, value, text)].

test(mixed_value_domains_are_refused,
     [throws(unsupported_construct(compiler_relation_mixed_domain(codec/2)))]) :-
    partition_compiler_relations(
        [col_type(codec/2, self, type), col_type(codec/2, label, text)], _, _).

test(recursive_positive_rules_reach_a_set_fixpoint) :-
    Decls = [ compiler_relation(parent/2, 2, []),
              compiler_relation(ancestor/2, 2, []) ],
    Rules = [ ancestor(X, Y) <- parent(X, Y),
              ancestor(X, Z) <- (parent(X, Y), ancestor(Y, Z)) ],
    Seeds = [parent(a, b), parent(b, c), parent(a, b)],
    evaluate_compiler_relations(compiler_relations(Decls, Rules), Seeds, Closure),
    Closure == [ancestor(a, b), ancestor(a, c), ancestor(b, c),
                parent(a, b), parent(b, c)].

test(keyed_functional_conflict_is_refused,
     [throws(unsupported_construct(
         compiler_relation_functional_conflict(codec/2, [named(local, relation, document)])))]) :-
    Relations = [compiler_relation(codec/2, 2, [1])],
    evaluate_compiler_relations(
        compiler_relations(Relations, []),
        [ codec(named(local, relation, document), primitive(json)),
          codec(named(local, relation, document), primitive(text)) ], _).

test(named_negation_is_refused,
     [throws(unsupported_construct(compiler_relation_negation_unsupported(codec/2)))]) :-
    Decls = [col_type(codec/2, self, type), col_type(codec/2, format, type)],
    partition_compiler_program(Decls,
                               [codec(X, Y) <- not(codec(X, Y))],
                               _, _, _).

test(runtime_rule_with_compiler_ref_under_negation_is_refused,
     [throws(unsupported_construct(compiler_relation_negation_unsupported(codec/2)))]) :-
    Decls = [col_type(codec/2, self, type), col_type(codec/2, format, type),
             col_type(runtime/1, value, text)],
    partition_compiler_program(Decls,
                               [runtime(X) <- wrapper(not(codec(X, text)))],
                               _, _, _).

test(unsafe_compiler_rule_is_refused,
     [throws(unsupported_construct(compiler_relation_unsafe_rule(codec/2)))]) :-
    Relations = [compiler_relation(codec/2, 2, [])],
    evaluate_compiler_relations(
        compiler_relations(Relations, [codec(X, _Format) <- codec(X, text)]), [], _).

test(bare_compiler_fact_reaches_closure) :-
    Program = prog(
        [ col_type(document/1, id, int),
          col_type(capability/2, self, type),
          col_type(capability/2, format, type) ],
        [capability(document, text)]),
    expand_generic_program_with_bindings(Program, [], prog(Decls, Rules)),
    Rules == [],
    member(compiler_type_metadata(_, Closure), Decls),
    Closure == [capability(named(local, relation, document), primitive(text))].

test(real_dl6_type_terms_elaborate_and_erase_before_runtime) :-
    string_codes(
        "rel Document(id: int).\nrel capability(Self: type, Format: type).\ncapability(Document, text).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, Rules)),
    Rules == [],
    \+ member(col_type(capability/2, _, _), Decls),
    member(compiler_type_metadata(_, Closure), Decls),
    Closure == [capability(named(local, relation, 'Document'), primitive(text))],
    type_relation_rows(Decls, Rows),
    member(type_relation(named(local, relation, capability), _, _, none, []), Rows).

test(compiler_and_oracle_expansion_share_compiler_closure) :-
    Program = prog(
        [ col_type(document/1, id, int),
          col_type(capability/2, self, type),
          col_type(capability/2, format, type) ],
        [capability(document, text) <- true]),
    expand_program(Program, prog(OracleDecls, _), _),
    member(compiler_type_metadata(_, OracleClosure), OracleDecls),
    program_plan(fixture(compiler_plane, Program, [], [], [])-[],
                 plan(_, prog(CompilerDecls, _), _, _, _, _, _, _, _)),
    member(compiler_type_metadata(_, CompilerClosure), CompilerDecls),
    CompilerClosure == OracleClosure.

test(authored_dl6_bare_fact_has_compiler_oracle_parity) :-
    string_codes(
        "rel Document(id: int).\nrel capability(Self: type, Format: type).\ncapability(Document, text).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(OracleDecls, _)),
    member(compiler_type_metadata(_, OracleClosure), OracleDecls),
    program_plan(fixture(compiler_plane, Program, [], [], [])-Bindings,
                 plan(_, prog(CompilerDecls, _), _, _, _, _, _, _, _)),
    member(compiler_type_metadata(_, CompilerClosure), CompilerDecls),
    CompilerClosure == [capability(named(local, relation, 'Document'), primitive(text))],
    CompilerClosure == OracleClosure.

test(real_dl6_fixture_reaches_compiler_erasure) :-
    predicate_property(plunit_compiler_relations:compiler_decls(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/compiler-relations.dl6', Fixture,
                       [relative_to(TestDir), access(read)]),
    Out = '/private/tmp/compiler-relations.types.ts',
    setup_call_cleanup(
        true,
        ( compile_dl6(Fixture, Out),
          read_file_to_string(Out, Text, []),
          \+ sub_string(Text, _, _, _, 'Capability') ),
        ( exists_file(Out) -> delete_file(Out) ; true )).

:- end_tests(compiler_relations).
