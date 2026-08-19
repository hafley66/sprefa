:- begin_tests(compiler_relations).

:- use_module('../../0_compiler_relations',
              [ partition_compiler_relations/3,
                partition_compiler_program/5,
                evaluate_compiler_relations/3 ]).
:- use_module('../../0_generic_expand', [ expand_generic_program_with_bindings/3,
                                          type_relation_rows/2 ]).
:- use_module('../../1_expansion', [expand_program/3]).
:- use_module('../../compile', [program_plan/2, compile_dl6/2]).
:- use_module('../../lower', [lower_program/2]).
:- use_module('../parse_dl_dcg', [parse_dl/4]).
:- use_module(library(process)).
:- use_module(library(readutil)).

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

test(annotation_key_evaluates_through_compiler_closure_and_erases_transport) :-
    string_codes(
        "rel key(Target: type) -> type.\nrel Revision(id: @(int, [key()]), body: text).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, Rules)),
    Rules == [],
    memberchk(col_type('Revision'/2, id, int), Decls),
    memberchk(keyed('Revision'/2, [1]), Decls),
    \+ member(compiler_annotation_requests(_), Decls),
    \+ member(compiler_annotation_evidence(_), Decls),
    member(compiler_type_metadata(_, _, Evidence), Decls),
    Evidence == [annotation_evidence(member(named(local, relation, 'Revision'), 1, id),
                                     [id], 1, primitive(int),
                                     key(named('Target', primitive(int))),
                                     primitive(int))].

test(empty_annotation_normalizes_without_compiler_relations) :-
    string_codes("rel X(a: @(int, [])).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, Rules)),
    Rules == [],
    memberchk(col_type('X'/1, a, int), Decls),
    \+ member(compiler_annotation_requests(_), Decls),
    \+ member(compiler_annotation_evidence(_), Decls).

test(unknown_annotation_without_compiler_relations_is_named,
     [throws(unsupported_construct(annotation_unknown_relation(mark)))]) :-
    string_codes("rel X(a: @(int, [mark()])).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(nested_key_annotation_is_named,
     [throws(unsupported_construct(annotation_key_nested_site(_, [value, 1])))]) :-
    string_codes("rel key(Target: type) -> type. rel X(value: list(@(int, [key()]))).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(explicit_annotation_target_is_named,
     [throws(unsupported_construct(annotation_target_is_implicit))]) :-
    string_codes("rel mark(Target: type) -> type. rel X(a: @(int, [mark(Target: int)])).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(annotation_compiler_fact_wrapper_outputs_round_trip) :-
    string_codes(
        "rel mark(Target: type) -> type. mark(int, list(int)). mark(text, option(text)). rel X(a: @(int, [mark()]), b: @(text, [mark()])).",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, _)),
    memberchk(col_type('X'/2, a, list(int)), Decls),
    memberchk(option_column('X'/2, b, text), Decls),
    memberchk(col_type('X'/2, b, '__opt_text'), Decls).

test(annotation_bool_and_float_arguments_reach_compiler_facts) :-
    string_codes(
        "rel mark(Target: type, Flag: bool, Ratio: float) -> type. mark(int, true, 1.5, text). rel X(a: @(int, [mark(flag: true, ratio: 1.5)])).",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, _)),
    memberchk(col_type('X'/1, a, text), Decls).

test(annotation_unknown_keyword_is_named,
     [throws(unsupported_construct(annotation_unknown_keyword(other)))]) :-
    string_codes("rel mark(Target: type, Value: int) -> type. rel X(a: @(int, [mark(other: 1)])).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(annotation_duplicate_keyword_is_named,
     [throws(unsupported_construct(annotation_duplicate_keyword(value)))]) :-
    string_codes("rel mark(Target: type, Value: int) -> type. rel X(a: @(int, [mark(value: 1, Value: 2)])).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(annotation_too_many_positionals_is_named,
     [throws(unsupported_construct(annotation_too_many_positional_arguments))]) :-
    string_codes("rel mark(Target: type, Value: int) -> type. rel X(a: @(int, [mark(1, 2)])).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(annotation_positional_named_duplicate_is_named,
     [throws(unsupported_construct(annotation_duplicate_argument(value)))]) :-
    string_codes("rel mark(Target: type, Value: int) -> type. rel X(a: @(int, [mark(1, value: 1)])).", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(annotation_composition_retains_each_site_ordinal) :-
    string_codes(
        "rel first(Target: type) -> type.\nrel second(Target: type) -> type.\nfirst(int, int).\nsecond(int, int).\nrel Revision(id: @(int, [first(), second()])).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, _)),
    member(compiler_type_metadata(_, _, Evidence), Decls),
    Evidence = [ annotation_evidence(_, [id], 1, primitive(int),
                                         first(named('Target', primitive(int))),
                                         primitive(int)),
                 annotation_evidence(_, [id], 2, primitive(int),
                                         second(named('Target', primitive(int))),
                                         primitive(int)) ].

test(annotation_mixed_compile_time_inputs_resolve_keywords_case_insensitively) :-
    string_codes(
        "rel min(Target: type, Value: int) -> type.\nmin(int, 1, text).\nrel Revision(id: @(int, [min(value: 1)])).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, _)),
    memberchk(col_type('Revision'/1, id, text), Decls).

test(annotation_zero_result_is_named,
     [throws(unsupported_construct(annotation_zero_results(mark/2)))]) :-
    string_codes(
        "rel mark(Target: type) -> type.\nrel Revision(id: @(int, [mark()])).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(annotation_multiple_results_are_named,
     [throws(unsupported_construct(annotation_multiple_results(mark/2, _)))]) :-
    string_codes(
        "rel mark(Target: type) -> type.\nmark(int, int).\nmark(int, text).\nrel Revision(id: @(int, [mark()])).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(annotation_key_uses_existing_sqlite_keyed_replace_lowering) :-
    string_codes(
        "rel key(Target: type) -> type.\nrel Revision(id: @(int, [key()]), body: text).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    program_plan(fixture(annotation_key, Program, [], [], [])-Bindings,
                 Plan),
    Plan = plan(_, _, _, RelPlans, _, _, _, _, _),
    member(rel('Revision'/2, Table, _, _, key([1])), RelPlans),
    lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)),
    process_create(path(sqlite3), [':memory:'],
                   [stdin(pipe(Input)), stdout(pipe(Output)), process(Pid)]),
    forall(member(Sql, Ddl), format(Input, '~w;~n', [Sql])),
    format(Input, 'INSERT INTO "~w" ("id", "body") VALUES (1, ''old'');~n', [Table]),
    format(Input, 'INSERT INTO "~w" ("id", "body") VALUES (1, ''new'') ON CONFLICT ("id") DO UPDATE SET "body" = excluded."body";~n', [Table]),
    format(Input, 'SELECT "body" FROM "~w";~n', [Table]),
    close(Input),
    read_string(Output, _, Text), close(Output),
    process_wait(Pid, exit(0)),
    Text == "new\n".

:- end_tests(compiler_relations).
