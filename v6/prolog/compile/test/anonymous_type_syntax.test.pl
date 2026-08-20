% anonymous_type_syntax.test.pl : anonymous product/sum type syntax and identity.
%
% Covers the first slice of @anonymous-type-syntax: product_type/2 and
% sum_type/2 parse anywhere a type expression is legal, reach a
% parse/print/reparse fixpoint (term equality plus second-print byte equality),
% mint owner-scoped anonymous(Owner, SitePath, SpecializedShape) identities
% after generic substitution, materialize an ordinary type_decl/enum_decl, and
% name empty and cyclic diagnostics.

:- begin_tests(anonymous_type_syntax).

:- use_module('../../compile/parse_dl_dcg', [ parse_dl/4 ]).
:- use_module('../../print_dl', [ print_dl_program/3 ]).
:- use_module('../../1_expansion', [ expand_program_with_bindings/4 ]).
:- use_module('../../0_anonymous_expand', [ expand_anonymous_decls/2 ]).

:- op(1150, xfx, <-).

parse_text(Text, Prog, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Prog, Bindings, []).

% ── parse / print / reparse fixpoint ─────────────────────────────────────────

test(product_parses_prints_and_reparses_to_term_equality) :-
    parse_text("rel r(a: (x: int, y: text), b: (Ok(v: int); Err(m: text))).",
               Prog, Bindings),
    Prog = prog([col_type(r/2, a, product_type([field(x, int), field(y, text)])),
                 col_type(r/2, b, sum_type([variant('Ok', [field(v, int)]),
                                            variant('Err', [field(m, text)])]))],
                []),
    print_dl_program(Prog, Bindings, Text),
    atom_codes(Text, Printed),
    parse_dl(Printed, RoundTripped, _, []),
    Prog =@= RoundTripped.

test(product_second_print_is_byte_identical) :-
    parse_text("rel r(a: (x: int, y: text)).", Prog, Bindings),
    print_dl_program(Prog, Bindings, Text1),
    atom_codes(Text1, Printed1),
    parse_dl(Printed1, Prog2, Bindings2, []),
    print_dl_program(Prog2, Bindings2, Text2),
    Text2 == Text1.

test(nested_product_round_trips) :-
    parse_text("rel r(a: (x: int, y: (p: text, q: float))).", Prog, Bindings),
    print_dl_program(Prog, Bindings, Text),
    atom_codes(Text, Printed),
    parse_dl(Printed, RoundTripped, _, []),
    Prog =@= RoundTripped.

test(sum_payload_accepts_complete_type_expression) :-
    parse_text("rel r(a: (Ok(v: list(int)); Err(m: option(text)))).",
               Prog, Bindings),
    Prog = prog([col_type(r/1, a,
                          sum_type([variant('Ok', [field(v, list(int))]),
                                    variant('Err', [field(m, option(text))])]))],
                []),
    print_dl_program(Prog, Bindings, Text),
    atom_codes(Text, Printed),
    parse_dl(Printed, RoundTripped, _, []),
    Prog =@= RoundTripped.

test(arrow_return_position_round_trips) :-
    parse_text("rel r(a: int) -> (b: text, c: bool).", Prog, Bindings),
    print_dl_program(Prog, Bindings, Text),
    assertion(sub_string(Text, _, _, _, "return: (b: text, c: bool)")),
    atom_codes(Text, Printed),
    parse_dl(Printed, RoundTripped, _, []),
    Prog =@= RoundTripped.

test(inline_arrow_parses_prints_and_reparses) :-
    parse_text("rel Pet(name: text).\nrel Pets(get: ((id: int) -> Pet)).",
               Prog, Bindings),
    Prog = prog(Decls, []),
    memberchk(col_type('Pets'/1, get,
                       arrow_type([field(id, int)], 'Pet')), Decls),
    print_dl_program(Prog, Bindings, Text),
    assertion(sub_string(Text, _, _, _, "get: ((id: int) -> Pet)")),
    atom_codes(Text, Printed),
    parse_dl(Printed, RoundTripped, _, []),
    Prog =@= RoundTripped.

test(inline_arrow_is_recursive_in_every_type_expression_site) :-
    parse_text(
        "rel Pet(name: text).\nrel Box(T)(value: T).\nrel Sites(field: ((id: int) -> Pet), wrapped: list(((id: int) -> Pet)), generic: Box(((id: int) -> Pet)), product: (op: ((id: int) -> Pet)), sum: (One(op: ((id: int) -> Pet)); Empty())).",
        Prog, Bindings),
    print_dl_program(Prog, Bindings, Text),
    atom_codes(Text, Printed),
    parse_dl(Printed, RoundTripped, _, []),
    Prog =@= RoundTripped,
    expand_program_with_bindings(Prog, Bindings, prog(Expanded, _), _),
    member(semantic_type_rows(Rows), Expanded),
    findall(Path-Inputs-Output,
            member(anonymous(named(local, relation, 'Sites'), Path,
                             arrow_type(Inputs, Output)), Rows),
            Sites),
    member(_-Inputs-Output, Sites),
    append(Inputs, [field(return, Output)], Fields),
    member(field(return, 'Pet'), Fields),
    findall(Path, member(Path-_-_, Sites), Paths),
    Paths \== [].

test(inline_arrow_mints_an_ordinary_return_member_role) :-
    parse_text("rel Pet(name: text).\nrel Pets(get: ((id: int) -> Pet)).",
               Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, prog(Decls, _), _),
    member(col_type('Pets'/1, get, Generated), Decls),
    member(type_decl(Generated, [col(id, int), col(return, 'Pet')]), Decls),
    member(semantic_type_rows(Rows), Decls),
    member(declaration(GeneratedId, root, Generated, relation, materialized),
           Rows),
    ReturnMember = member(GeneratedId, 2, return),
    member(member(ReturnMember, GeneratedId, 2, return, _), Rows),
    member(member_role(ReturnMember, return), Rows).

test(empty_product_or_sum_is_named,
     [throws(unsupported_construct(anonymous_type_empty))]) :-
    parse_text("rel r(a: ()).", _, _).

test(sum_with_two_zero_field_variants_parses) :-
    parse_text("rel r(a: (Ok(); Err())).", Prog, _),
    Prog = prog([col_type(r/1, a,
                          sum_type([variant('Ok', []), variant('Err', [])]))],
                []).

% ── identity minting ─────────────────────────────────────────────────────────

test(product_mints_owner_scoped_identity_and_materializes_type_decl) :-
    parse_text("rel resident(input: text) -> (a: int, b: text).", Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, Expanded, _),
    Expanded = prog(Decls, _),
    once(member(semantic_type_rows(Rows), Decls)),
    once(member(anonymous(named(local, relation, resident), [return],
                          product_type([field(a, int), field(b, text)])), Rows)),
    once(member(declaration(GenId, root, GenName, relation, materialized),
                Rows)),
    GenName \== '',
    once(member(type_decl(GenName, [col(a, int), col(b, text)]), Decls)),
    once(member(col_type(resident/2, return, GenName), Decls)).

test(sum_mints_identity_and_enum_context_sees_it) :-
    parse_text("rel A(a: int, b: (Derp(value: int); Derpy(value: float))).",
               Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, Expanded, EnumContext),
    Expanded = prog(Decls, _),
    once(member(semantic_type_rows(Rows), Decls)),
    once(member(anonymous(named(local, relation, 'A'), [b],
                          sum_type([variant('Derp', [field(value, int)]),
                                    variant('Derpy', [field(value, float)])])),
                Rows)),
    % enum context carries the minted sum and its variant refs
    once(member(EnumName-_, EnumContext)),
    sub_atom(EnumName, 0, _, _, '__anon').

test(nested_product_mints_recursive_site_path) :-
    parse_text("rel r(a: (x: int, y: (p: text))).", Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, Expanded, _),
    Expanded = prog(Decls, _),
    once(member(semantic_type_rows(Rows), Decls)),
    once(member(anonymous(_, [a, y], product_type([field(p, text)])), Rows)),
    once(member(anonymous(_, [a], _Outer), Rows)).

test(generic_specialization_precedes_minting) :-
    parse_text("rel Box(T)(value: T).\nrel use(b: Box((a: int, b: text))).",
               Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, Expanded, _),
    Expanded = prog(Decls, _),
    once(member(semantic_type_rows(Rows), Decls)),
    % The owner is the CONCRETE instantiation (module-qualified, __gen__ name),
    % and the shape carries the specialized int/text, never the parameter T.
    once(member(anonymous(Owner, [value],
                          product_type([field(a, int), field(b, text)])), Rows)),
    Owner = named(_, relation, ConcreteName),
    sub_atom(ConcreteName, 0, _, _, '__gen__Box').

test(generic_semantic_rows_use_materialized_anonymous_identity) :-
    parse_text("rel Box(T)(value: T).\nrel use(b: Box((a: int, b: text))).",
               Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, prog(Decls, _), _),
    member(semantic_type_rows(Rows), Decls),
    once(member(anonymous(_, [value],
                          product_type([field(a, int), field(b, text)])),
                Rows)),
    once(member(declaration(AnonymousId, root, AnonymousName, relation,
                            materialized), Rows)),
    sub_atom(AnonymousName, 0, _, _, '__anon_'),
    once(member(application(ApplicationId, named(local, relation, 'Box')), Rows)),
    ApplicationId = application(named(local, relation, 'Box'), [AnonymousId]),
    memberchk(substitution(ApplicationId, _, AnonymousId), Rows),
    memberchk(argument(_, ApplicationId, 1, type_declaration(AnonymousId)), Rows),
    \+ ( member(Row, Rows), generic_argument_row(Row),
         sub_term(anonymous_placeholder(_), Row) ),
    \+ ( member(Row, Rows), generic_argument_row(Row),
         sub_term(product_type(_), Row) ).

generic_argument_row(application(_, _)).
generic_argument_row(well_formed(_)).
generic_argument_row(substitution(_, _, _)).
generic_argument_row(argument(_, _, _, _)).
generic_argument_row(member(_, _, _, _, _)).

test(module_qualified_owner_identity_uses_decl_module) :-
    Decls0 = [ semantic_decl_module(relation, resident, 'modhash42'),
               col_type(resident/2, input, text),
               col_type(resident/2, return, product_type([field(a, int)])),
               semantic_type_rows([]) ],
    expand_anonymous_decls(Decls0, Decls),
    once(member(semantic_type_rows(Rows), Decls)),
    once(member(anonymous(named('modhash42', relation, resident), [return],
                          product_type([field(a, int)])), Rows)),
    once(member(declaration(named('modhash42', relation, GeneratedName), root,
                            GeneratedName, relation, materialized), Rows)),
    memberchk(semantic_decl_module(relation, GeneratedName, 'modhash42'), Decls).

test(generic_generated_declaration_inherits_template_module) :-
    parse_text("rel Box(T)(value: T).\nrel use(b: Box((a: int))).",
               prog(Decls0, Rules), Bindings),
    Decls = [semantic_decl_module(relation, 'Box', module_a) | Decls0],
    expand_program_with_bindings(prog(Decls, Rules), Bindings, prog(Expanded, _), _),
    member(semantic_type_rows(Rows), Expanded),
    once(member(anonymous(named(module_a, relation, Concrete), [value],
                          product_type([field(a, int)])), Rows)),
    sub_atom(Concrete, 0, _, _, '__gen__Box'),
    once(member(declaration(named(module_a, relation, AnonymousName), root,
                            AnonymousName, relation, materialized), Rows)),
    memberchk(semantic_decl_module(relation, AnonymousName, module_a), Expanded).

test(type_decl_specs_are_rewritten_and_nested_declarations_materialize) :-
    Decls0 = [ semantic_decl_module(relation, outer, module_a),
               type_decl(outer,
                         [col(value,
                              product_type([field(child,
                                                  product_type([field(x, int)]))]))]),
               semantic_type_rows([]) ],
    expand_anonymous_decls(Decls0, Decls),
    member(type_decl(outer, [col(value, OuterGenerated)]), Decls),
    member(type_decl(OuterGenerated, [col(child, InnerGenerated)]), Decls),
    member(type_decl(InnerGenerated, [col(x, int)]), Decls),
    member(semantic_type_rows(Rows), Decls),
    memberchk(anonymous(named(module_a, relation, outer), [value],
                        product_type([field(child,
                                            product_type([field(x, int)]))])),
              Rows),
    memberchk(anonymous(named(module_a, relation, outer), [value, child],
                        product_type([field(x, int)])), Rows).

test(authored_anon_prefix_is_not_classified_as_generated) :-
    Decls0 = [ type_decl('__anon_authored', [col(value, int)]),
               col_type(outer/1, value, product_type([field(x, int)])),
               semantic_type_rows([]) ],
    expand_anonymous_decls(Decls0, Decls),
    memberchk(type_decl('__anon_authored', [col(value, int)]), Decls),
    \+ memberchk(anonymous_generated_decl('__anon_authored'), Decls).

test(identity_is_stable_under_unrelated_declaration_insertion) :-
    parse_text("rel resident(input: text) -> (a: int, b: text).", Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, Expanded, _),
    Expanded = prog(Decls, _),
    once(member(semantic_type_rows(Rows), Decls)),
    once(member(anonymous(Owner, [return],
                          product_type([field(a, int), field(b, text)])), Rows)),
    % A second, unrelated declaration must not change the identity.
    parse_text("rel resident(input: text) -> (a: int, b: text).\nrel unrelated(x: text).",
               Prog2, Bindings2),
    expand_program_with_bindings(Prog2, Bindings2, Expanded2, _),
    Expanded2 = prog(Decls2, _),
    once(member(semantic_type_rows(Rows2), Decls2)),
    once(member(anonymous(Owner2, [return],
                          product_type([field(a, int), field(b, text)])), Rows2)),
    Owner2 == Owner.

% ── cycle diagnostics ────────────────────────────────────────────────────────

test(unguarded_anonymous_cycle_is_named,
     [throws(unsupported_construct(anonymous_type_cycle(_, [next])))]) :-
    parse_text("rel node(next: (value: int, next: node)).", Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, _, _).

test(guarded_anonymous_cycle_materializes) :-
    Decls0 = [ col_type(node/1, next,
                         product_type([field(value, int),
                                       field(parent, option(node))])),
               semantic_type_rows([]) ],
    expand_anonymous_decls(Decls0, Decls),
    member(col_type(node/1, next, Generated), Decls),
    member(type_decl(Generated,
                     [col(value, int), col(parent, option(node))]), Decls).

% ── printer fixpoint on the generated decl ───────────────────────────────────

test(generated_decl_is_deterministic_across_two_mints) :-
    parse_text("rel r(a: (x: int, y: text)).", Prog, Bindings),
    expand_program_with_bindings(Prog, Bindings, Expanded1, _),
    Expanded1 = prog(Decls1, _),
    once(member(type_decl(Name1, Specs1), Decls1)),
    once(member(col_type(r/1, a, Name1), Decls1)),
    once(member(semantic_type_rows(Rows1), Decls1)),
    once(member(anonymous(Id1, [a], Shape1), Rows1)),
    parse_text("rel r(a: (x: int, y: text)).", Prog2, Bindings2),
    expand_program_with_bindings(Prog2, Bindings2, Expanded2, _),
    Expanded2 = prog(Decls2, _),
    once(member(semantic_type_rows(Rows2), Decls2)),
    once(member(anonymous(Id2, [a], Shape2), Rows2)),
    Id2 == Id1,
    Shape2 == Shape1.

:- end_tests(anonymous_type_syntax).
