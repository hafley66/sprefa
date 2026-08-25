:- begin_tests(type_relation_ir).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- use_module('../../1_expansion/0_generic_expand',
              [ schema_member_rows/2, type_relation_rows/2,
                expand_generic_program/2, expand_generic_program_raw/2,
                expand_generic_program_with_bindings/3,
                freeze_type_rows/2, normalize_key_wrappers/2 ]).
:- use_module('../../lower', [ catalog_type_rows/6,
                               catalog_type_relation_rows/3,
                               catalog_type_transport_rows/4,
                               lower_program/2 ]).
:- use_module('../../compile', [ program_plan/2 ]).
:- use_module('../../compile', [ compile_dl6/2 ]).
:- use_module('../../3_analyze/0_rel_record', [ relplan_shape/6 ]).
:- use_module('../../compile/parse_dl_dcg', [ parse_dl/4 ]).
:- use_module('../../print_dl', [ print_dl_program/3 ]).
:- use_module('../../1_expansion/1_expansion', [ expand_program/3 ]).
:- use_module('../../conformance/engine', [ run_program/5 ]).
:- use_module('../../compile/typegen_export', []).
:- use_module('../../compile/8_emit_rust_types', [ rust_types_text/3 ]).
:- use_module('../../compile/4_emit_jsonschema', [ option_rows/3 ]).
:- use_module(library(process)).

ordinary_schema_decls([
    type_decl(person, [col(id, int), col(name, text)]),
    col_type(person/2, id, int),
    col_type(person/2, name, text),
    keyed(person/2, [1])
]).

test(key_wrapper_normalizes_to_ordered_relation_key) :-
    Decls0 = [ col_type(user/2, id, key(int)),
               col_type(user/2, name, key(text)) ],
    normalize_key_wrappers(Decls0, Decls),
    Decls = [ col_type(user/2, id, int),
              col_type(user/2, name, text),
              keyed(user/2, [1, 2]) ].

test(key_wrapper_and_legacy_key_deduplicate) :-
    Decls0 = [ col_type(user/2, id, key(int)),
               col_type(user/2, name, key(text)),
               keyed(user/2, [2, 1]) ],
    normalize_key_wrappers(Decls0, Decls),
    Decls = [ col_type(user/2, id, int),
              col_type(user/2, name, text),
              keyed(user/2, [1, 2]) ].

test(key_wrapper_generic_specialization_normalizes_after_substitution) :-
    Surface = prog([
        rel_template([user], [type_parameter('T', [])],
                      [column(id, key('T')), column(name, text)]),
        col_type(holder/1, value, user(text))
    ], []),
    expand_generic_program(Surface, prog(Decls, [])),
    member(col_type(Concrete/2, id, text), Decls),
    member(keyed(Concrete/2, [1]), Decls),
    member(col_type(Concrete/2, name, text), Decls).

test(key_wrapper_nested_is_named,
     [throws(unsupported_construct(key_wrapper_nested(user/1, 1)))]) :-
    normalize_key_wrappers([col_type(user/1, id, list(key(int)))], _).

test(key_wrapper_repeated_is_named,
     [throws(unsupported_construct(key_wrapper_repeated(user/1, 1)))]) :-
    normalize_key_wrappers([col_type(user/1, id, key(key(int)))], _).

test(key_wrapper_legacy_conflict_is_named,
     [throws(unsupported_construct(
          key_wrapper_legacy_conflict(user/2, [1], [2])))]) :-
    normalize_key_wrappers([col_type(user/2, id, key(int)),
                            col_type(user/2, name, text),
                            keyed(user/2, [2])], _).

test(key_wrapper_option_normalizes_to_existing_option_enum_key) :-
    expand_generic_program(
        prog([col_type(user/1, id, key(option(int)))], []), prog(Decls, [])),
    memberchk(col_type(user/1, id, '__opt_int'), Decls),
    memberchk(keyed(user/1, [1]), Decls),
    memberchk(option_column(user/1, id, int), Decls).

test(key_wrapper_relation_option_stays_in_the_owner_key) :-
    expand_generic_program(
        prog([col_type(person/1, name, text),
              col_type(user/1, parent, key(option(person)))], []),
        prog(Decls, [])),
    memberchk(col_type(user/1, parent, '__opt_person'), Decls),
    memberchk(keyed(user/1, [1]), Decls),
    memberchk(option_column(user/1, parent, person), Decls),
    \+ member(col_type(user__parent/2, _, _), Decls).

test(key_wrapper_print_reparse_canonicalizes_to_legacy_key) :-
    string_codes("rel user(id: key(int), name: text).\n", Codes),
    parse_dl(Codes, Program, Bindings, []),
    print_dl_program(Program, Bindings, Printed),
    string_codes(Printed, PrintedCodes),
    parse_dl(PrintedCodes, Reparsed, _, []),
    expansion:expand_program(Program, Expanded, _),
    expansion:expand_program(Reparsed, Reexpanded, _),
    Expanded =@= Reexpanded.

test(key_wrapper_legacy_plan_and_lowering_are_exactly_equal) :-
    Wrapper = fixture(key_wrapper_parity,
                      prog([col_type(user/2, id, key(int)),
                            col_type(user/2, name, text)], []),
                      [], [], []),
    Legacy = fixture(key_wrapper_parity,
                     prog([col_type(user/2, id, int),
                           col_type(user/2, name, text),
                           keyed(user/2, [1])], []),
                     [], [], []),
    program_plan(Wrapper-[], WrapperPlan),
    program_plan(Legacy-[], LegacyPlan),
    WrapperPlan =@= LegacyPlan,
    lower_program(WrapperPlan, WrapperLowered),
    lower_program(LegacyPlan, LegacyLowered),
    WrapperLowered =@= LegacyLowered.

test(key_wrapper_reuses_rel_key_ddl_and_edge_upsert) :-
    Program = fixture(key_wrapper_plan,
                      prog([ col_type(user/2, id, key(int)),
                             col_type(user/2, name, text),
                             col_type(source/2, id, int),
                             col_type(source/2, name, text) ],
                            [(user(Id, Name) <+ source(Id, Name))]),
                      [], [], []),
    program_plan(Program-[], Plan),
    Plan = plan(_, prog(Decls, _), _, RelPlans, _, _, _, _, _),
    memberchk(keyed(user/2, [1]), Decls),
    relplan_shape(RelPlans, user/2, set, [id, name], key([1]),
                  [int, text]),
    lower_program(Plan, lowered(_, Ddl, _, EdgeStatements, _, _, _, _)),
    once(( member(Statement, Ddl),
           sub_atom(Statement, _, _, _, 'UNIQUE ("id")') )),
    once(( member(edgestmt(user/2, source/2, _, [id], _, UpsertSql, _, _, _),
                  EdgeStatements),
           sub_atom(UpsertSql, _, _, _, 'ON CONFLICT("id") DO UPDATE') )).

test(key_wrapper_replacement_then_old_row_retraction_matches_legacy) :-
    Wrapper = prog([col_type(user/2, id, key(int)),
                    col_type(user/2, name, text)], []),
    Legacy = prog([col_type(user/2, id, int),
                   col_type(user/2, name, text),
                   keyed(user/2, [1])], []),
    Schedule = [[+user(1, a)], [+user(1, b)], [-user(1, a)]],
    expand_program(Wrapper, ExpandedWrapper, _),
    expand_program(Legacy, ExpandedLegacy, _),
    run_program(ExpandedWrapper, [], Schedule, WrapperFinal, _),
    run_program(ExpandedLegacy, [], Schedule, LegacyFinal, _),
    WrapperFinal == [user(1, b)],
    LegacyFinal == [user(1, b)],
    WrapperFinal == LegacyFinal.

test(ordinary_members_keep_authored_and_value_type_ids) :-
    ordinary_schema_decls(Decls),
    schema_member_rows(Decls, Rows),
    once(member(schema_member(Id, Owner, 1, id, int, primitive(int), [key]), Rows)),
    once(member(schema_member(NameId, Owner, 2, name, text, primitive(text), []), Rows)),
    Id \== NameId,
    Owner = named(local, relation, person).

test(ordinary_relation_does_not_require_self) :-
    ordinary_schema_decls(Decls),
    type_relation_rows(Decls, Rows),
    member(type_relation(named(local, relation, person), none, Inputs,
                        none, Keys), Rows),
    Inputs = [member(named(local, relation, person), 1, id),
              member(named(local, relation, person), 2, name)],
    Keys = [member(named(local, relation, person), 1, id)].

test(anonymous_owner_role_retains_path) :-
    Decls = [type_decl(outer,
                       [col(value, anonymous_product(path, text))])],
    schema_member_rows(Decls, Rows),
    once(member(schema_member(_, _, 1, value, anonymous_product(path, text),
                         _, [anonymous_owner(path)]), Rows)), !.

self_schema_decls([
    type_decl('Convert', [col('Self', type), col('Input', text),
                          col(return, text)]),
    keyed('Convert'/3, [1, 2])
]).

test(trait_relation_projects_self_inputs_return_and_keys) :-
    self_schema_decls(Decls),
    type_relation_rows(Decls, Rows),
    once(member(schema_member(SelfId, Owner, 1, 'Self', type,
                         primitive(type), [self_subject, key]), Rows)),
    once(member(schema_member(_, Owner, 2, 'Input', text, primitive(text),
                         [key]), Rows)),
    once(member(schema_member(ReturnId, Owner, 3, return, text,
                         primitive(text), [return]), Rows)),
    once(member(type_relation(Owner, SelfId, [InputId], ReturnId,
                              [SelfId, InputId]), Rows)),
    InputId = member(Owner, 2, 'Input').

test(type_relation_without_self_remains_metadata) :-
    Decls = [type_decl('Convert', [col('Input', type)])],
    type_relation_rows(Decls, Rows),
    member(type_relation(_, none, [_], none, []), Rows).

test(type_relation_duplicate_self_remains_metadata) :-
    Decls = [type_decl('Convert', [col('Self', type), col('Self', type)])],
    type_relation_rows(Decls, Rows),
    member(type_relation(_, Self, _, none, []), Rows),
    Self \== none.

test(type_relation_self_position_remains_metadata) :-
    Decls = [type_decl('Convert', [col('Input', text), col('Self', type)])],
    type_relation_rows(Decls, Rows),
    member(type_relation(_, Self, _, none, []), Rows),
    Self = member(_, 2, 'Self').

test(type_relation_self_value_type_remains_metadata) :-
    Decls = [type_decl('Convert', [col('Self', text), col('Input', text)])],
    type_relation_rows(Decls, Rows),
    member(type_relation(_, Self, _, none, []), Rows),
    Self = member(_, 1, 'Self').

test(catalog_metadata_is_the_same_parallel_stream) :-
    ordinary_schema_decls(Decls),
    catalog_type_relation_rows(type_relation_fixture, Decls, CatalogRows),
    type_relation_rows(Decls, DirectRows),
    CatalogRows =@= DirectRows.

test(catalog_transport_uses_typed_child_rows) :-
    ordinary_schema_decls(Decls),
    CatalogRows = [ row(10, 0, 0, person, rel, 0, 0, 0, '', '', ''),
                    row(11, 10, 0, id, column, 1, 0, 0, '', '', ''),
                    row(12, 10, 1, name, column, 2, 0, 0, '', '', '') ],
    catalog_type_transport_rows(type_relation_fixture, CatalogRows, Decls,
                                TransportRows),
    once(member(schema_member_column(11, MemberId), TransportRows)),
    once(member(schema_member_role(MemberId, 1, key, ''), TransportRows)),
    once(member(type_relation_key(_, 1, MemberId), TransportRows)), !.

test(rust_omits_self_subject_from_struct_fields) :-
    Rows = [ row(1, 0, 0, text, primitive, 0, 0, 0, '', '', ''),
             row(2, 0, 0, doc, module, 0, 0, 0, 'hash', '', ''),
             row(3, 2, 0, convert, rel, 0, 0, 1, '', '', ''),
             row(4, 3, 0, 'Self', column, 1, 0, 1, '', '', ''),
             row(5, 3, 1, input, column, 1, 0, 1, '', '', ''),
             row(6, 2, 1, other, rel, 0, 0, 1, '', '', ''),
             row(7, 6, 0, 'Self', column, 1, 0, 1, '', '', ''),
             schema_member(member(convert, 1, 1, 'Self'),
                           named(local, relation, convert), 1, 'Self', type,
                           primitive(type), [self_subject]),
             schema_member(member(convert, 1, 2, input),
                           named(local, relation, convert), 2, input, text,
                           primitive(text), []),
             schema_member_column(4, member(convert, 1, 1, 'Self')),
             schema_member_column(5, member(convert, 1, 2, input)),
             schema_member_column(7, member(other, 1, 1, 'Self')),
             schema_member_role(member(convert, 1, 1, 'Self'), 1,
                                self_subject, '') ],
    once(rust_types_text(doc, Rows, Text)),
    sub_string(Text, _, _, _, 'pub struct Convert {'),
    sub_string(Text, _, _, _, 'pub input: String,'),
    sub_string(Text, _, _, _, 'pub struct Other {'),
    sub_string(Text, _, _, _, 'pub Self: String,'),
    \+ sub_string(Text, _, _, _, "pub struct Convert {\n    pub Self:"), !.

test(typegen_metadata_round_trips_as_terms) :-
    Rows = [ schema_member(
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 1, 'Self', 'type',
                 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc'),
             schema_member_role(
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 1, 'self_subject', ''),
             schema_member_column(
                 4,
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'),
             type_relation(
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 ''),
             type_relation_input(
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 1,
                 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd'),
             type_relation_key(
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 1,
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa') ],
    Path = '/private/tmp/dl6-type-relation-roundtrip.jsonl',
    setup_call_cleanup(
        open(Path, write, Stream),
        forall(member(Row, Rows), typegen_export:write_row_line(Stream, Row)),
        close(Stream)),
    typegen_export:read_row_lines(Path, RoundTripped),
    RoundTripped =@= Rows, !.

test(typegen_evidence_round_trips_as_terms) :-
    Rows = [ type_relation_evidence(
                 named(local, relation, 'Convert'),
                 convert(named(local, relation, 'Document'),
                         named(local, relation, 'Format'), primitive(text))) ],
    Path = '/private/tmp/dl6-type-relation-evidence-roundtrip.jsonl',
    setup_call_cleanup(
        open(Path, write, Stream),
        forall(member(Row, Rows), typegen_export:write_row_line(Stream, Row)),
        close(Stream)),
    typegen_export:read_row_lines(Path, RoundTripped),
    RoundTripped = [type_relation_evidence(OwnerText, Evidence)],
    atom_length(OwnerText, 64),
    Evidence = convert(named(local, relation, 'Document'),
                       named(local, relation, 'Format'), primitive(text)), !.

associated_scalar_rows([
    type_relation(named(local, relation, 'Convert'),
                  member(named(local, relation, 'Convert'), 1, 'Self'),
                  [member(named(local, relation, 'Convert'), 2, 'Input')],
                  member(named(local, relation, 'Convert'), 3, return),
                  [ member(named(local, relation, 'Convert'), 1, 'Self'),
                    member(named(local, relation, 'Convert'), 2, 'Input') ]),
    schema_member(member(named(local, relation, 'Convert'), 1, 'Self'),
                  named(local, relation, 'Convert'), 1, 'Self', key(type),
                  primitive(type), [self_subject, key]),
    schema_member(member(named(local, relation, 'Convert'), 2, 'Input'),
                  named(local, relation, 'Convert'), 2, 'Input', key(type),
                  primitive(type), [key]),
    schema_member(member(named(local, relation, 'Convert'), 3, return),
                  named(local, relation, 'Convert'), 3, return, type,
                  primitive(type), [return])
]).

test(rust_trait_consumes_self_and_projects_scalar_output) :-
    associated_scalar_rows(Rows),
    rust_types_text(doc, Rows, Text),
    sub_string(Text, _, _, _, 'pub trait Convert<Input> {'),
    sub_string(Text, _, _, _, '    type Output;'),
    \+ sub_string(Text, _, _, _, 'pub struct Convert'),
    \+ sub_string(Text, _, _, _, 'pub Self:').

test(rust_marker_trait_keeps_nonfunctional_relation) :-
    Rows = [ type_relation(named(local, relation, 'Codec'),
                           member(named(local, relation, 'Codec'), 1, 'Self'),
                           [member(named(local, relation, 'Codec'), 2, 'Format')],
                           none, []),
             schema_member(member(named(local, relation, 'Codec'), 1, 'Self'),
                           named(local, relation, 'Codec'), 1, 'Self', type,
                           primitive(type), [self_subject]),
             schema_member(member(named(local, relation, 'Codec'), 2, 'Format'),
                           named(local, relation, 'Codec'), 2, 'Format', type,
                           primitive(type), []) ],
    rust_types_text(doc, Rows, Text),
    sub_string(Text, _, _, _, 'pub trait Codec<Format> {}').

test(rust_trait_projects_anonymous_product_fields) :-
    associated_scalar_rows(ScalarRows),
    ScalarRows = [Relation | _],
    Relation = type_relation(Owner, Self, Inputs, Return, Keys),
    Return = member(Owner, 3, return),
    Product = named(local, relation, pair),
    maplist(replace_return_type(Return, Product), ScalarRows, BaseRows),
    append(BaseRows,
           [ type_relation(Product, none,
                           [member(Product, 1, a), member(Product, 2, b)],
                           none, []),
             schema_member(member(Product, 1, a), Product, 1, a, int,
                           primitive(int), []),
             schema_member(member(Product, 2, b), Product, 2, b, text,
                           primitive(text), []) ],
           Rows),
    rust_types_text(doc, Rows, Text),
    sub_string(Text, _, _, _, 'type A;'),
    sub_string(Text, _, _, _, 'type B;'),
    \+ sub_string(Text, _, _, _, 'type Output;'),
    _ = [Self, Inputs, Keys].

test(rust_trait_refuses_self_outside_type_domain,
     [throws(unsupported_construct(associated_output_self_domain('Convert')))]) :-
    associated_scalar_rows(Rows0),
    select(schema_member(Self, Owner, Position, 'Self', _Authored, ValueType,
                         Roles), Rows0, Rest),
    append([schema_member(Self, Owner, Position, 'Self', text, ValueType,
                          Roles)], Rest, Rows),
    rust_types_text(doc, Rows, _).

test(rust_trait_refuses_duplicate_self_member,
     [throws(unsupported_construct(associated_output_self_duplicate('Convert')))]) :-
    associated_scalar_rows(Rows0),
    Rows0 = [type_relation(Owner, _Self, _Inputs, _Return, _Keys) | _],
    append(Rows0,
           [schema_member(member(Owner, 4, 'Self'), Owner, 4, 'Self', type,
                          primitive(type), [self_subject])],
           Rows),
    rust_types_text(doc, Rows, _).

replace_return_type(ReturnMember, Product, Row, Replaced) :-
    ( Row = schema_member(ReturnMember, Owner, Position, return, _Authored,
                          _Value, Roles)
    -> Replaced = schema_member(ReturnMember, Owner, Position, return, Product,
                                Product, Roles)
    ;  Replaced = Row
    ).

test(rust_trait_refuses_missing_functional_return,
     [throws(unsupported_construct(associated_output_missing_return('Convert')))]) :-
    associated_scalar_rows(Rows),
    select(type_relation(Owner, Self, Inputs, _Return, _Keys), Rows, Rest),
    append([type_relation(Owner, Self, Inputs, none, [Self | Inputs])], Rest,
           Broken),
    rust_types_text(doc, Broken, _).

test(rust_trait_refuses_nonfunctional_selector,
     [throws(unsupported_construct(associated_output_nonfunctional('Convert')))]) :-
    associated_scalar_rows(Rows),
    select(type_relation(Owner, Self, Inputs, Return, _Keys), Rows, Rest),
    append([type_relation(Owner, Self, Inputs, Return, [])], Rest, Broken),
    rust_types_text(doc, Broken, _).

test(rust_trait_refuses_post_normalization_output_collision,
     [throws(unsupported_construct(
         associated_output_name_collision('Convert', AB)))]) :-
    associated_scalar_rows(ScalarRows),
    ScalarRows = [Relation | _],
    Relation = type_relation(Owner, Self, Inputs, Return, Keys),
    Product = named(local, relation, pair),
    maplist(replace_return_type(Return, Product), ScalarRows, BaseRows),
    append(BaseRows,
           [ type_relation(Product, none,
                           [member(Product, 1, a_b), member(Product, 2, 'AB')],
                           none, []),
             schema_member(member(Product, 1, a_b), Product, 1, a_b, int,
                           primitive(int), []),
             schema_member(member(Product, 2, 'AB'), Product, 2, 'AB', text,
                           primitive(text), []) ],
           Rows),
    rust_types_text(doc, Rows, _),
    _ = [Owner, Self, Inputs, Return, Keys, AB].

test(rust_trait_emits_only_complete_compiler_evidence_impl) :-
    associated_scalar_rows(ScalarRows),
    append(ScalarRows,
           [ type_relation_evidence(
                 named(local, relation, 'Convert'),
                 convert(named(local, relation, document),
                         named(local, relation, format), primitive(text))) ],
           Rows),
    rust_types_text(doc, Rows, Text),
    sub_string(Text, _, _, _,
               'impl Convert<Format> for Document {\n    type Output = String;'),
    \+ sub_string(Text, _, _, _, 'impl Convert<Input> for').

test(rust_evidence_uses_module_qualified_same_name_relation_types) :-
    associated_scalar_rows(ScalarRows),
    append(ScalarRows,
           [ type_relation(named(left_module, relation, document), none,
                           [], none, []),
             type_relation(named(right_module, relation, document), none,
                           [], none, []),
             type_relation(named(local, relation, format), none,
                           [], none, []),
             type_relation_owner(named(left_module, relation, document),
                                 left_module, document),
             type_relation_owner(named(right_module, relation, document),
                                 right_module, document),
             type_relation_owner(named(local, relation, format), local, format),
             type_relation_evidence(
                 named(local, relation, 'Convert'),
                 convert(named(left_module, relation, document),
                         named(local, relation, format), primitive(text))) ],
           Rows),
    rust_types_text(doc, Rows, Text),
    sub_string(Text, _, _, _,
               'impl Convert<Format> for LeftModuleDocument {\n    type Output = String;').

test(module_ambiguous_compiler_evidence_does_not_cross_owner_boundary) :-
    associated_scalar_rows(MetadataRows),
    append([ [rel_module_decl('Convert', left_module),
              rel_module_decl('Convert', right_module)],
             [compiler_type_metadata(
                  MetadataRows,
                  [convert(named(left_module, relation, document),
                           named(left_module, relation, format),
                           primitive(text))])]],
           Decls),
    type_relation_rows(Decls, Rows),
    \+ member(type_relation_evidence(_, _), Rows).

test(authored_dl6_rust_renderer_emits_and_compiles_product_outputs) :-
    predicate_property(plunit_type_relation_ir:associated_scalar_rows(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/typegen/render_rust.dl6', Renderer,
                       [relative_to(TestDir), access(read)]),
    compile_dl6(Renderer, '/private/tmp/dl6-render-rust-door.ts'),
    read_file_to_string(Renderer, Source, []),
    string_codes(Source, SourceCodes),
    parse_dl(SourceCodes, Program, _, []),
    Initial = [
        type_relation(project, self, return),
        type_relation_input(project, 1, input),
        type_relation_key(project, 1, self),
        type_relation_key(project, 2, input),
        type_relation_owner(project, local, project),
        type_relation_rust_name(project, 'Project'),
        schema_member(self, project, 1, 'Self', type, type),
        schema_member_role(self, 1, self_subject, ''),
        schema_member(input, project, 2, 'Input', type, type),
        schema_member(return, project, 3, return, pair, pair),
        type_relation(pair, '', ''),
        schema_member(left, pair, 1, 'Self', int, int),
        schema_member(right, pair, 2, right, text, text),
        type_relation_evidence(project, evidence),
        type_relation_rust_impl(project,
          'impl Project<Format> for Document {\n    type SelfValue = i64;\n    type Right = String;\n}\n')
    ],
    run_program(Program, Initial, [], Final, _),
    member(rendered_type('Project', 0, 0, TraitText), Final),
    sub_string(TraitText, _, _, _, 'type SelfValue;'),
    sub_string(TraitText, _, _, _, 'type Right;'),
    member(rendered_type('EvidenceImpl', 1, 0, ImplText), Final),
    RustPath = '/private/tmp/dl6-render-rust-associated-product.rs',
    setup_call_cleanup(
        open(RustPath, write, Stream),
        ( format(Stream, 'pub struct Document;\npub struct Format;\n~s~s',
                 [TraitText, ImplText]) ),
        close(Stream)),
    process_create(path(rustc),
                   [RustPath, '--crate-type=lib', '--edition=2021',
                    '-o', '/private/tmp/dl6-render-rust-associated-product.rlib'],
                   [process(Pid)]),
    process_wait(Pid, exit(0)).

test(real_dl6_fixture_reaches_rust_typegen) :-
    predicate_property(plunit_type_relation_ir:associated_scalar_rows(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/rust-associated-outputs.dl6',
                       Fixture, [relative_to(TestDir), access(read)]),
    read_file_to_string(Fixture, Source, []),
    string_codes(Source, SourceCodes),
    parse_dl(SourceCodes, Program, Bindings, []),
    program_plan(fixture(rust_associated_outputs, Program, [], [], [])-
                 Bindings, Plan),
    Plan = plan(_, prog(Decls, Rules), _Types, RelPlans, _, _, _, _, Mode),
    catalog_type_rows(Mode, rust_associated_outputs, Rules, RelPlans,
                      Decls, CatalogRows0),
    option_rows(Decls, CatalogRows0, CatalogRows),
    catalog_type_relation_rows(rust_associated_outputs, Decls, RelationRows),
    catalog_type_transport_rows(rust_associated_outputs, CatalogRows, Decls,
                                ChildRows),
    append([CatalogRows, RelationRows, ChildRows], Rows),
    rust_types_text(rust_associated_outputs, Rows, Text),
    sub_string(Text, _, _, _, 'pub trait Convert<Input> {'),
    sub_string(Text, _, _, _,
               'impl Convert<Format> for Document {\n    type Output = String;'),
    \+ sub_string(Text, _, _, _, 'pub Self:'),
    Jsonl = '/private/tmp/rust-associated-outputs.types.jsonl',
    typegen_export:dump_type_rows(Plan, Jsonl),
    typegen_export:read_row_lines(Jsonl, RoundTrippedRows),
    member(type_relation_rust_impl(_, ImplText), RoundTrippedRows),
    sub_string(ImplText, _, _, _, 'impl Convert<Format> for Document'),
    rust_types_text(rust_associated_outputs, RoundTrippedRows,
                    RoundTrippedText),
    sub_string(RoundTrippedText, _, _, _, 'pub trait Convert<Input> {'),
    sub_string(RoundTrippedText, _, _, _,
               'impl Convert<Format> for Document {\n    type Output = String;').

test(canonical_freeze_typespec_probe_compiles) :-
    predicate_property(plunit_type_relation_ir:associated_scalar_rows(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/0_typespec_basic_probe.dl6',
                       Fixture, [relative_to(TestDir), access(read)]),
    Output = '/private/tmp/canonical-freeze-typespec-probe.ts',
    setup_call_cleanup(
        true,
        compile_dl6(Fixture, Output),
        ( exists_file(Output) -> delete_file(Output) ; true )
    ).

test(canonical_freeze_adds_concrete_generic_member_rows) :-
    string_codes("rel Box(T)(value: T).\nrel use(box: Box(int)).\n", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    member(declaration(ConcreteId, root, ConcreteName, relation, materialized), Rows),
    sub_atom(ConcreteName, 0, _, _, '__gen__Box'),
    memberchk(member(_, ConcreteId, 1, value, type_ref(primitive(int))), Rows).

test(canonical_freeze_adds_anonymous_product_and_sum_member_rows) :-
    string_codes(
        "rel Holder(value: (a: int, b: text), choice: (Yes(code: int); No(message: text))).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    member(anonymous(_, [value], product_type([field(a, int), field(b, text)])), Rows),
    member(derived_from(ProductId,
                        anonymous(_, [value],
                                  product_type([field(a, int), field(b, text)]))), Rows),
    memberchk(member(_, ProductId, 1, a, type_ref(primitive(int))), Rows),
    memberchk(member(_, ProductId, 2, b, type_ref(primitive(text))), Rows),
    member(anonymous(_, [choice],
                     sum_type([variant('Yes', [field(code, int)]),
                               variant('No', [field(message, text)])])), Rows),
    member(derived_from(EnumId,
                        anonymous(_, [choice],
                                  sum_type([variant('Yes', [field(code, int)]),
                                            variant('No', [field(message, text)])]))), Rows),
    member(member(_, EnumId, 1, 'Yes', type_ref(declaration(YesId))), Rows),
    memberchk(member(_, YesId, 2, code, type_ref(primitive(int))), Rows).

test(canonical_freeze_retains_nested_wrapper_applications_and_arguments) :-
    string_codes("rel Holder(value: option(list(text))).\n", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    member(member(_, named(local, relation, 'Holder'), 1, value,
                  type_ref(application(OptionId))), Rows),
    memberchk(application(OptionId, named(local, relation, option)), Rows),
    memberchk(argument(_, OptionId, 1, type_application(ListId)), Rows),
    memberchk(application(ListId, named(local, relation, list)), Rows),
    memberchk(argument(_, ListId, 1, type_atom(text)), Rows).

test(canonical_freeze_adds_option_enum_variant_member_rows) :-
    string_codes("rel Holder(value: option(text)).\n", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    member(declaration(OptionId, root, '__opt_text', enum, compile_time), Rows),
    member(derived_from(SomeId, OptionId), Rows),
    memberchk(member(_, SomeId, 1, id, type_ref(primitive(int))), Rows),
    memberchk(member(_, SomeId, 2, value, type_ref(primitive(text))), Rows).

test(canonical_freeze_retains_named_enum_member_identity) :-
    string_codes("rel Holder(value: Status).\nrel Status(Ready(); Failed(message: text)).\n",
                 Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    memberchk(member(_, named(local, relation, 'Holder'), 1, value,
                     type_ref(declaration(named(local, enum, 'Status')))), Rows).

test(canonical_freeze_keeps_module_qualified_generated_member_identity) :-
    Program = prog(
        [ semantic_decl_module(relation, 'Box', module_a),
          rel_template(['Box'], [type_parameter('T', [])],
                       [column(value, 'T')]),
          col_type(use/1, box, 'Box'(int)) ],
        []),
    expand_program(Program, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    member(declaration(ConcreteId, root, ConcreteName, relation, materialized), Rows),
    ConcreteId = named(module_a, relation, ConcreteName),
    sub_atom(ConcreteName, 0, _, _, '__gen__Box'),
    memberchk(member(_, ConcreteId, 1, value, type_ref(primitive(int))), Rows).

test(canonical_freeze_refuses_conflicting_member_identity,
     [throws(unsupported_construct(canonical_type_row_duplicate(
                 member(named(local, relation, holder), 1, value))))]) :-
    Owner = named(local, relation, holder),
    MemberId = member(Owner, 1, value),
    freeze_type_rows(
        [ semantic_type_rows(
            [ member(MemberId, Owner, 1, value, type_ref(primitive(int))),
              member(MemberId, Owner, 1, value, type_ref(primitive(text))) ]) ],
        _).

test(canonical_freeze_has_one_solution) :-
    Decls = [ kind(reading/2, set),
              col_type(reading/2, sensor_name, text) ],
    findall(Frozen, freeze_type_rows(Decls, Frozen), Solutions),
    Solutions = [_].

test(canonical_freeze_compiler_and_oracle_rows_are_equal) :-
    Program = prog(
        [ rel_template([box], [type_parameter('T', [])],
                       [column(value, 'T')]),
          col_type(holder/1, value, box(int)) ],
        []),
    expand_program(Program, prog(OracleDecls, _), _),
    memberchk(semantic_type_rows(OracleRows), OracleDecls),
    program_plan(fixture(canonical_type_rows, Program, [], [], [])-[],
                 plan(_, prog(CompilerDecls, _), _, _, _, _, _, _, _)),
    memberchk(semantic_type_rows(CompilerRows), CompilerDecls),
    CompilerRows == OracleRows.

test(canonical_freeze_annotation_rows_survive_carrier_free_refreeze) :-
    predicate_property(plunit_type_relation_ir:associated_scalar_rows(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/type-annotation-ci.dl6', Fixture,
                       [relative_to(TestDir), access(read)]),
    read_file_to_string(Fixture, Source, []),
    string_codes(Source, Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(FrozenRows), Decls),
    memberchk(member(_, named(local, relation, 'AnnotationCoverage'), 2,
                     configured, type_ref(primitive(text))), FrozenRows),
    exclude(canonical_freeze_carrier, Decls, CarrierFree),
    freeze_type_rows(CarrierFree, Refrozen),
    memberchk(semantic_type_rows(RefrozenRows), Refrozen),
    RefrozenRows == FrozenRows.

canonical_freeze_carrier(type_decl(_, _)).
canonical_freeze_carrier(col_type(_, _, _)).
canonical_freeze_carrier(keyed(_, _)).
canonical_freeze_carrier(compiler_type_metadata(_, _)).
canonical_freeze_carrier(compiler_type_metadata(_, _, _)).
canonical_freeze_carrier(compiler_annotation_evidence(_)).

test(canonical_freeze_json_list_application_is_closed) :-
    string_codes("rel Holder(values: json_list(text)).\n", Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    member(member(_, named(local, relation, 'Holder'), 1, values,
                  type_ref(application(AppId))), Rows),
    memberchk(application(AppId, named(local, relation, json_list)), Rows),
    memberchk(argument(_, AppId, 1, type_atom(text)), Rows).

test(canonical_freeze_relation_id_application_is_closed) :-
    string_codes(
        "rel Revision(oid: text).\nrel Batch(revisions: list(Revision.id)).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    expansion:expand_program_with_bindings(Program, Bindings, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    RevisionId = named(local, relation, 'Revision'),
    IdApp = application(named(local, relation, id), [RevisionId]),
    ListApp = application(named(local, relation, list), [IdApp]),
    memberchk(application(IdApp, named(local, relation, id)), Rows),
    memberchk(argument(_, IdApp, 1, type_atom('Revision')), Rows),
    memberchk(application(ListApp, named(local, relation, list)), Rows),
    memberchk(argument(_, ListApp, 1, type_application(IdApp)), Rows).

test(canonical_freeze_imported_generic_enum_keeps_enum_identity) :-
    Program = prog(
        [ semantic_decl_module(enum, 'Result', foreign),
          rel_template_enum(['Result'], [type_parameter('E', []),
                                         type_parameter('T', [])],
                            (err(error: 'E') ; ok(value: 'T'))),
          col_type(holder/1, value, 'Result'(text, int)) ],
        []),
    expand_program(Program, prog(Decls, _), _),
    memberchk(semantic_type_rows(Rows), Decls),
    memberchk(application(AppId, named(foreign, enum, 'Result')), Rows),
    memberchk(member(_, named(local, relation, holder), 1, value,
                     type_ref(application(AppId))), Rows),
    member(declaration(ConcreteId, root, ConcreteName, enum, compile_time), Rows),
    ConcreteId = named(foreign, enum, ConcreteName),
    sub_atom(ConcreteName, 0, _, _, '__gen__Result').

test(type_application_reuses_existing_semantic_identity) :-
    Constructor = named(local, relation, box),
    Application = application(Constructor, [primitive(int)]),
    Rows = [ declaration(Constructor, root, box, relation, compile_time),
             application(Application, Constructor),
             argument(arg(Application, 1), Application, 1, type_atom(int)) ],
    freeze_type_rows([semantic_type_rows(Rows)], Frozen),
    memberchk(semantic_type_rows(FrozenRows), Frozen),
    findall(Id, member(application(Id, _), FrozenRows), Ids),
    Ids == [Application].

test(type_application_refreeze_constructs_nested_applications) :-
    Constructor = named(local, relation, 'Box'),
    Inner = application(named(local, relation, list), [primitive(int)]),
    Outer = application(Constructor, [Inner]),
    string_codes("rel Box(T)(value: T).\nrel Holder(value: Box(list(int))).\n",
                 Codes),
    parse_dl(Codes, Program, Bindings, []),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, _)),
    memberchk(semantic_type_rows(Rows), Decls),
    memberchk(application(Inner, named(local, relation, list)), Rows),
    memberchk(application(Outer, Constructor), Rows),
    memberchk(argument(_, Outer, 1, type_application(Inner)), Rows).

test(type_application_duplicate_requests_have_one_application_row) :-
    Constructor = named(local, relation, box),
    Application = application(Constructor, [primitive(int)]),
    Rows = [ declaration(Constructor, root, box, relation, compile_time),
             application(Application, Constructor),
             application(Application, Constructor),
             argument(arg(Application, 1), Application, 1, type_atom(int)),
             argument(arg(Application, 1), Application, 1, type_atom(int)) ],
    freeze_type_rows([semantic_type_rows(Rows)], Frozen),
    memberchk(semantic_type_rows(FrozenRows), Frozen),
    findall(application(Application, Candidate),
            member(application(Application, Candidate), FrozenRows),
            RowsForApp),
    RowsForApp = [application(Application, Constructor)].

test(canonical_freeze_type_annotation_fixture_compiles) :-
    predicate_property(plunit_type_relation_ir:associated_scalar_rows(_),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../dl/fixtures/type-annotation-ci.dl6',
                       Fixture, [relative_to(TestDir), access(read)]),
    Output = '/private/tmp/canonical-freeze-type-annotation-ci.ts',
    setup_call_cleanup(true, compile_dl6(Fixture, Output),
                       ( exists_file(Output) -> delete_file(Output) ; true )).

:- end_tests(type_relation_ir).
