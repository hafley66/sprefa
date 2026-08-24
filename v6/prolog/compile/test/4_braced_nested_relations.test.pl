:- begin_tests(braced_nested_relations).

:- use_module(library(filesex), [delete_directory_and_contents/1]).
:- use_module('../parse_dl_dcg',
              [ parse_dl/4,
                parse_dl_line_for_reason/2,
                statement_location_for_reference/4
              ]).
:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../0_dot_expand',
              [ expand_dot_in_context/3,
                resolve_qualified_types/2
              ]).
:- use_module('../../1_expansion', [expand_program/3]).
:- use_module('../../0_generic_expand', [type_projection_targets/2]).
:- use_module('../../1_host_expand', [prepare_program/5]).
:- use_module('../../use_resolve', [expand_uses/6]).
:- use_module('../../compile', [dl6_seeded_form/3, program_plan/3]).
:- use_module('../../lower',
              [ lower_program/2,
                boot_statements/7,
                catalog_decl_rows/6
              ]).
:- use_module('../../emit_ts', [emit_program/5]).
:- use_module('../../emit_rust', [emit_program/5 as emit_rust_program]).
:- use_module('../4_emit_jsonschema', [jsonschema_text/3]).
:- use_module('../5_emit_openapi', [openapi_text/3]).
:- use_module('../7_emit_ts_types', [ts_types_text/3]).
:- use_module('../8_emit_rust_types', [rust_types_text/3]).

parse_braced_source(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

source_text(Lines, Text) :-
    atomics_to_string(Lines, "\n", Text).

write_use_source(Dir, Name=Content) :-
    atomic_list_concat([Dir, '/', Name], Path),
    setup_call_cleanup(
        open(Path, write, Stream),
        format(Stream, "~s", [Content]),
        close(Stream)).

expand_use_sources(Files, EntryName, Program) :-
    tmp_file(braced_nested_use, Seed),
    atom_concat(Seed, '_dir', Dir),
    setup_call_cleanup(
        make_directory_path(Dir),
        ( maplist(write_use_source(Dir), Files),
          atomic_list_concat([Dir, '/', EntryName], Entry),
          expand_uses(Entry, [], [], _, Program, _) ),
        delete_directory_and_contents(Dir)).

parse_outcome(Source, Outcome) :-
    string_codes(Source, Codes),
    catch(( parse_dl(Codes, _, _, []), Outcome = parsed ),
          Error,
          Outcome = Error).

brace_equivalent_sources(Brace, Dotted) :-
    source_text(
        [ "rel orchard(orchard_id: int) {",
          "    rel tree(tree_id: int) {",
          "        rel branch(branch_id: int).",
          "    }.",
          "}.",
          "rel grove(tree: orchard.tree, branch: orchard.tree.branch)." ],
        Brace),
    source_text(
        [ "rel orchard(orchard_id: int).",
          "rel orchard.tree(tree_id: int).",
          "rel orchard.tree.branch(branch_id: int).",
          "rel grove(tree: orchard.tree, branch: orchard.tree.branch)." ],
        Dotted).

test(three_level_braces_parse_to_the_dotted_program) :-
    brace_equivalent_sources(Brace, Dotted),
    parse_braced_source(Brace, BraceProgram, BraceBindings),
    parse_braced_source(Dotted, DottedProgram, DottedBindings),
    BraceProgram =@= DottedProgram,
    BraceBindings =@= DottedBindings.

test(a_local_dotted_child_path_appends_to_the_enclosing_path) :-
    Brace = "rel orchard(orchard_id: int) { rel north.tree(tree_id: int). }.",
    Dotted = "rel orchard(orchard_id: int). rel orchard.north.tree(tree_id: int).",
    parse_braced_source(Brace, BraceProgram, _),
    parse_braced_source(Dotted, DottedProgram, _),
    BraceProgram =@= DottedProgram.

test(an_empty_block_is_the_same_declaration_as_a_period) :-
    parse_braced_source("rel orchard(orchard_id: int) {}.", BraceProgram, _),
    parse_braced_source("rel orchard(orchard_id: int).", DottedProgram, _),
    BraceProgram =@= DottedProgram.

test(brace_children_keep_authored_columns_and_key_positions) :-
    source_text(
        [ "rel orchard(orchard_id: int) {",
          "    rel tree(tree_id: int) key(1) {",
          "        rel branch(branch_id: int).",
          "    }.",
          "}." ],
        Source),
    parse_braced_source(Source, Program, _),
    once(expand_program(Program, prog(Decls, _), _)),
    memberchk(col_type(orchard__tree/1, tree_id, int), Decls),
    memberchk(keyed(orchard__tree/1, [1]), Decls),
    memberchk(col_type(orchard__tree__branch/1, branch_id, int), Decls),
    \+ memberchk(col_type(orchard__tree/_, parent, _), Decls),
    \+ memberchk(col_type(orchard__tree__branch/_, parent, _), Decls).

test(a_zero_column_brace_child_stays_zero_arity) :-
    parse_braced_source("rel orchard(orchard_id: int) { rel flag(). }.",
                        Program, _),
    once(expand_program(Program, prog(Decls, _), _)),
    memberchk(rel_path_decl(orchard__flag/0, [orchard, flag]), Decls),
    \+ memberchk(col_type(orchard__flag/_, _, _), Decls).

test(a_member_and_nested_relation_with_different_targets_refuse,
     [throws(unsupported_construct(
                 ambiguous_type_projection(
                     named(local, relation, a),
                     x,
                     [ primitive(text),
                       named(local, relation, a__x)
                     ])))]) :-
    parse_braced_source(
        "rel a(x: text) { rel x(value: int). }.",
        Program, _),
    once(expand_program(Program, _, _)).

test(a_projection_collision_diagnostic_names_both_targets) :-
    Reason = ambiguous_type_projection(
                 named(local, relation, a),
                 x,
                 [primitive(text), named(local, relation, a__x)]),
    message_to_string(unsupported_construct(Reason), Text),
    Text == "rule-index unavailable: unsupported_construct: compiler refused projection 'a.x': name resolves to [text, a__x] (ambiguous_type_projection)".

test(a_member_and_nested_relation_with_one_target_collapse_to_one_projection) :-
    parse_braced_source(
        "rel a(x: a.x) { rel x(value: int). }.",
        Program, _),
    once(expand_program(Program, prog(Decls, _), _)),
    type_projection_targets(Decls, Targets),
    findall(Target,
            member(type_projection(named(local, relation, a), x, Target),
                   Targets),
            XTargets),
    XTargets == [named(local, relation, a__x)].

test(an_inline_sum_contributes_one_member_projection_and_no_nested_x_edge) :-
    parse_braced_source(
        "rel a(x: (left(); right())).",
        Program, _),
    once(expand_program(Program, prog(Decls, _), _)),
    type_projection_targets(Decls, Targets),
    findall(Target,
            member(type_projection(named(local, relation, a), x, Target),
                   Targets),
            XTargets),
    XTargets = [named(local, enum, AnonymousEnum)],
    sub_atom(AnonymousEnum, 0, _, _, '__anon_a_x_').

test(a_nested_declaration_keeps_its_own_source_line) :-
    source_text(
        [ "rel orchard(orchard_id: int) {",
          "    rel tree(tree_id: int) {",
          "        rel branch(branch_id: int).",
          "    }.",
          "}." ],
        Source),
    parse_braced_source(Source, _, _),
    parse_dl_line_for_reason(synthetic(orchard__tree__branch/1), Line),
    statement_location_for_reference(decl, orchard__tree__branch/1,
                                     LocationLine, Column),
    Line == 3,
    LocationLine == 3,
    Column == 9.

test(a_non_relation_statement_inside_a_block_reports_its_line) :-
    source_text(
        [ "rel orchard(orchard_id: int) {",
          "    branch(Value) <- source(Value).",
          "}." ],
        Source),
    string_codes(Source, Codes),
    catch(parse_dl(Codes, _, _, []), Error, true),
    Error = dl_parse_error(nested_relation_declaration, position(2, 5)).

test(the_canonical_printer_turns_braces_into_dotted_declarations) :-
    brace_equivalent_sources(Brace, _),
    parse_braced_source(Brace, Program, Bindings),
    print_dl_program(Program, Bindings, Printed),
    Printed == 'rel orchard(orchard_id: int).\nrel orchard.tree(tree_id: int).\nrel orchard.tree.branch(branch_id: int).\nrel grove(tree: orchard.tree, branch: orchard.tree.branch).\n',
    parse_braced_source(Printed, Reparsed, _),
    Program =@= Reparsed.

test(child_modifiers_and_arrow_match_dotted_declarations) :-
    source_text(
        [ "rel root(id: int) {",
          "    rel child(value: int) -> text log keep(all) key(1) {",
          "        rel leaf(flag: int) keep(count(2)).",
          "    }.",
          "}." ],
        Brace),
    source_text(
        [ "rel root(id: int).",
          "rel root.child(value: int) -> text log keep(all) key(1).",
          "rel root.child.leaf(flag: int) keep(count(2))." ],
        Dotted),
    parse_braced_source(Brace, BraceProgram, _),
    parse_braced_source(Dotted, DottedProgram, _),
    BraceProgram =@= DottedProgram,
    once(expand_program(BraceProgram, prog(Decls, _), _)),
    memberchk(kind(root__child/2, log), Decls),
    memberchk(keep(root__child/2, all), Decls),
    memberchk(keyed(root__child/2, [1]), Decls),
    memberchk(keep(root__child__leaf/1, count(2)), Decls).

test(outside_rules_read_and_contribute_to_a_brace_declared_child) :-
    source_text(
        [ "rel orchard(orchard_id: int) { rel tree(tree_id: int). }.",
          "rel planted(orchard_id: int, tree_id: int).",
          "rel any_tree(tree_id: int).",
          "orchard.tree(TreeId) <- orchard(OrchardId), planted(OrchardId, TreeId).",
          "any_tree(TreeId) <- orchard.tree(TreeId)." ],
        Source),
    parse_braced_source(Source, Program, _),
    once(expand_program(Program, prog(_, Rules), _)),
    Rules =@=
        [ (orchard__tree(TreeId) <-
              (orchard(OrchardId), planted(OrchardId, TreeId))),
          (any_tree(TreeId) <- orchard__tree(TreeId)) ].

test(a_dotted_fact_resolves_to_its_declared_flat_name) :-
    parse_braced_source(
        "rel config() { rel global(poll_period: int). }. config.global(30).",
        Parsed, Bindings),
    dl6_seeded_form(Parsed, Initial, Program),
    Initial == [config__global(30)],
    Program = prog(_, []),
    program_plan(fixture(braced_dotted_fact, Program, Initial, [], [])-Bindings,
                 [],
                 plan(_, prog(_, Rules), _, RelPlans, _, _, _, _, _)),
    Rules == [],
    memberchk(rel(config__global/1, _, _, _, _), RelPlans).

test(an_explicit_parent_relation_column_uses_ordinary_relation_value_typing) :-
    source_text(
        [ "rel orchard(orchard_id: int) {",
          "    rel tree(orchard: orchard, tree_id: int).",
          "}.",
          "rel planted(orchard_id: int, tree_id: int).",
          "orchard.tree(orchard(OrchardId), TreeId) <-",
          "    orchard(OrchardId), planted(OrchardId, TreeId)." ],
        Source),
    parse_braced_source(Source, Program, Bindings),
    program_plan(fixture(braced_explicit_parent, Program, [], [], [])-Bindings,
                 [],
                 plan(_, prog(Decls, Rules), _, RelPlans, _, _, _, _, _)),
    memberchk(col_type(orchard__tree/2, orchard, orchard), Decls),
    memberchk(col_type(orchard__tree/2, tree_id, int), Decls),
    memberchk(rel(orchard__tree/2, _, _, _, _), RelPlans),
    Rules =@= [(orchard__tree(orchard(OrchardId), TreeId) <-
                   (orchard(OrchardId), planted(OrchardId, TreeId)))].

test(flat_name_collision_digest_matches_the_dotted_spelling) :-
    Brace = "rel orchard__tree(flat: int). rel orchard(id: int) { rel tree(nested: int, other: text). }.",
    Dotted = "rel orchard__tree(flat: int). rel orchard(id: int). rel orchard.tree(nested: int, other: text).",
    parse_braced_source(Brace, BraceProgram, _),
    parse_braced_source(Dotted, DottedProgram, _),
    BraceProgram =@= DottedProgram,
    BraceProgram = prog(Decls, []),
    memberchk(rel_path_decl(Digested/2, [orchard, tree]), Decls),
    atom_concat('orchard__tree__', _, Digested).

test(generic_enum_and_arrival_children_have_pinned_parse_failures) :-
    Cases =
        [ "rel root(id: int) { rel box(T)(value: T). }."-
              dl_parse_error(nested_relation_declaration, position(1, 42)),
          "rel root(id: int) { rel choice(One(); Two()). }."-
              dl_parse_error(nested_relation_declaration, position(1, 32)),
          "rel root(id: int) { rel fetch(input: int) -> (output: int). }."-
              dl_parse_error(nested_relation_declaration, position(1, 42)) ],
    forall(member(Source-Expected, Cases),
           ( parse_outcome(Source, Outcome),
             Outcome == Expected )).

test(generic_enum_and_arrival_owners_cannot_open_blocks) :-
    Cases =
        [ "rel box(T)(value: T) { rel child(x: int). }."-
              dl_parse_error(statement, position(1, 22)),
          "rel choice(One(); Two()) { rel child(x: int). }."-
              dl_parse_error(statement, position(1, 26)),
          "rel fetch(input: int) -> (output: int) { rel child(x: int). }."-
              dl_parse_error(statement, position(1, 40)) ],
    forall(member(Source-Expected, Cases),
           ( parse_outcome(Source, Outcome),
             Outcome == Expected )).

test(deep_rule_head_body_match_and_wrapper_references_share_one_path) :-
    source_text(
        [ "rel orchard.north.tree(value: int).",
          "rel source(value: int).",
          "rel seen(value: int).",
          "orchard.north.tree(Value) <- source(Value).",
          "seen(Value) <- latest(orchard.north.tree(Value)).",
          "match orchard.north.tree(Value) (source(Value) |-> seen(Value))." ],
        Source),
    parse_braced_source(Source, prog(Decls, Rules), _),
    expand_dot_in_context([], prog(Decls, Rules), prog(_, Resolved)),
    Resolved =@=
        [ (orchard__north__tree(Value) <- source(Value)),
          (seen(Value) <- latest(orchard__north__tree(Value))),
          match(orchard__north__tree(Value),
                (seen(Value) <- source(Value))) ].

test(deep_negated_relation_reference_resolves) :-
    source_text(
        [ "rel orchard.north.tree(value: int).",
          "rel source(value: int).",
          "rel seen(value: int).",
          "seen(Value) <- source(Value), !orchard.north.tree(Value)." ],
        Source),
    parse_braced_source(Source, prog(Decls, Rules), _),
    expand_dot_in_context([], prog(Decls, Rules), prog(_, Resolved)),
    Resolved =@=
        [(seen(Value) <- (source(Value), not(orchard__north__tree(Value))))].

test(deep_relation_value_in_an_expression_resolves) :-
    source_text(
        [ "rel orchard.north.tree(value: int).",
          "rel source(value: int).",
          "rel captured(value: text).",
          "captured(orchard.north.tree(Value)) <- source(Value)." ],
        Source),
    parse_braced_source(Source, prog(Decls, Rules), _),
    expand_dot_in_context([], prog(Decls, Rules), prog(_, Resolved)),
    Resolved =@=
        [(captured(orchard__north__tree(Value)) <- source(Value))].

test(deep_query_target_resolves_before_query_compilation) :-
    source_text(
        [ "rel orchard.north.tree(value: int).",
          "? orchard.north.tree(Value) order by Value." ],
        Source),
    parse_braced_source(Source, Program, _),
    prepare_program(Program, prog(Decls, []), [], [], QueryPlans),
    memberchk(query(orchard__north__tree(_), order([order_col(1, asc)])), Decls),
    QueryPlans = [query_plan(orchard__north__tree/1,
                             columns([QueryValue]), snapshot(current))],
    var(QueryValue).

test(a_deep_host_call_resolves_in_rule_body_position) :-
    source_text(
        [ "rel namespace.fetch(input: text) -> (output: text).",
          "rel result(output: text).",
          "result(Output) <- namespace.fetch('x', Output)." ],
        Source),
    parse_braced_source(Source, program(Decls, Rules, []), _),
    memberchk(sh_decl(namespace__fetch,
                      [col(input, text)], [col(output, text)], template("")),
              Decls),
    Rules =@= [(result(Output) <-
                    probe(namespace__fetch, [x], [Output], []))].

test(deep_bare_wrapped_identity_and_host_types_resolve) :-
    source_text(
        [ "rel namespace.entity(id: int).",
          "rel holder(one: namespace.entity, many: list(namespace.entity), identity: namespace.entity.id).",
          "rel namespace.fetch(input: namespace.entity) -> (output: namespace.entity)." ],
        Source),
    parse_braced_source(Source, Program, _),
    Program = program(_, Rules, Queries),
    resolve_qualified_types(Program, program(Decls, Rules, Queries)),
    memberchk(col_type(holder/3, one, namespace__entity), Decls),
    memberchk(col_type(holder/3, many, list(namespace__entity)), Decls),
    memberchk(col_type(holder/3, identity, id(namespace__entity)), Decls),
    memberchk(sh_decl(namespace__fetch,
                      [col(input, namespace__entity)],
                      [col(output, namespace__entity)], _), Decls).

test(nested_relations_used_as_types_keep_their_authored_shape) :-
    source_text(
        [ "rel orchard(orchard_id: int) { rel tree(tree_id: int) { rel branch(branch_id: int). }. }.",
          "rel grove(tree: orchard.tree, branch: orchard.tree.branch)." ],
        Source),
    parse_braced_source(Source, Program, _),
    once(expand_program(Program, prog(Decls, _), _)),
    memberchk(type_decl(orchard__tree, [col(tree_id, int)]), Decls),
    memberchk(type_decl(orchard__tree__branch, [col(branch_id, int)]), Decls),
    memberchk(col_type(grove/2, tree, orchard__tree), Decls),
    memberchk(col_type(grove/2, branch, orchard__tree__branch), Decls).

test(deep_type_constructor_application_resolves_and_materializes) :-
    source_text(
        [ "rel namespace.types.box(T)(value: T).",
          "rel holder(value: namespace.types.box(int))." ],
        Source),
    parse_braced_source(Source, Program, _),
    resolve_qualified_types(Program, prog(ResolvedDecls, _)),
    memberchk(col_type(holder/1, value, namespace__types__box(int)),
              ResolvedDecls),
    once(expand_program(Program, prog(ExpandedDecls, _), _)),
    once(member(type_decl(Concrete, [col(value, int)]), ExpandedDecls)),
    sub_atom(Concrete, 0, _, _, '__gen__namespace__types__box_int_').

test(deep_compiler_relation_application_elaborates_from_column_colon_syntax) :-
    source_text(
        [ "rel namespace.types.identity(Target: type) -> Target.",
          "rel holder(value: namespace.types.identity(int))." ],
        Source),
    parse_braced_source(Source, Program, _),
    once(expand_program(Program, prog(Decls, _), _)),
    memberchk(col_type(holder/1, value, int), Decls),
    memberchk(compiler_type_metadata(
                  _, _,
                  [annotation_evidence(_, [value], 1, _, _, _, _)]),
              Decls).

test(deep_paths_resolve_inside_direct_annotations_generic_enum_and_anonymous_terms) :-
    source_text(
        [ "rel namespace.entity(id: int).",
          "rel namespace.types.box(T)(value: T).",
          "rel namespace.annotations.identity(Target: type) -> Target.",
          "rel wrapper(T)(direct: namespace.annotations.identity(namespace.entity), boxed: namespace.types.box(T)).",
          "rel outcome(Ok(value: namespace.entity); Boxed(value: namespace.types.box(int))).",
          "rel holder(value: (direct: namespace.annotations.identity(namespace.entity), boxed: namespace.types.box(int)))." ],
        Source),
    parse_braced_source(Source, Program, _),
    resolve_qualified_types(Program, prog(Decls, _)),
    memberchk(
        rel_template([wrapper],
                     [type_parameter('T', [])],
                     [ column(direct,
                              namespace__annotations__identity(
                                  namespace__entity)),
                       column(boxed, namespace__types__box('T')) ]),
        Decls),
    memberchk(
        enum_decl(outcome,
                  ( 'Ok'(value:namespace__entity)
                  ; 'Boxed'(value:namespace__types__box(int)) )),
        Decls),
    memberchk(
        col_type(holder/1, value,
                 product_type(
                     [ field(direct,
                             namespace__annotations__identity(
                                 namespace__entity)),
                       field(boxed, namespace__types__box(int)) ])),
        Decls),
    \+ ( member(Decl, Decls),
         sub_term(type_path_application(_, _), Decl) ).

test(a_deep_type_application_prints_and_reparses_canonically) :-
    Source = "rel namespace.types.box(T)(value: T). rel holder(value: namespace.types.box(list(int))).",
    parse_braced_source(Source, Program, Bindings),
    once(print_dl_program(Program, Bindings, Printed)),
    once(sub_atom(Printed, _, _, _,
                  'value: namespace.types.box(list(int))')),
    once(parse_braced_source(Printed, Reparsed, _)),
    Program =@= Reparsed.

test(an_imported_deep_generic_constructor_resolves_through_its_mount_path) :-
    expand_use_sources(
        [ "lib.dl6" = "rel namespace.types.box(T)(value: T).\n",
          "main.dl6" = "use \"lib.dl6\" as library.\nrel holder(value: library.namespace.types.box(int)).\n" ],
        'main.dl6',
        Program),
    once(expand_program(Program, prog(Decls, _), _)),
    once(( member(type_decl(Concrete, [col(value, int)]), Decls),
           sub_atom(Concrete, 0, _, _,
                    '__gen__namespace__types__box_int_') )),
    memberchk(col_type(holder/1, value, Concrete), Decls).

target_bundle(Text,
              bundle(Plan, Lowered, Boot, Ts, Rust, TsTypes, RustTypes,
                     Schema, Openapi)) :-
    parse_braced_source(Text, Program, Bindings),
    program_plan(fixture(braced_nested_equivalence, Program, [], [], [])-Bindings,
                 [intern(direct)],
                 Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, Rules), Types, RelPlans, _, _, _, _, Mode),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Mode, Decls, Types, RelPlans, [], LevelStatements, Boot),
    emit_program(braced_nested_equivalence, Plan, Lowered, Boot, Ts),
    emit_rust_program(braced_nested_equivalence, Plan, Lowered, Boot, Rust),
    catalog_decl_rows(braced_nested_equivalence, Rules, RelPlans, Decls,
                      Rows, _),
    ts_types_text(braced_nested_equivalence, Rows, TsTypes),
    rust_types_text(braced_nested_equivalence, Rows, RustTypes),
    jsonschema_text(braced_nested_equivalence, Rows, Schema),
    openapi_text(braced_nested_equivalence, Rows, Openapi).

test(brace_and_dotted_programs_emit_identical_target_artifacts) :-
    brace_equivalent_sources(Brace, Dotted),
    once(target_bundle(Brace, BraceBundle)),
    once(target_bundle(Dotted, DottedBundle)),
    BraceBundle == DottedBundle.

:- end_tests(braced_nested_relations).
