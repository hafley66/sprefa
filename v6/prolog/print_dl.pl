% print_dl.pl : phase D printer, the inverse of parse_dl.pl. Takes a
% prog(Decls, Rules) term plus its Bindings (Name=Var pairs, the same shape
% compile.pl:read_fixture_term/4 and parse_dl.pl both produce) and renders
% canonical .dl6 TEXT that parse_dl.pl re-parses back to a variant of the
% original term (G1's round-trip grade).
%
% Column names for decl lines are MINED, not reinvented: this file calls
% analyze.pl's exported rel_kind/3, decl_key/3, decl_keep/3, declared_refs/2,
% program_refs/2, rel_columns/5, snake_name/2 directly (read-only use of an
% already-owned module, not a duplicate implementation -- the repo style law
% against reinventing a mining pass that already exists elsewhere).
%
% Syntax choices here are canonical per SYNTAX.md: `<-`/`<+` arrows, latest/
% pre/departed/now/decode/json_each/:= as function-call-shaped body items,
% infix arithmetic (precedence-safe, parens added only where flattening
% would change meaning), atom literals single-quoted (every bare identifier
% is a variable in this grammar -- see parse_dl.pl's module header for the
% full reasoning), strings double-quoted, decls spelled
% `rel Name(cols[: type]) log [keep(...)] [key(...)].`.

:- module(print_dl, [ print_dl_program/3, print_dl_to_file/3,
                      augmented_decls/6, print_dl_program_with_edb_types/7,
                      % The term renderer, exported for the
                      % expression_inventory unit (rank R5): it checks that
                      % parenthesization follows registry.pl expression/5's
                      % precedence field rather than a local operator list.
                      print_term/5
                    ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(analyze, [ rel_columns/5, declared_refs/2 ]).
:- use_module('0_rel_record', [ relplan_shape/6 ]).
:- use_module('0_dot_expand/registry',
              [ body_surface_for_term/6,
                wrapper_lower_role/3,
                expression/5,
                host_input_contract/3
              ]).
:- use_module('2_host_expand/0_cst_query', [ serialize_ts_query/2 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ entry points ════════════════════════════════════════════════════════════

print_dl_to_file(Prog, Bindings, FilePath) :-
    print_dl_program(Prog, Bindings, Text),
    setup_call_cleanup(
        open(FilePath, write, Stream),
        format(Stream, "~w", [Text]),
        close(Stream)).

print_dl_program(prog(Decls, Rules), Bindings, Text) :-
    print_dl_parts(Decls, Rules, [], Bindings, Text).
print_dl_program(program(Decls, Rules, Queries), Bindings, Text) :-
    print_dl_parts(Decls, Rules, Queries, Bindings, Text).

print_dl_parts(Decls, Rules, Queries, Bindings, Text) :-
    decl_ref_order(Decls, DeclItems),
    maplist(decl_line(Decls, Rules, Bindings), DeclItems, DeclLines),
    maplist(rule_line(Bindings), Rules, RuleLines),
    maplist(query_line(Bindings), Queries, QueryLines),
    ( DeclLines == [] -> DeclBlock = "" ; atomic_list_concat(DeclLines, DeclBlock) ),
    ( RuleLines == [] -> RuleBlock = "" ; atomic_list_concat(RuleLines, RuleBlock) ),
    ( QueryLines == [] -> QueryBlock = "" ; atomic_list_concat(QueryLines, QueryBlock) ),
    join_program_blocks([DeclBlock, RuleBlock, QueryBlock], Text).

join_program_blocks(Blocks, Text) :-
    exclude(==(""), Blocks, Present),
    atomic_list_concat(Present, "\n", Text).

% ═══ EDB decl synthesis (the text-door fix) ══════════════════════════════════
%
% print_dl_program/3 above only ever prints a decl line for a ref that
% literally has a Decls entry (module header's own note above
% decl_ref_order/2: fabricating one would break the =@= round-trip check).
% That is correct for G1's own grade, but it means an EDB ref whose only
% typing evidence is a Schedule/Initial LITERAL WITNESS (PHASE C2 RULING 1's
% own inference path, analyze.pl:column_type_at_decl/6) prints with ZERO
% decls at all -- e.g. expressions.pl's comparison_filters_rows fixture,
% Decls=[], callee_set_size/2 and shared_count/3 typed only by their Initial
% rows' literal ints. Re-parsing that printed text through compile_dl6/2
% (v6/prolog/compile/compile.pl) then infers every untyped column as TEXT
% (the C2a "zero witnesses -> text" default -- there IS no witness inside
% the text door's own input, since decls are the only thing the text door
% ever reads) and refuses on the fixture's own `Union > 0` int comparison:
% unsupported_construct(arith_operand_not_int(...,text)). The TERM door
% compiles the same fixture fine because compile.pl:program_plan/2 sees the
% original Initial/Schedule rows directly, never round-tripping through
% printed text.
%
% Fix: a CALLER holding the compile plan (compile.pl:program_plan/2's
% RelPlans + ArrivalTargets, both already computed once per fixture) can
% synthesize `col_type(Ref, Column, Type)` decl facts for exactly the EDB
% refs (ArrivalTargets: rule-headless, so per the one-rel-one-rule-kind law
% their typing can never come from a rule body, only from decls or literal
% witnesses) that the ORIGINAL surface program left undeclared, and hand
% the AUGMENTED decl list to the unmodified print_dl_program/3 above.
% print_decl_column/3 and rel_columns/5 (analyze.pl) already know how to
% render a col_type/3 entry into `col: type` text and to prefer decl-typed
% column names over mined ones -- nothing about print_dl_program/3 itself
% changes; this predicate only widens what Decls list it is handed.
%
% DECL AUTHORITY (never duplicate or contradict an explicit decl): a ref
% counts as "already declared" if declared_refs/2 (analyze.pl) finds ANY
% kind/2, keyed/2, keep/2, or col_type/3 entry for it in the EXPANDED decls
% -- Plan's own Prog field is already enum-expanded
% (compile.pl:program_plan/2 calls expand_enum_program/2 first), so an
% enum's variant refs (body_page/2, body_redirect/2, ...) already carry
% col_type/keyed entries THERE even though the RAW surface Decls list this
% predicate augments only ever shows the single `enum_decl(body, ...)`
% sugar term -- checking coverage against the expanded decls is what keeps
% this predicate from fabricating a redundant, sugar-contradicting
% `rel body_page(...)` line next to the enum decl.
%
% ONLY EDB refs are covered on purpose (compile.pl:program_plan/2's own
% ArrivalTargets = AllRefs minus DerivedRefs): a derived (rule-headed) ref's
% column types are re-inferred by the type fixpoint from its rule bodies
% every time regardless of what a decl says, so synthesizing a decl for one
% would be inert at best and misleading at worst.
%
% WITNESSED refs only, a SECOND and sharper restriction found by running
% the receipt, not assumed up front: an EDB ref that has ZERO rows in
% EITHER Initial or Schedule anywhere in a given fixture (dead alternative
% source -- e.g. timeless_rail.pl's clean_state_no_diags declares TWO
% rules feeding eprintln_waiver_line, one from waiver_block_comment, one
% from waiver_trailing_comment, but only the LATTER ever receives an
% Initial row in that fixture) types via analyze.pl's C2a fallback with
% NO real evidence -- contribution `open(none)`, the fixpoint's neutral
% "no opinion yet" placeholder (analyze.pl:seed_column_contribution/7).
% That placeholder is INERT in a union: analyze.pl:merge_type/3 only
% treats a concrete `text`/`int` contribution as sticky, so a witness-less
% sibling never overrides a genuinely-witnessed one merged into the same
% derived rel (eprintln_waiver_line correctly comes out `int` in the real
% term door, from waiver_trailing_comment's witnessed row alone).
% Synthesizing a col_type/3 decl for the WITNESS-LESS ref changes its
% contribution from that inert placeholder to a FROZEN, concrete `text`
% (analyze.pl:contribution_to_type/2's own "none -> text" default, now
% pinned instead of deferred) -- and a frozen contribution is sticky
% (merge_contribution/3's first clause never revises it), so it silently
% overrides the sibling's real `int` witness in the union and the SAME
% comparison the real term door accepts refuses in the text door with
% comparison_operand_not_int(...). This is a synthesis-caused regression,
% not a pre-existing gap, so WitnessedRefs (computed by the caller from
% the fixture's own Initial/Schedule, see text_door_receipt.pl and
% roundtrip.sh's write_one_view/6) restricts synthesis to refs that
% actually have a row somewhere -- a witness-less ref is left exactly as
% undeclared as print_dl_program/3 always left it, so both doors
% independently keep it neutral instead of one door pinning it.
%
% RESIDUAL, named rather than silently widened further: this check is
% REF-granular (does the ref have any row at all), not per-column. A ref
% with rows where one column happens to hold only non-atomic values (a
% braces/list literal, never observed in the current corpus for an EDB
% ref) would still get that column's decl synthesized and could in
% principle hit the same freeze hazard at column granularity. No fixture
% in the current corpus exercises this; a future one that does is a real
% finding, not a silent gap.

% DECLARED BUT UNTYPED is its own case, and it stayed invisible until an edge
% head's column type started flowing from its body: `kind(rev_fill/4, log)`
% covers the ref for declared_refs/2 while saying nothing about its columns,
% so the printed text carried a decl line with no types and the text door read
% every column as TEXT. The term door reads the fixture's Schedule literals
% and calls them int, the edge head downstream inherits int, and the two doors
% disagreed (comparison_operand_not_int on rev_status' `Behind > 0`).
% The extra candidates are restricted to refs the RAW program declares:
% expansion-GENERATED decls (enum variant rels) must keep printing exactly as
% the sugar implies, never as synthesized typed decls.
augmented_decls(RawDecls, ExpandedDecls, RelPlans, ArrivalTargets, WitnessedRefs, AugmentedDecls) :-
    declared_refs(ExpandedDecls, CoveredRefs),
    subtract(ArrivalTargets, CoveredRefs, UndeclaredCandidates),
    findall(Ref, member(col_type(Ref, _, _), ExpandedDecls), TypedRefs0),
    sort(TypedRefs0, TypedRefs),
    declared_refs(RawDecls, RawDeclaredRefs),
    findall(Ref,
            ( member(Ref, ArrivalTargets),
              memberchk(Ref, RawDeclaredRefs),
              \+ memberchk(Ref, TypedRefs) ),
            UntypedCandidates),
    append(UndeclaredCandidates, UntypedCandidates, Candidates0),
    sort(Candidates0, NeedsDeclCandidates),
    intersection(NeedsDeclCandidates, WitnessedRefs, NeedsDeclRefs),
    findall(col_type(Ref, Column, Type),
            ( member(Ref, NeedsDeclRefs),
              relplan_shape(RelPlans, Ref, _Kind, Columns, _KeyOrNone, ColumnTypes),
              nth1(Position, Columns, Column),
              nth1(Position, ColumnTypes, Type)
            ),
            NewColTypeDecls),
    append(RawDecls, NewColTypeDecls, AugmentedDecls).

print_dl_program_with_edb_types(prog(RawDecls, RawRules), Bindings,
                                ExpandedDecls, RelPlans, ArrivalTargets, WitnessedRefs, Text) :-
    augmented_decls(RawDecls, ExpandedDecls, RelPlans, ArrivalTargets, WitnessedRefs, AugmentedDecls),
    print_dl_program(prog(AugmentedDecls, RawRules), Bindings, Text).

% Decl lines print ONLY for refs that literally appear in Decls, in
% first-occurrence order -- NOT the declared_refs/program_refs union
% compile.pl:program_plan/2 computes for its own table-and-arrival purposes.
% A ref that is purely an arrival target with ZERO decl entries (Decls=[]
% level-only fixtures like expressions.pl are the extreme case) gets no line
% at all: synthesizing one would FABRICATE a kind(Ref,_) entry the original
% term never had, breaking G1's `=@=` LIST-structure check even though the
% two programs would mean the same thing under rel_kind/3's own fallback.
% The rule text alone already reveals the ref's shape (name + arity + the
% column names rel_columns/4 mines), so nothing is actually hidden.

decl_ref_order(Decls, Order) :-
    findall(Item,
            ( member(Decl, Decls),
              decl_order_item(Decl, Item),
              \+ shadowed_by_type_decl(Decls, Item)
            ), Refs0),
    dedup_preserve_order(Refs0, Order).

% A rel whose type_decl already prints must not also print from its bare
% col_type/kind/keyed/keep ref, or the decl line doubles and the reparse
% drops the type (parse_dl relation_schema arity-checks the doubled list).
shadowed_by_type_decl(Decls, Name/Arity) :-
    member(type_decl(Name, Specs), Decls),
    length(Specs, Arity).

decl_order_item(enum_decl(Name, Variants), enum_decl(Name, Variants)).
decl_order_item(Decl, Decl) :- Decl = rel_template_enum(_, _, _).
decl_order_item(Decl, Decl) :- Decl = interface_decl(_, _).
decl_order_item(Decl, Decl) :- Decl = rel_template(_, _, _).
decl_order_item(Decl, Decl) :- Decl = type_decl(_, _).
decl_order_item(Decl, Decl) :- Decl = sh_decl(_, _, _, _).
decl_order_item(kind(Ref, log), Ref).
% Arity 0 only: a column-bearing rel prints from its col_type entries, and a
% kind(Ref, set) line there would double the decl.
decl_order_item(kind(Name/0, set), Name/0).
decl_order_item(keyed(Ref, _), Ref).
decl_order_item(keep(Ref, _), Ref).
decl_order_item(col_type(Ref, _, _), Ref).

dedup_preserve_order(List, Deduped) :-
    dedup_preserve_order_(List, [], RevDeduped),
    reverse(RevDeduped, Deduped).
dedup_preserve_order_([], Acc, Acc).
dedup_preserve_order_([X | Xs], Acc, Out) :-
    ( memberchk(X, Acc) -> dedup_preserve_order_(Xs, Acc, Out)
    ; dedup_preserve_order_(Xs, [X | Acc], Out)
    ).

print_generic_parameter(type_parameter(Name, []), Name) :- !.
print_generic_parameter(type_parameter(Name, Constraints), Text) :-
    maplist(print_type_application, Constraints, ConstraintTexts),
    atomic_list_concat(ConstraintTexts, ' + ', ConstraintText),
    format(atom(Text), "~w: ~w", [Name, ConstraintText]).
print_generic_parameter(Name, Name).

% ═══ decl line : `rel Name(cols[: type]) [log] [keep(policy)] [key(positions)].`
% Reproduces EXACTLY the literal decl/2 entries this ref has in the original
% Decls list, in their original relative order -- never rel_kind/3's or
% decl_keep/3's fallback-merged view, which would silently synthesize or
% drop an entry the round-trip needs bit-for-bit (see the module-level note
% above decl_ref_order/2). ═══════════════════════════════════════════════

decl_line(_, _, _, enum_decl(Name, Variants), Line) :-
    !,
    print_enum_variants(Variants, VariantsText),
    format(atom(Line), "rel ~w(~w).~n", [Name, VariantsText]).
decl_line(_, _, _, interface_decl(Name, Parameters), Line) :-
    !,
    ( Parameters == []
    -> format(atom(Line), "interface ~w.~n", [Name])
    ; atomic_list_concat(Parameters, ', ', ParametersText),
      format(atom(Line), "interface ~w(~w).~n", [Name, ParametersText])
    ).
% Two parenthesized groups is the whole surface of a template, and its record
% holds the path segments, so the printed name is rebuilt from them.
decl_line(_, _, _, rel_template(Segments, Parameters, Specs), Line) :-
    !,
    atomic_list_concat(Segments, '.', Name),
    maplist(print_generic_parameter, Parameters, ParameterTexts),
    atomic_list_concat(ParameterTexts, ', ', ParametersText),
    maplist(print_template_column, Specs, ColumnTexts),
    atomic_list_concat(ColumnTexts, ', ', ColumnsText),
    format(atom(Line), "rel ~w(~w)(~w).~n", [Name, ParametersText, ColumnsText]).

% A parameterized enum prints its parameters in the first paren group and its
% variants in the second, mirroring the parser's two-group surface.
decl_line(_, _, _, rel_template_enum(Segments, Parameters, Variants), Line) :-
    !,
    atomic_list_concat(Segments, '.', Name),
    maplist(print_generic_parameter, Parameters, ParameterTexts),
    atomic_list_concat(ParameterTexts, ', ', ParametersText),
    print_enum_variants(Variants, VariantsText),
    format(atom(Line), "rel ~w(~w)(~w).~n", [Name, ParametersText, VariantsText]).

% shadowed_by_type_decl/2 suppresses this ref's bare decl line, so the
% modifiers carried by its kind/keep/keyed entries have nowhere else to print.
decl_line(Decls, _, _, type_decl(Name, Specs), Line) :-
    !,
    maplist(print_host_column, Specs, ColumnTexts),
    atomic_list_concat(ColumnTexts, ', ', ColumnsText),
    length(Specs, Arity),
    decl_modifiers_text(Decls, Name/Arity, Sep, ModifiersText),
    format(atom(Line), "rel ~w(~w)~w~w.~n",
           [Name, ColumnsText, Sep, ModifiersText]).
% Arrival rel spelling (ruling arrival_arrow_spelling); the template is not
% printed, so a non-empty term-door template deliberately does not round-trip.
decl_line(Decls, _, _, sh_decl(Name, Inputs, Outputs, template(_)), Line) :-
    !,
    maplist(print_host_column, Inputs, InputTexts),
    maplist(print_host_column, Outputs, OutputTexts),
    atomic_list_concat(InputTexts, ', ', InputsText),
    atomic_list_concat(OutputTexts, ', ', OutputsText),
    ( memberchk(arrival_identity(Name, Positions), Decls)
    -> atomic_list_concat(Positions, ', ', PositionsText),
       format(atom(KeyText), " key(~w)", [PositionsText])
    ;  KeyText = ''
    ),
    format(atom(Line), "rel ~w(~w) -> (~w)~w.~n",
           [Name, InputsText, OutputsText, KeyText]).
% rel_columns/5 runs numlist(1, Arity, _), which fails outright at arity 0.
decl_line(Decls, _Rules, _Bindings, Name/0, Line) :-
    !,
    decl_ref_spelling(Decls, Name/0, Spelling),
    decl_modifiers_text(Decls, Name/0, Sep, ModifiersText),
    format(atom(Line), "rel ~w()~w~w.~n",
           [Spelling, Sep, ModifiersText]).
decl_line(Decls, Rules, Bindings, Ref, Line) :-
    decl_ref_spelling(Decls, Ref, Name),
    rel_columns(Decls, Rules, Bindings, Ref, Columns),
    relation_arrow_columns(Decls, Ref, Columns, PrintedColumns, ArrowText),
    maplist(print_decl_column(Decls, Ref), PrintedColumns, ColumnTexts),
    atomic_list_concat(ColumnTexts, ', ', ColsText),
    decl_modifiers_text(Decls, Ref, Sep, ModifiersText),
    format(atom(Line), "rel ~w(~w)~w~w~w.~n",
           [Name, ColsText, ArrowText, Sep, ModifiersText]).

relation_arrow_columns(Decls, Ref, Columns, InputColumns, ArrowText) :-
    memberchk(return_alias(Ref, Position), Decls),
    nth1(Position, Columns, Alias),
    append(InputColumns, [return], Columns),
    !,
    format(atom(ArrowText), " -> ~w", [Alias]).
relation_arrow_columns(_, _, Columns, Columns, '').

% Sep is '' rather than ' ' when the ref carries no modifier, so a decl line
% with none closes straight onto the column list.
decl_modifiers_text(Decls, Ref, Sep, ModifiersText) :-
    findall(Decl, ( member(Decl, Decls), decl_is_modifier(Decl, Ref) ), RefDecls),
    maplist(print_decl_modifier, RefDecls, ModifierTexts),
    (   ModifierTexts == []
    ->  ModifiersText = '', Sep = ''
    ;   atomic_list_concat(ModifierTexts, ' ', ModifiersText), Sep = ' '
    ).

print_type_application(Application, Text) :-
    (   Application = type_path_application(Segments, Arguments)
    ->  atomic_list_concat(Segments, '.', Name),
        maplist(print_column_type, Arguments, ArgumentTexts),
        atomic_list_concat(ArgumentTexts, ', ', ArgumentsText),
        format(atom(Text), "~w(~w)", [Name, ArgumentsText])
    ;   Application = type_path(Segments)
    ->  atomic_list_concat(Segments, '.', Text)
    ;   compound(Application)
    ->  Application =.. [Name | Arguments],
        maplist(print_column_type, Arguments, ArgumentTexts),
        atomic_list_concat(ArgumentTexts, ', ', ArgumentsText),
        format(atom(Text), "~w(~w)", [Name, ArgumentsText])
    ;   Text = Application
    ).

print_template_column(column(Name, none), Text) :-
    !,
    Text = Name.
print_template_column(column(Name, Type), Text) :-
    print_column_type(Type, TypeText),
    format(atom(Text), "~w: ~w", [Name, TypeText]).

% The flat name is the mangle the dot phase resolves TO; printing it would
% strand every dotted atom in the rules on a reparse.
decl_ref_spelling(Decls, Ref, Name) :-
    (   memberchk(rel_path_decl(Ref, Segments), Decls)
    ->  atomic_list_concat(Segments, '.', Name)
    ;   Ref = Name/_Arity
    ).

print_host_column(col(Name, Type), Text) :-
    print_column_type(Type, TypeText),
    format(atom(Text), "~w: ~w", [Name, TypeText]).

% Mirror of decl_ref_spelling/3 for an element type: the dotted spelling is
% what reparses onto the same rel.
print_column_type(type_path(Segments), Text) :-
    !,
    atomic_list_concat(Segments, '.', Text).
% The retained internal term is json_list(T); the text door renders it with the
% same spelling.
print_column_type(json_list(Element), Text) :-
    !,
    print_column_type(Element, InnerText),
    format(atom(Text), "json_list(~w)", [InnerText]).
print_column_type(list(Element), Text) :-
    !,
    print_column_type(Element, InnerText),
    format(atom(Text), "list(~w)", [InnerText]).
print_column_type(option(Element), Text) :-
    !,
    print_column_type(Element, InnerText),
    format(atom(Text), "option(~w)", [InnerText]).
print_column_type(list_entity_dense_sequence(Element), Text) :-
    !,
    print_column_type(Element, InnerText),
    format(atom(Text), "list_entity_dense_sequence(~w)", [InnerText]).
print_column_type(list_interned_set(Element), Text) :-
    !,
    print_column_type(Element, InnerText),
    format(atom(Text), "list_interned_set(~w)", [InnerText]).
print_column_type(list_entity_linked_sequence(Element), Text) :-
    !,
    print_column_type(Element, InnerText),
    format(atom(Text), "list_entity_linked_sequence(~w)", [InnerText]).
print_column_type(id(Name), Text) :-
    !,
    format(atom(Text), "~w.id", [Name]).
print_column_type(annotated_type(Type, Applications), Text) :-
    !,
    print_column_type(Type, TypeText),
    maplist(print_annotation_application, Applications, ApplicationTexts),
    atomic_list_concat(ApplicationTexts, ', ', ApplicationsText),
    format(atom(Text), "@(~w, [~w])", [TypeText, ApplicationsText]).
print_column_type(named(Name, Value), Text) :-
    !,
    print_annotation_argument(named(Name, Value), Text).
print_column_type(arrow_type(Inputs, Output), Text) :-
    !,
    maplist(print_product_field, Inputs, InputTexts),
    atomic_list_concat(InputTexts, ', ', InputsText),
    print_column_type(Output, OutputText),
    format(atom(Text), "((~w) -> ~w)", [InputsText, OutputText]).
print_column_type(product_type(Fields), Text) :-
    !,
    maplist(print_product_field, Fields, FieldTexts),
    atomic_list_concat(FieldTexts, ', ', Inner),
    format(atom(Text), "(~w)", [Inner]).
print_column_type(sum_type(Variants), Text) :-
    !,
    maplist(print_sum_variant, Variants, VariantTexts),
    atomic_list_concat(VariantTexts, '; ', Inner),
    format(atom(Text), "(~w)", [Inner]).
print_column_type(type_path(Segments), Text) :-
    !,
    atomic_list_concat(Segments, '.', Text).
print_column_type(type_path_application(Segments, Arguments), Text) :-
    !,
    atomic_list_concat(Segments, '.', Name),
    maplist(print_type_argument, Arguments, ArgumentTexts),
    atomic_list_concat(ArgumentTexts, ', ', ArgumentsText),
    format(atom(Text), "~w(~w)", [Name, ArgumentsText]).
print_column_type(Type, Text) :-
    compound(Type),
    Type =.. [Name | Arguments],
    maplist(print_type_argument, Arguments, ArgumentTexts),
    atomic_list_concat(ArgumentTexts, ', ', ArgumentsText),
    format(atom(Text), "~w(~w)", [Name, ArgumentsText]), !.
print_column_type(Type, Text) :-
    format(atom(Text), "~w", [Type]).

print_type_argument(named(Name, Value), Text) :-
    !,
    print_type_value(Value, ValueText),
    format(atom(Text), "~w: ~w", [Name, ValueText]).
print_type_argument(Value, Text) :-
    print_type_value(Value, Text).

print_type_value(bool_lit(Value), Text) :- !, format(atom(Text), "~w", [Value]).
print_type_value(Value, Text) :- print_column_type(Value, Text).

print_annotation_application(Application, Text) :-
    Application =.. [Name | Arguments],
    maplist(print_annotation_argument, Arguments, ArgumentTexts),
    atomic_list_concat(ArgumentTexts, ', ', ArgumentsText),
    format(atom(Text), "~w(~w)", [Name, ArgumentsText]).

print_annotation_argument(named(Name, Value), Text) :-
    print_term(Value, [], 0, top, ValueText),
    format(atom(Text), "~w: ~w", [Name, ValueText]).
print_annotation_argument(pos(Value), Text) :-
    print_term(Value, [], 0, top, Text).

decl_is_modifier(kind(Ref, log), Ref).
decl_is_modifier(keep(Ref, _), Ref).
decl_is_modifier(keyed(Ref, _), Ref).

print_decl_column(Decls, Ref, Column, Text) :-
    ( memberchk(col_type(Ref, Column, Type), Decls)
    -> print_column_type(Type, TypeText),
       format(atom(Text), "~w: ~w", [Column, TypeText])
    ; Text = Column
    ).

print_decl_modifier(kind(_, log), Text) :- Text = log.
print_decl_modifier(keep(_, all), Text) :- !, Text = 'keep(all)'.
print_decl_modifier(keep(_, count(N)), Text) :- !, format(atom(Text), "keep(count(~w))", [N]).
print_decl_modifier(keyed(_, Positions), Text) :-
    atomic_list_concat(Positions, ', ', PosText),
    format(atom(Text), "key(~w)", [PosText]).

print_enum_variants((Left ; Right), Text) :-
    !,
    print_enum_variant(Left, LeftText),
    print_enum_variants(Right, RightText),
    format(atom(Text), "~w ; ~w", [LeftText, RightText]).
print_enum_variants(Variant, Text) :-
    print_enum_variant(Variant, Text).

print_enum_variant(Variant, Text) :-
    Variant =.. [Name | Fields],
    maplist(print_enum_field, Fields, FieldTexts),
    atomic_list_concat(FieldTexts, ', ', FieldsText),
    format(atom(Text), "~w(~w)", [Name, FieldsText]).

print_enum_field(Field, Text) :-
    Field =.. [':', ColumnName, TypeName],
    print_column_type(TypeName, TypeText),
    format(atom(Text), "~w: ~w", [ColumnName, TypeText]).

print_product_field(field(Name, Type), Text) :-
    print_column_type(Type, TypeText),
    format(atom(Text), "~w: ~w", [Name, TypeText]).

print_sum_variant(variant(Name, Fields), Text) :-
    maplist(print_product_field, Fields, FieldTexts),
    atomic_list_concat(FieldTexts, ', ', FieldsText),
    format(atom(Text), "~w(~w)", [Name, FieldsText]).

% ═══ rule line : `HeadText <- BodyText.` / `HeadText <+ BodyText.` ══════════

rule_line(Bindings, match(SourceAtom, Arms), Text) :-
    !,
    print_term(SourceAtom, Bindings, 0, top, SourceText),
    match_arm_terms(Arms, ArmList),
    maplist(print_match_arm(Bindings), ArmList, ArmTexts),
    atomic_list_concat(ArmTexts, "\n  ; ", ArmsText),
    format(atom(Text), "match ~w (\n  ; ~w\n).~n", [SourceText, ArmsText]).
rule_line(Bindings, (Head <- Body), Line) :- !,
    print_goal_term(Head, Bindings, HeadText),
    print_body(Body, Bindings, BodyText),
    format(atom(Line), "~w <-~w.~n", [HeadText, BodyText]).
rule_line(Bindings, (Head <+ Body), Line) :- !,
    print_goal_term(Head, Bindings, HeadText),
    print_body(Body, Bindings, BodyText),
    format(atom(Line), "~w <+~w.~n", [HeadText, BodyText]).

% A rel/0 atom is a GOAL and the SAME atom in an argument is a data value, so
% the `name()` spelling belongs to head and goal positions alone.
print_goal_term(Term, Bindings, Text) :-
    (   relation_atom_of_arity_zero(Term)
    ->  format(atom(Text), "~w()", [Term])
    ;   print_term(Term, Bindings, 0, top, Text)
    ).

% The surface-word clause of print_body_item/3 runs first, so anything still
% a bare atom at this point is an ordinary relation.
relation_atom_of_arity_zero(Term) :-
    atom(Term),
    Term \== true.

query_line(Bindings, query(Atom), Line) :-
    print_term(Atom, Bindings, 0, top, AtomText),
    format(atom(Line), "? ~w.~n", [AtomText]).
query_line(Bindings, query(Atom, order(OrderCols)), Line) :-
    print_term(Atom, Bindings, 0, top, AtomText),
    maplist(order_col_text(Atom, Bindings), OrderCols, ColTexts),
    atomic_list_concat(ColTexts, ', ', OrderText),
    format(atom(Line), "? ~w order by ~w.~n", [AtomText, OrderText]).

% The tail names the argument AS PRINTED, so re-parsing resolves the same
% position whatever spelling the binding gave that argument.
order_col_text(Atom, Bindings, order_col(Position, Direction), Text) :-
    arg(Position, Atom, Value),
    print_term(Value, Bindings, 0, top, ColumnText),
    (   Direction == desc
    ->  atomic_list_concat([ColumnText, ' desc'], Text)
    ;   Text = ColumnText
    ).

match_arm_terms((Left ; Right), Arms) :-
    !,
    match_arm_terms(Left, LeftArms),
    match_arm_terms(Right, RightArms),
    append(LeftArms, RightArms, Arms).
match_arm_terms(Arm, [Arm]).

print_match_arm(Bindings, (Head <- Guards), Text) :-
    !,
    print_term(Head, Bindings, 0, top, HeadText),
    print_body_inline(Guards, Bindings, GuardsText),
    format(atom(Text), "~w |-> ~w", [GuardsText, HeadText]).
print_match_arm(Bindings, (Head <+ Guards), Text) :-
    print_term(Head, Bindings, 0, top, HeadText),
    print_body_inline(Guards, Bindings, GuardsText),
    format(atom(Text), "~w |+> ~w", [GuardsText, HeadText]).

print_body_inline((Left, Right), Bindings, Text) :- !,
    print_body_item(Left, Bindings, LeftText),
    print_body_inline(Right, Bindings, RightText),
    format(atom(Text), "~w, ~w", [LeftText, RightText]).
print_body_inline(Item, Bindings, Text) :-
    print_body_item(Item, Bindings, Text).

% ═══ body : one goal per indented line ═══════════════════════════════════════

print_body((Left, Right), Bindings, Text) :- !,
    body_items((Left, Right), Items),
    maplist(print_body_item_with_bindings(Bindings), Items, ItemTexts),
    indented_body_lines(ItemTexts, Text).
print_body(Item, Bindings, Text) :-
    print_body_item(Item, Bindings, ItemText),
    format(atom(Text), " ~w", [ItemText]).

body_items((Left, Right), Items) :- !,
    body_items(Left, LeftItems),
    body_items(Right, RightItems),
    append(LeftItems, RightItems, Items).
body_items(Item, [Item]).

print_body_item_with_bindings(Bindings, Item, Text) :-
    print_body_item(Item, Bindings, Text).

indented_body_lines([], "").
indented_body_lines([Item], Text) :-
    format(atom(Text), "\n  ~w", [Item]).
indented_body_lines([Item | Rest], Text) :-
    indented_body_lines(Rest, RestText),
    format(atom(Text), "\n  ~w,~w", [Item, RestText]).

print_body_item(Term, Bindings, Text) :-
    Term = probe(Name, Inputs, Outputs, Salts),
    !,
    probe_surface_inputs(Name, Inputs, Salts, SurfaceInputs),
    append(SurfaceInputs, Outputs, Values),
    maplist(print_arg(Bindings), Values, ValueTexts),
    atomic_list_concat(ValueTexts, ', ', ValuesText),
    format(atom(Text), "~w(~w)", [Name, ValuesText]).
print_body_item(cst(Path, Digest, Language, Query, _), Bindings, Text) :-
    !,
    print_cst_body(Path, Digest, Language, Query, Bindings, Text).
print_body_item(cst(Path, Digest, Language, Query), Bindings, Text) :-
    !,
    print_cst_body(Path, Digest, Language, Query, Bindings, Text).
print_body_item(Term, Bindings, Text) :-
    body_surface_for_term(Term, _, _, _, LowerRole, _), !,
    print_surface_body_item(LowerRole, Term, Bindings, Text).
print_body_item(Term, Bindings, Text) :-
    relation_atom_of_arity_zero(Term), !,
    print_goal_term(Term, Bindings, Text).
print_body_item(Term, Bindings, Text) :-
    print_term(Term, Bindings, 0, top, Text).

print_cst_body(Path, Digest, Language, Query, Bindings, Text) :-
    print_term(Path, Bindings, 0, top, PathText),
    print_term(Digest, Bindings, 0, top, DigestText),
    serialize_ts_query(Query, QueryText),
    format(atom(Text), "cst(~w, ~w, ~w) { ~s }",
           [PathText, DigestText, Language, QueryText]).

print_surface_body_item(LowerRole, Term, Bindings, Text) :-
    wrapper_lower_role(LowerRole, Shape, _), !,
    Term =.. [Name | Args],
    print_surface_wrapper_args(Shape, Args, Bindings, ArgTexts),
    atomic_list_concat(ArgTexts, ', ', ArgsText),
    format(atom(Text), "~w(~w)", [Name, ArgsText]).
% A `goal(...)` row is a registry-claimed word whose SHAPE is an ordinary
% relation atom (`scan`, ruling files_naming). It prints exactly as the plain
% atom it is -- the row exists so the reserved-word walk reaches it, not to
% give it syntax -- and reparsing that text lands back on the same term.
print_surface_body_item(goal(_), Term, Bindings, Text) :-
    print_term(Term, Bindings, 0, top, Text).
print_surface_body_item(word(_), Term, _Bindings, Text) :-
    format(atom(Text), "~w", [Term]).
print_surface_body_item(infix(_), Term, Bindings, Text) :-
    Term =.. [Op, Left, Right],
    print_term(Left, Bindings, 0, top, LeftText),
    print_term(Right, Bindings, 0, top, RightText),
    format(atom(Text), "~w ~w ~w", [LeftText, Op, RightText]).

print_surface_wrapper_args(body_item, [Inner], Bindings, [InnerText]) :- !,
    print_body_item(Inner, Bindings, InnerText).
print_surface_wrapper_args(_, Args, Bindings, ArgTexts) :-
    maplist(print_arg(Bindings), Args, ArgTexts).

probe_surface_inputs(Name, IdentityValues, Salts, SurfaceValues) :-
    ( host_input_contract(Name, Columns, Roles),
      contract_value_counts(Roles, IdentityValues, Salts)
    -> interleave_host_inputs(Columns, Roles, IdentityValues, Salts,
                              SurfaceValues, [], [])
    ; Salts == []
    -> SurfaceValues = IdentityValues
    ; throw(probe_mismatch(probe(Name, IdentityValues, _, Salts)))
    ).

contract_value_counts(Roles, IdentityValues, Salts) :-
    include(==(identity), Roles, IdentityRoles),
    include(==(freshness), Roles, FreshnessRoles),
    same_length(IdentityRoles, IdentityValues),
    same_length(FreshnessRoles, Salts).

interleave_host_inputs([], [], Identity, Salts, [], Identity, Salts).
interleave_host_inputs([col(Name, _) | Columns], [Role | Roles],
                       Identity0, Salts0, [Value | Values],
                       Identity, Salts) :-
    ( Role == identity,
      Identity0 = [Value | Identity1],
      Salts1 = Salts0
    ; Role == freshness,
      Salts0 = [salt(Name, Value) | Salts1],
      Identity1 = Identity0
    ),
    interleave_host_inputs(Columns, Roles, Identity1, Salts1, Values,
                           Identity, Salts).

% ═══ general term printer : var (via Bindings) | int | atom (single-quoted)
% | string (double-quoted) | '{}'(Pairs) braces | list | arithmetic (infix,
% precedence-safe) | generic compound Name(Args...) ══════════════════════════

print_term(Term, Bindings, ParentPrec, Side, Text) :-
    ( var(Term)
    -> print_var(Term, Bindings, Text)
    ; Term = bool_lit(Boolean)
    -> format(atom(Text), "~w", [Boolean])
    ; integer(Term)
    -> format(atom(Text), "~w", [Term])
    ; float(Term)
    -> finite_float_text(Term, Text)
    ; string(Term)
    -> quote_value(Term, 0'", Text)
    ; Term = json_null
    -> Text = null
    ; Term = json_object(Pairs)
    -> print_json_object(Pairs, Bindings, Text)
    ; Term = json_array(Values)
    -> print_json_array(Values, Bindings, Text)
    ; Term == '{}'
    -> Text = '{}'
    ; is_list(Term)
    -> print_list(Term, Bindings, Text)
    ; Term = '{}'(Pairs)
    -> print_braces(Pairs, Bindings, PairsText), format(atom(Text), "{~w}", [PairsText])
    ; Term = spread(Element)
    -> print_term(Element, Bindings, 0, top, ElementText),
       format(atom(Text), "[... ~w]", [ElementText])
    ; Term = dot_get(_, _), dot_chain_fields(Term, Fields), maplist(identifier_atom, Fields)
    -> print_dot_chain(Term, Bindings, Text)
    ; Term = rel_path(Segments, Args), is_list(Segments), is_list(Args)
    -> atomic_list_concat(Segments, '.', PathText),
       maplist(print_arg(Bindings), Args, PathArgTexts),
       atomic_list_concat(PathArgTexts, ', ', PathArgsText),
       format(atom(Text), "~w(~w)", [PathText, PathArgsText])
    ; compound(Term), Term =.. [Op, Left, Right], arith_op(Op, MyPrec)
    -> print_term(Left, Bindings, MyPrec, left, LeftText),
       print_term(Right, Bindings, MyPrec, right, RightText),
       format(atom(Inner), "~w ~w ~w", [LeftText, Op, RightText]),
       ( needs_parens(MyPrec, Side, ParentPrec) -> format(atom(Text), "(~w)", [Inner]) ; Text = Inner )
    ; compound(Term)
    -> Term =.. [Name | Args],
       maplist(print_arg(Bindings), Args, ArgTexts),
       atomic_list_concat(ArgTexts, ', ', ArgsText),
       format(atom(Text), "~w(~w)", [Name, ArgsText])
    ; atom(Term)
    -> quote_value(Term, 0'\', Text)
    ; format(atom(Text), "~w", [Term])
    ).

finite_float_text(Value, Text) :-
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]),
    format(atom(Text), "~h", [Value]).

% The dot spelling, printed back as `Receiver.field.sub`. A chain only takes
% this route when every field re-reads as one (the parser admits only an
% identifier after the member dot); anything else falls through to the generic
% compound arm so the round trip cannot lose it.
print_dot_chain(Term, Bindings, Text) :-
    dot_chain_root(Term, Root),
    dot_chain_fields(Term, Fields),
    print_term(Root, Bindings, 0, top, RootText),
    field_dots(Fields, DotsText),
    format(atom(Text), "~w~w", [RootText, DotsText]).

dot_chain_root(Term, Root) :-
    ( nonvar(Term), Term = dot_get(Receiver, _)
    -> dot_chain_root(Receiver, Root)
    ;  Root = Term
    ).

dot_chain_fields(Term, Fields) :-
    ( nonvar(Term), Term = dot_get(Receiver, Field)
    -> dot_chain_fields(Receiver, Prefix),
       append(Prefix, [Field], Fields)
    ;  Fields = []
    ).

field_dots([], '').
field_dots([Field | Rest], Text) :-
    field_dots(Rest, RestText),
    format(atom(Text), ".~w~w", [Field, RestText]).

print_arg(Bindings, Arg, Text) :- print_term(Arg, Bindings, 0, top, Text).

% Precedence comes from registry.pl's expression/5.
arith_op(Operator, Precedence) :-
    expression(Operator/2, arithmetic, Precedence, _, _).

needs_parens(MyPrec, _, ParentPrec) :- MyPrec < ParentPrec, !.
needs_parens(MyPrec, right, ParentPrec) :- MyPrec =:= ParentPrec, !.

print_var(Var, Bindings, Text) :-
    ( member(Name = BoundVar, Bindings), BoundVar == Var
    -> format(atom(Text), "~w", [Name])
    ; Text = '_'
    ).

print_list([], _Bindings, "[]") :- !.
print_list(List, Bindings, Text) :-
    maplist(print_arg(Bindings), List, ItemTexts),
    atomic_list_concat(ItemTexts, ', ', Inner),
    format(atom(Text), "[~w]", [Inner]).

print_json_object([], _Bindings, "{}") :- !.
print_json_object(Pairs, Bindings, Text) :-
    maplist(print_json_pair(Bindings), Pairs, PairTexts),
    atomic_list_concat(PairTexts, ', ', Inner),
    format(atom(Text), "{~w}", [Inner]).

print_json_pair(Bindings, Key-Value, Text) :-
    quote_value(Key, 0'", KeyText),
    print_term(Value, Bindings, 0, top, ValueText),
    format(atom(Text), "~w: ~w", [KeyText, ValueText]).

print_json_array(Values, Bindings, Text) :-
    maplist(print_arg(Bindings), Values, ItemTexts),
    atomic_list_concat(ItemTexts, ', ', Inner),
    format(atom(Text), "[~w]", [Inner]).

print_braces((Pair, Rest), Bindings, Text) :- !,
    print_brace_pair(Pair, Bindings, PairText),
    print_braces(Rest, Bindings, RestText),
    format(atom(Text), "~w, ~w", [PairText, RestText]).
print_braces(Pair, Bindings, Text) :-
    print_brace_pair(Pair, Bindings, Text).

% A TYPED CAPTURE `{stars: Stars: int}` prints its type back, and the clause
% has to come first: the value of such a pair is itself a `:`/2 term, which
% print_term/5 would otherwise render through its generic compound arm.
print_brace_pair(Key:(Hole:Type), Bindings, Text) :-
    var(Hole), atom(Type), !,
    print_brace_key(Key, Bindings, KeyText),
    print_term(Hole, Bindings, 0, top, HoleText),
    format(atom(Text), "~w: ~w: ~w", [KeyText, HoleText, Type]).
print_brace_pair(Key:Value, Bindings, Text) :-
    print_brace_key(Key, Bindings, KeyText),
    print_term(Value, Bindings, 0, top, ValueText),
    format(atom(Text), "~w: ~w", [KeyText, ValueText]).

% The key axis prints back to the exact surface parse_dl.pl's brace_key/5
% reads. Only three shapes exist and all three round-trip:
%
%   '**'      -> **        ruling descent_depth_cap = uncapped
%   $(Var)    -> $name     ruling json_key_hole_marker = dollar
%   Atom      -> name      when it is identifier-shaped, otherwise 'name'
%
% The quoted fallback is what keeps a real JSON key like `$ref` a key: bare
% `$ref` would re-read as a hole, so a key whose text is not a plain
% identifier comes back quoted, which parse_dl.pl reads as a literal label.
print_brace_key('**', _Bindings, '**') :- !.
print_brace_key($(Var), Bindings, Text) :- !,
    print_var(Var, Bindings, Name),
    format(atom(Text), "$~w", [Name]).
print_brace_key(Key, _Bindings, Text) :-
    ( identifier_atom(Key) -> Text = Key ; quote_value(Key, 0'\', Text) ).

identifier_atom(Atom) :-
    atom(Atom),
    atom_codes(Atom, [First | Rest]),
    ( code_type(First, alpha) ; First == 0'_ ),
    forall(member(Code, Rest), ( code_type(Code, alnum) ; Code == 0'_ )).

% ═══ quoting : always explicit, never Prolog's own ~q "quote only if
% necessary" logic -- see parse_dl.pl's module header for why every atom
% must be quoted (bare identifiers are always variables in this grammar) ═══

quote_value(Value, QuoteChar, Out) :-
    ( atom(Value) -> atom_codes(Value, Codes) ; string_codes(Value, Codes) ),
    escape_for_quote(Codes, QuoteChar, EscapedCodes),
    atom_codes(Body, EscapedCodes),
    char_code(QuoteCharAtom, QuoteChar),
    format(atom(Out), "~w~w~w", [QuoteCharAtom, Body, QuoteCharAtom]).

escape_for_quote([], _, []).
escape_for_quote([C | Cs], Q, Out) :-
    ( C == Q -> Out = [0'\\, C | More]
    ; C == 0'\\ -> Out = [0'\\, 0'\\ | More]
    ; C == 0'\n -> Out = [0'\\, 0'n | More]
    ; Out = [C | More]
    ),
    escape_for_quote(Cs, Q, More).
