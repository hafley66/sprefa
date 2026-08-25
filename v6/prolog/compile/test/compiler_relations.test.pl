:- begin_tests(compiler_relations).

:- use_module('../../0_compiler_relations',
              [ partition_compiler_relations/3,
                partition_compiler_program/5,
                evaluate_compiler_relations/3 ]).
:- use_module('../../0_generic_expand', [ expand_generic_program_with_bindings/3,
                                          canonical_type_name/2,
                                          type_relation_rows/2 ]).
:- use_module('../../1_expansion', [expand_program/3]).
:- use_module('../../compile', [program_plan/2, compile_dl6/2]).
:- use_module('../../lower', [lower_program/2]).
:- use_module('../parse_dl_dcg', [parse_dl/4]).
:- use_module('../../use_resolve', [expand_uses/8]).
:- use_module(library(process)).
:- use_module(library(readutil)).

:- op(1150, xfx, <-).
:- op(700, xfx, :=).

compiler_decls([
    col_type(codec/2, self, type),
    col_type(codec/2, format, type),
    keyed(codec/2, [1]),
    col_type(runtime/1, value, text)
]).

partial_derived_source(Codes) :-
    string_codes(
        "rel User(id: key(int), name: text).\n\c
         rel Partial(Source: type) -> type.\n\c
         rel Holder(value: Partial(User)).\n\c
         Partial(Source, Partial(Source)) <- type_requested(_, Partial, [Source]).\n\c
         derived_relation_request(PartialType, Partial, [Source], Count) <- type_requested(PartialType, Partial, [Source]), type_field_count(Source, Count).\n\c
         derived_member_request(PartialType, Position, Name, option(MemberType)) <- type_requested(PartialType, Partial, [Source]), type_field(_, Source, Position, Name, MemberType).\n\c
         derived_member_role_request(PartialType, Position, 'optionalized', '') <- type_requested(PartialType, Partial, [Source]), type_field(_, Source, Position, _, _).\n",
        Codes).

derived_shape_result(Closure, Result) :-
    catch(( generic_expand:compiler_derived_relation_shapes(Closure, Shapes),
            Result = shapes(Shapes) ),
          Error,
          Result = throws(Error)).

compiler_derived_fixture_plan(Plan) :-
    predicate_property(plunit_compiler_relations:compiler_decls(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name(
        '../../../dl/fixtures/0_compiler-derived-relation.dl6', Fixture,
        [relative_to(TestDir), access(read)]),
    expand_uses(Fixture, [], [], _, Program, _, Bindings, []),
    compile:dl6_seeded_form(Program, Initial, Seeded),
    program_plan(
        fixture('compiler-derived-relation', Seeded, Initial, [], [])-Bindings,
        Plan).

test(functional_type_heads_lower_to_explicit_type_apply_ir) :-
    partial_derived_source(Codes),
    parse_dl(Codes, prog(Decls, Rules), Bindings, []),
    partition_compiler_program(
        Decls, Rules, compiler_relations(_, CompilerRules0), _, _),
    generic_expand:elaborate_compiler_rules(
        Decls, Bindings, CompilerRules0, CompilerRules, []),
    nth1(1, CompilerRules, PartialRule),
    nth1(3, CompilerRules, MemberRule),
    copy_term([PartialRule, MemberRule], Lowered),
    numbervars(Lowered, 0, _),
    Lowered ==
    [ ('Partial'('$VAR'(0), '$VAR'(1)) <-
          ( type_requested('$VAR'(2), named(local, relation, 'Partial'),
                           ['$VAR'(0)]),
            type_apply(named(local, relation, 'Partial'), ['$VAR'(0)],
                       '$VAR'(1)) )),
      (derived_member_request('$VAR'(3), '$VAR'(4), '$VAR'(5), '$VAR'(6)) <-
          ( ( type_requested('$VAR'(3), named(local, relation, 'Partial'),
                             ['$VAR'(7)]),
              type_field('$VAR'(8), '$VAR'(7), '$VAR'(4), '$VAR'(5),
                         '$VAR'(9)) ),
            type_apply(named(local, relation, option), ['$VAR'(9)],
                       '$VAR'(6)) )) ].

test(functional_head_reuses_erased_generic_constructor_arity) :-
    string_codes(
        "rel Box(T)(value: T).\nrel seed(Source: type).\nrel output(Source: type, Result: type).\nseed(int).\noutput(Source, Box(Source)) <- seed(Source).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, [])),
    Application = application(named(local, relation, 'Box'), [primitive(int)]),
    member(compiler_type_metadata(_, Closure), Decls),
    memberchk(output(primitive(int), Application), Closure).

test(nested_functional_head_terms_lower_inside_out) :-
    Decls = [col_type(seed/1, value, type),
             col_type(output/1, value, type)],
    Rules = [output(option(list(Type))) <- seed(Type)],
    partition_compiler_program(
        Decls, Rules, compiler_relations(_, CompilerRules0), _, _),
    generic_expand:elaborate_compiler_rules(
        Decls, [], CompilerRules0, CompilerRules, []),
    copy_term(CompilerRules, Lowered),
    numbervars(Lowered, 0, _),
    Lowered ==
    [ (output('$VAR'(0)) <-
          ( seed('$VAR'(1)),
            ( type_apply(named(local, relation, list), ['$VAR'(1)],
                         '$VAR'(2)),
              type_apply(named(local, relation, option), ['$VAR'(2)],
                         '$VAR'(0)) ) )) ].

test(partition_erases_compiler_declarations_from_runtime) :-
    compiler_decls(Decls),
    partition_compiler_relations(Decls, compiler_relations(Relations, []), Runtime),
    Relations = [compiler_relation(codec/2, 2, [1])],
    Runtime == [col_type(runtime/1, value, text)].

test(mixed_scalar_domains_are_compiler_values) :-
    partition_compiler_relations(
        [ col_type(codec/2, self, type),
          col_type(codec/2, label, text),
          col_type(runtime/1, value, text) ],
        compiler_relations([compiler_relation(codec/2, 2, [])], []),
        [col_type(runtime/1, value, text)]).

test(runtime_structural_terms_do_not_activate_compiler_pattern_sources) :-
    Decls = [ col_type(source/1, value, text),
              col_type(output/1, value, any) ],
    Rules = [output(primitive(Value)) <- source(Value)],
    partition_compiler_program(
        Decls, Rules, compiler_relations(Relations, CompilerRules),
        RuntimeDecls, RuntimeRules),
    Relations == [],
    CompilerRules == [],
    RuntimeDecls == Decls,
    RuntimeRules == Rules.

test(recursive_positive_rules_reach_a_set_fixpoint) :-
    Decls = [ compiler_relation(parent/2, 2, []),
              compiler_relation(ancestor/2, 2, []) ],
    Rules = [ ancestor(X, Y) <- parent(X, Y),
              ancestor(X, Z) <- (parent(X, Y), ancestor(Y, Z)) ],
    Seeds = [parent(a, b), parent(b, c), parent(a, b)],
    evaluate_compiler_relations(compiler_relations(Decls, Rules), Seeds, Closure),
    Closure == [ancestor(a, b), ancestor(a, c), ancestor(b, c),
                parent(a, b), parent(b, c)].

test(scalar_bind_and_comparison_share_runtime_expression_semantics) :-
    Relations = [ compiler_relation(source_position/3, 3, []),
                  compiler_relation(shifted_position/3, 3, []) ],
    Rules = [ shifted_position(Owner, Next, Label) <-
                  ( source_position(Owner, Position, RawLabel),
                    Next := Position + 1,
                    Label := upper(RawLabel),
                    Next > Position ) ],
    Seeds = [source_position(item, 1, lower)],
    evaluate_compiler_relations(compiler_relations(Relations, Rules), Seeds,
                                Closure),
    Closure == [shifted_position(item, 2, 'LOWER'),
                source_position(item, 1, lower)].

test(expression_reads_follow_authored_body_order,
     [throws(unsupported_construct(compiler_expression_non_ground(_)))]) :-
    Relations = [ compiler_relation(source_position/2, 2, []),
                  compiler_relation(shifted_position/2, 2, []) ],
    Rules = [ shifted_position(Owner, Next) <-
                  ( Next := Position + 1,
                    source_position(Owner, Position) ) ],
    evaluate_compiler_relations(compiler_relations(Relations, Rules), [], _).

test(comparisons_require_prior_ground_bindings,
     [throws(unsupported_construct(compiler_comparison_non_ground(_)))]) :-
    Relations = [ compiler_relation(source_position/2, 2, []),
                  compiler_relation(positive_position/1, 1, []) ],
    Rules = [ positive_position(Owner) <-
                  ( Position > 0,
                    source_position(Owner, Position) ) ],
    evaluate_compiler_relations(compiler_relations(Relations, Rules), [], _).

test(grouped_count_reads_a_completed_lower_stratum) :-
    Relations = [ compiler_relation(source_member/3, 3, []),
                  compiler_relation(normalized_member/3, 3, []),
                  compiler_relation(member_count/2, 2, []),
                  compiler_relation(complete_owner/1, 1, []) ],
    Rules = [ normalized_member(Owner, Position, Name) <-
                  source_member(Owner, Position, Name),
              member_count(Owner, count(Position)) <-
                  normalized_member(Owner, Position, _),
              complete_owner(Owner) <-
                  ( member_count(Owner, Count), Count =:= 2 ) ],
    Seeds = [ source_member(a, 1, first),
              source_member(a, 2, second),
              source_member(b, 1, only) ],
    evaluate_compiler_relations(compiler_relations(Relations, Rules), Seeds,
                                Closure),
    Closure == [ complete_owner(a),
                member_count(a, 2),
                member_count(b, 1),
                normalized_member(a, 1, first),
                normalized_member(a, 2, second),
                normalized_member(b, 1, only),
                source_member(a, 1, first),
                source_member(a, 2, second),
                source_member(b, 1, only) ].

test(aggregate_dependency_cycle_has_named_diagnostic,
     [throws(unsupported_construct(compiler_aggregate_not_stratified))]) :-
    Relations = [ compiler_relation(member_count/2, 2, []),
                  compiler_relation(expanded_member/2, 2, []) ],
    Rules = [ member_count(Owner, count(Member)) <-
                  expanded_member(Owner, Member),
              expanded_member(Owner, Member) <- member_count(Owner, Member) ],
    evaluate_compiler_relations(compiler_relations(Relations, Rules), [], _).

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

test(type_apply_constructor_cycle_is_refused,
     [throws(unsupported_construct(type_apply_recursive_construction([a/2])))]) :-
    Relations = [compiler_relation(a/2, 2, []), compiler_relation(b/2, 2, []),
                 compiler_relation(type_apply/3, 3, [])],
    Rules = [ a(Constructor, Application) <-
                  ( b(Constructor, Application),
                    type_apply(Constructor, [Application], Application) ),
              b(Constructor, Application) <- a(Constructor, Application) ],
    evaluate_compiler_relations(compiler_relations(Relations, Rules), [], _).

test(type_apply_non_ground_application_is_refused,
     [throws(unsupported_construct(type_apply_non_ground_application(_)))]) :-
    Relations = [compiler_relation(result/1, 1, []),
                 compiler_relation(type_apply/3, 3, [])],
    Rules = [result(Application) <- type_apply(_Constructor, [_Argument], Application)],
    evaluate_compiler_relations(compiler_relations(Relations, Rules), [], _).

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

test(generic_compiler_plane_is_erased_after_application_evaluation) :-
    string_codes(
        "rel Box(T)(value: T).\nrel capability(Self: type, Format: type).\ncapability(Box(int), text).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, Rules)),
    Rules == [],
    member(compiler_type_metadata(_, Closure), Decls),
    member(capability(AppId, primitive(text)), Closure),
    AppId = application(named(local, relation, 'Box'), [_]),
    \+ member(col_type(capability/2, _, _), Decls),
    \+ member(rel_template(_, _, _), Decls),
    \+ member(compiler_type_metadata(_, _), Rules).

test(type_apply_body_request_refreezes_and_next_round_observes_generated_type) :-
    Program = prog(
        [ rel_template([box], [type_parameter('T', [])], [column(value, 'T')]),
          col_type(seed/2, constructor, type), col_type(seed/2, argument, type),
          col_type(request/3, constructor, type), col_type(request/3, argument, type),
          col_type(request/3, application, type), col_type(observed/2, type, type),
          col_type(observed/2, member, type) ],
        [ seed(box, int),
          request(Constructor, Argument, Application) <-
              ( seed(Constructor, Argument),
                type_apply(Constructor, [Argument], Application) ),
          observed(Type, Member) <-
              ( type_application(Type, _), type_member(Member, _, _, value, _) ) ]),
    expand_generic_program_with_bindings(Program, [], prog(Decls, [])),
    Constructor = named(local, relation, box),
    Application = application(Constructor, [primitive(int)]),
    member(compiler_type_metadata(_, Closure), Decls),
    member(request(Constructor, primitive(int), Application), Closure),
    member(semantic_type_rows(Rows), Decls),
    member(declaration(GeneratedId, root, _, relation, materialized), Rows),
    member(observed(Application, GeneratedMember), Closure),
    memberchk(member(GeneratedMember, GeneratedId, 1, value,
                     type_ref(primitive(int))), Rows),
    \+ member(compiler_type_apply_request_rows(_), Decls).

test(type_apply_list_request_refreezes_and_erases_transport) :-
    Program = prog(
        [ col_type(seed/1, type, type), col_type(request/1, type, type) ],
        [ seed(int), request(ListInt) <-
              ( seed(T), type_apply(list, [T], ListInt) ) ]),
    expand_generic_program_with_bindings(Program, [], prog(Decls, [])),
    ListId = named(local, relation, list),
    ListInt = application(ListId, [primitive(int)]),
    member(compiler_type_metadata(_, Closure), Decls),
    member(request(ListInt), Closure),
    member(semantic_type_rows(Rows), Decls),
    memberchk(application(ListInt, ListId), Rows),
    memberchk(argument(_, ListInt, 1, type_atom(int)), Rows),
    \+ member(compiler_type_apply_request_rows(_), Decls),
    \+ member(compiler_type_apply_request(_), Decls).

test(type_apply_existing_application_reuses_canonical_identity) :-
    Program = prog(
        [ rel_template([box], [type_parameter('T', [])], [column(value, 'T')]),
          col_type(holder/1, value, box(int)),
          col_type(seed/1, type, type), col_type(request/1, type, type) ],
        [ seed(int), request(App) <- ( seed(T), type_apply(box, [T], App) ) ]),
    expand_generic_program_with_bindings(Program, [], prog(Decls, [])),
    App = application(named(local, relation, box), [primitive(int)]),
    member(compiler_type_metadata(_, Closure), Decls),
    member(request(App), Closure),
    member(semantic_type_rows(Rows), Decls),
    findall(Application, member(application(Application, _), Rows), Applications),
    include(=(App), Applications, [App]),
    \+ member(compiler_type_apply_request_rows(_), Decls),
    \+ member(compiler_type_apply_request(_), Decls).

test(type_apply_only_demand_materializes_derived_relation) :-
    Program = prog(
        [ col_type(user/2, id, int), col_type(user/2, name, text),
          col_type(partial/2, source, type), col_type(partial/2, return, type),
          col_type(seed/1, source, type),
          col_type(request/1, application, type) ],
        [ seed(user),
          request(Application) <-
              ( seed(Source), type_apply(partial, [Source], Application) ),
          derived_relation_request(Application, partial, [Source], Count) <-
              ( type_requested(Application, partial, [Source]),
                type_field_count(Source, Count) ),
          derived_member_request(Application, Position, Name, Optional) <-
              ( type_requested(Application, partial, [Source]),
                type_field(_, Source, Position, Name, MemberType),
                type_apply(option, [MemberType], Optional) ) ]),
    expand_generic_program_with_bindings(Program, [], prog(Decls, [])),
    User = named(local, relation, user),
    Constructor = named(local, relation, partial),
    Application = application(Constructor, [User]),
    canonical_type_name(partial(user), GeneratedName),
    Generated = named(local, relation, GeneratedName),
    Option = named(local, relation, option),
    member(compiler_type_metadata(_, Closure), Decls),
    memberchk(request(Application), Closure),
    member(semantic_type_rows(Rows), Decls),
    memberchk(derived_from(Generated, Application), Rows),
    memberchk(member(member(Generated, 1, id), Generated, 1, id,
                     type_ref(application(
                         application(Option, [primitive(int)])))), Rows),
    memberchk(member(member(Generated, 2, name), Generated, 2, name,
                     type_ref(application(
                         application(Option, [primitive(text)])))), Rows),
    \+ member(compiler_derived_type_demand(_), Decls),
    \+ member(compiler_derived_type_application(_), Decls).

test(zero_member_derived_relation_materializes) :-
    Program = prog(
        [ type_decl(empty, []),
          col_type(partial/2, source, type), col_type(partial/2, return, type),
          col_type(seed/1, source, type),
          col_type(request/1, application, type) ],
        [ seed(empty),
          request(Application) <-
              ( seed(Source), type_apply(partial, [Source], Application) ),
          derived_relation_request(Application, partial, [Source], Count) <-
              ( type_requested(Application, partial, [Source]),
                type_field_count(Source, Count) ) ]),
    expand_generic_program_with_bindings(Program, [], prog(Decls, [])),
    Empty = named(local, relation, empty),
    Constructor = named(local, relation, partial),
    Application = application(Constructor, [Empty]),
    canonical_type_name(partial(empty), GeneratedName),
    Generated = named(local, relation, GeneratedName),
    member(semantic_type_rows(Rows), Decls),
    memberchk(declaration(Generated, root, GeneratedName, relation,
                          materialized), Rows),
    memberchk(derived_from(Generated, Application), Rows),
    \+ member(member(_, Generated, _, _, _), Rows).

test(functional_type_head_builds_demanded_partial_relation) :-
    partial_derived_source(Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, [])),
    User = named(local, relation, 'User'),
    Constructor = named(local, relation, 'Partial'),
    Application = application(Constructor, [User]),
    canonical_type_name('Partial'('User'), GeneratedName),
    Generated = named(local, relation, GeneratedName),
    Option = named(local, relation, option),
    OptionInt = application(Option, [primitive(int)]),
    OptionText = application(Option, [primitive(text)]),
    memberchk(col_type('Holder'/1, value, GeneratedName), Decls),
    ( member(compiler_type_metadata(_, Closure, _), Decls)
    ; member(compiler_type_metadata(_, Closure), Decls) ),
    memberchk('Partial'(User, Application), Closure),
    findall(Arguments,
            member(type_requested(application(Constructor, Arguments),
                                  Constructor, Arguments), Closure),
            PartialDemands),
    PartialDemands == [[User]],
    member(semantic_type_rows(Rows), Decls),
    memberchk(application(Application, Constructor), Rows),
    memberchk(derived_from(Generated, Application), Rows),
    memberchk(member(member(Generated, 1, id), Generated, 1, id,
                     type_ref(application(OptionInt))), Rows),
    memberchk(member(member(Generated, 2, name), Generated, 2, name,
                     type_ref(application(OptionText))), Rows),
    memberchk(member_role(member(Generated, 1, id), optionalized), Rows),
    memberchk(member_role(member(Generated, 2, name), optionalized), Rows),
    \+ member(member_role(member(Generated, 1, id), key), Rows),
    \+ member(compiler_type_apply_request_rows(_), Decls),
    \+ member(compiler_type_apply_request(_), Decls),
    \+ member(compiler_derived_relation_request_rows(_), Decls),
    \+ member(compiler_derived_member_role(_, _, _, _), Decls),
    \+ member(compiler_derived_type_application(_), Decls).

test(compiler_derived_relation_reaches_catalog_and_sqlite) :-
    compiler_derived_fixture_plan(Plan),
    Generated = '__gen__Partial_User_9d7a703929b72789',
    GeneratedTable =
        '0_compiler_derived_relation___gen__Partial_User_9d7a703929b72789',
    Plan = plan(_, _, _, RelPlans, _, _, _, _, _),
    include([rel(Name/_, _, _, _, _)]>>
                memberchk(Name, ['Holder', 'User', Generated]),
            RelPlans, PublicPlans),
    PublicPlans ==
    [ rel('Holder'/1,
          '0_compiler_derived_relation_Holder_b505f3e9a0ba', set,
          [col(value, declared('Partial'('User')), ref(Generated))], none),
      rel('User'/2,
          '0_compiler_derived_relation_User_a429a5abde3f', set,
          [col(id, declared(int), int), col(name, declared(text), text)],
          key([1])),
      rel(Generated/2, GeneratedTable, set,
          [ col(id, declared(option(int)), int),
            col(name, declared(option(text)), int) ], none) ],
    lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)),
    format(atom(GeneratedDdl),
           'CREATE TABLE "~w" ("__id" INTEGER PRIMARY KEY, "id" INTEGER NOT NULL, "name" INTEGER NOT NULL, UNIQUE ("id", "name"))',
           [GeneratedTable]),
    memberchk(GeneratedDdl, Ddl),
    process_create(path(sqlite3), [':memory:'],
                   [stdin(pipe(Input)), stdout(pipe(Output)), process(Pid)]),
    forall(member(Sql, Ddl), format(Input, '~w;~n', [Sql])),
    format(Input,
           'SELECT name FROM sqlite_master WHERE type = ''table'' AND name = ''~w'';~n',
           [GeneratedTable]),
    close(Input),
    read_string(Output, _, Text),
    close(Output),
    process_wait(Pid, exit(0)),
    format(string(Expected), '~w\n', [GeneratedTable]),
    Text == Expected.

test(repeated_and_nested_derived_applications_deduplicate) :-
    Program = prog(
        [ col_type(user/1, id, int),
          col_type(partial/2, source, type),
          col_type(partial/2, return, type),
          col_type(seed/1, source, type),
          col_type(request/1, application, type) ],
        [ seed(user),
          request(Inner) <-
              ( seed(Source), type_apply(partial, [Source], Inner) ),
          request(Inner) <-
              ( seed(Source), type_apply(partial, [Source], Inner) ),
          request(Outer) <-
              ( seed(Source),
                type_apply(partial, [Source], Inner),
                type_apply(partial, [Inner], Outer) ),
          derived_relation_request(Application, partial, [Source], Count) <-
              ( type_requested(Application, partial, [Source]),
                type_field_count(Source, Count) ),
          derived_member_request(Application, Position, Name, Optional) <-
              ( type_requested(Application, partial, [Source]),
                type_field(_, Source, Position, Name, MemberType),
                type_apply(option, [MemberType], Optional) ) ]),
    expand_generic_program_with_bindings(Program, [], prog(Decls, [])),
    User = named(local, relation, user),
    Partial = named(local, relation, partial),
    Inner = application(Partial, [User]),
    Outer = application(Partial, [Inner]),
    canonical_type_name(partial(user), InnerName),
    canonical_type_name(partial(partial(user)), OuterName),
    InnerGenerated = named(local, relation, InnerName),
    OuterGenerated = named(local, relation, OuterName),
    member(semantic_type_rows(Rows), Decls),
    findall(Application,
            member(application(Application, Partial), Rows),
            PartialApplications),
    sort([Inner, Outer], ExpectedApplications),
    PartialApplications == ExpectedApplications,
    findall(Generated-Application,
            ( member(derived_from(Generated, Application), Rows),
              memberchk(Application, [Inner, Outer]) ),
            DerivedPairs),
    DerivedPairs ==
        [OuterGenerated-Outer, InnerGenerated-Inner].

test(derived_relation_request_validation_matrix) :-
    Constructor = named(local, relation, partial),
    Int = primitive(int),
    Application = application(Constructor, [Int]),
    Demand = type_requested(Application, Constructor, [Int]),
    Header0 = derived_relation_request(Application, Constructor, [Int], 0),
    Header1 = derived_relation_request(Application, Constructor, [Int], 1),
    Header2 = derived_relation_request(Application, Constructor, [Int], 2),
    Member1 = derived_member_request(Application, 1, value, Int),
    derived_shape_result([Demand, Header0], Zero),
    derived_shape_result([Demand, Header1, Member1, Member1], Deduplicated),
    derived_shape_result([Demand, Member1], MissingHeader),
    derived_shape_result([Demand, Header0, Header1], HeaderConflict),
    derived_shape_result([Demand, Header1], Incomplete),
    derived_shape_result(
        [Demand, Header1,
         derived_member_request(Application, 2, value, Int)], Positions),
    derived_shape_result(
        [Demand, Header2,
         derived_member_request(Application, 1, same, Int),
         derived_member_request(Application, 2, same, primitive(text))],
        NameConflict),
    derived_shape_result(
        [Demand, Header1,
         derived_member_request(Application, 1, value, bogus)], InvalidType),
    derived_shape_result(
        [Demand, Header0,
         derived_member_role_request(Application, 1, key, '')], InvalidRole),
    derived_shape_result(
        [Demand, Header1, Member1,
         derived_member_role_request(Application, 1, indexed, a),
         derived_member_role_request(Application, 1, indexed, b)],
        RoleConflict),
    derived_shape_result([Header0], MissingDemand),
    [Zero, Deduplicated, MissingHeader, HeaderConflict, Incomplete, Positions,
     NameConflict, InvalidType, InvalidRole, RoleConflict, MissingDemand] ==
    [ shapes([derived_relation_shape(Application, Constructor, [Int], 0,
                                     [], [])]),
      shapes([derived_relation_shape(Application, Constructor, [Int], 1,
                                     [member(1, value, Int)], [])]),
      throws(unsupported_construct(
          derived_relation_request_missing_header(Application))),
      throws(unsupported_construct(
          derived_relation_request_header_conflict(
              Application,
              [header(Constructor, [Int], 0),
               header(Constructor, [Int], 1)]))),
      throws(unsupported_construct(
          derived_relation_request_incomplete(
              Application, expected(1), found(0)))),
      throws(unsupported_construct(
          derived_relation_request_positions(
              Application, expected([1]), found([2])))),
      throws(unsupported_construct(
          derived_relation_request_name_conflict(Application, [same, same]))),
      throws(unsupported_construct(
          derived_relation_request_type(Application, bogus))),
      throws(unsupported_construct(
          derived_relation_request_role(Application, 1, key, ''))),
      throws(unsupported_construct(
          derived_relation_request_role_conflict(
              Application, 1, indexed, [a, b]))),
      throws(unsupported_construct(
          derived_relation_request_without_demand(Application))) ].

test(type_apply_unknown_constructor_is_named,
     [throws(unsupported_construct(
         type_apply_unknown_constructor(named(local, relation, missing))))]) :-
    Program = prog(
        [ col_type(seed/2, constructor, semantic), col_type(seed/2, return, type),
          col_type(request/1, application, type) ],
        [ seed(named(local, relation, missing), int),
          request(App) <- ( seed(C, T), type_apply(C, [T], App) ) ]),
    expand_generic_program_with_bindings(Program, [], _).

test(type_apply_arity_mismatch_is_named,
     [throws(unsupported_construct(
         type_apply_arity_mismatch(named(local, relation, box), 1, 2)))]) :-
    Program = prog(
        [ rel_template([box], [type_parameter('T', [])], [column(value, 'T')]),
          col_type(seed/2, constructor, semantic), col_type(seed/2, return, type),
          col_type(request/1, application, type) ],
        [ seed(named(local, relation, box), int),
          request(App) <- ( seed(C, T), type_apply(C, [T, T], App) ) ]),
    expand_generic_program_with_bindings(Program, [], _).

test(annotation_application_sites_are_typed_relation_values) :-
    string_codes("rel operation(Target: type, Method: text) -> Target.\nrel Pet(id: int).\nrel route(first: operation(Pet, Method: 'GET'), second: operation(Pet, Method: 'GET')).\nrel seen(Owner: type, Member: type, Method: text, Input: type, Output: type, Position: semantic) -> Owner.\nseen(Owner, Member, Method, Input, Output, Position, Owner) <- type_application_site(operation(Input, Method, Output), Owner, Member, Position).\n", Source),
    parse_dl(Source, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, Rules)),
    Rules == [],
    member(compiler_type_metadata(_, Closure, _), Decls),
    member(seen(Owner, First, 'GET', Pet, Pet, site([first], 1), Owner), Closure),
    member(seen(Owner, Second, 'GET', Pet, Pet, site([second], 1), Owner), Closure),
    Pet = named(local, relation, 'Pet'),
    First = member(named(local, relation, route), 1, first),
    Second = member(named(local, relation, route), 2, second),
    \+ member(type_application_site(_, _, _, _), Decls),
    \+ member(compiler_annotation_evidence(_), Decls).

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
    member(reflected_decl(Host, 'Host', relation, _, Host), Closure),
    member(reflected_decl(Imported, 'Imported', relation, _, Imported), Closure),
    Imported = named(ImportedModule, relation, 'Imported'),
    ImportedModule \== local,
    member(reflected(Imported, 1, 'ImportedCase', _, _, Imported), Closure),
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
        "rel key(Target: type) -> Target.\nrel Revision(id: key(int), body: text).\n",
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
