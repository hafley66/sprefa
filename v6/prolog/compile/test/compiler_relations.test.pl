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
:- use_module('../../use_resolve', [expand_uses/8]).
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

test(authored_rules_query_the_frozen_canonical_type_graph) :-
    predicate_property(plunit_compiler_relations:compiler_decls(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/0_type-reflection.dl6', Fixture,
                       [relative_to(TestDir), access(read)]),
    expand_uses(Fixture, [], [], _, Program, _, Bindings, Findings),
    Findings == [],
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, Rules)),
    Rules == [],
    member(compiler_type_metadata(_, Closure), Decls),
    Host = named(_, relation, 'Host'),
    member(reflected(Host, 1, 'CamelCase', _, CamelMember, Host), Closure),
    CamelMember = member(Host, 1, 'CamelCase'),
    member(reflected(Host, 2, 'MaybeValue', _, _, Host), Closure),
    member(reflected(Host, 3, 'KeyedList', _, KeyedMember, Host), Closure),
    member(reflected_role(KeyedMember, key, '', KeyedMember), Closure),
    member(reflected(Host, 4, 'Inline', type_ref(declaration(Inline)), _, Host),
           Closure),
    member(reflected(Inline, 1, 'InnerValue', _, _, Inline), Closure),
    member(reflected(Inline, 2, 'ExactCase', _, _, Inline), Closure),
    member(reflected_application(Application, Constructor, Application), Closure),
    member(reflected_argument(_, Application, 1, _, Application), Closure),
    Application = application(Constructor, [_]),
    \+ member(col_type(reflected/6, _, _), Decls),
    \+ member(col_type(reflected_role/4, _, _), Decls),
    \+ member(col_type(type_member/5, _, _), Decls).

test(authored_type_reflection_has_compiler_oracle_parity) :-
    predicate_property(plunit_compiler_relations:compiler_decls(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/0_type-reflection.dl6', Fixture,
                       [relative_to(TestDir), access(read)]),
    expand_uses(Fixture, [], [], _, Program, _, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings,
                                         prog(OracleDecls, [])),
    member(compiler_type_metadata(_, OracleClosure), OracleDecls),
    program_plan(fixture(type_reflection, Program, [], [], [])-Bindings,
                 plan(_, prog(CompilerDecls, _), _, _, _, _, _, _, _)),
    member(compiler_type_metadata(_, CompilerClosure), CompilerDecls),
    CompilerClosure == OracleClosure.

test(authored_relation_cannot_shadow_a_type_reflection_source,
     [throws(unsupported_construct(
                 compiler_relation_builtin_collision(type_member/5)))]) :-
    partition_compiler_program(
        [ col_type(type_member/5, a, type),
          col_type(type_member/5, b, type),
          col_type(type_member/5, c, type),
          col_type(type_member/5, d, type),
          col_type(type_member/5, e, type),
          col_type(project/2, source, type),
          col_type(project/2, return, type) ],
        [project(Owner, Owner) <- type_member(_, Owner, _, _, _)], _, _, _).

test(type_reflection_sources_are_absent_from_emitted_runtime) :-
    predicate_property(plunit_compiler_relations:compiler_decls(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/0_type-reflection.dl6', Fixture,
                       [relative_to(TestDir), access(read)]),
    Out = '/private/tmp/type-reflection.ts',
    setup_call_cleanup(
        true,
        ( compile_dl6(Fixture, Out),
          read_file_to_string(Out, Text, []),
          \+ sub_string(Text, _, _, _, 'type_member'),
          \+ sub_string(Text, _, _, _, 'reflected_application') ),
        ( exists_file(Out) -> delete_file(Out) ; true )).

test(direct_calls_compose_and_bind_typed_values) :-
    string_codes(
        "rel first(Target: type) -> type. first(int, text). rel second(Target: type) -> type. second(text, text). rel configure(Target: type, Value: int, Enabled: bool, Ratio: float) -> type. configure(int, 7, true, 1.5, text). rel X(composed: second(first(int)), configured: configure(int, Value: 7, Enabled: true, Ratio: 1.5)).",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, _)),
    memberchk(col_type('X'/2, composed, text), Decls),
    memberchk(col_type('X'/2, configured, text), Decls).

test(direct_type_calls_survive_the_text_door_fact_split) :-
    string_codes(
        "rel configure(Target: type, Value: int) -> type. configure(int, 7, text). rel X(configured: configure(int, Value: 7)).",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    compile:dl6_seeded_form(Program, Initial, Seeded),
    Initial == [],
    program_plan(fixture(direct_type_call, Seeded, Initial, [], [])-Bindings, _).

test(direct_key_uses_existing_sqlite_keyed_replace_lowering) :-
    string_codes(
        "rel key(Target: type) -> type.\nrel Revision(id: key(int), body: text).\n",
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
