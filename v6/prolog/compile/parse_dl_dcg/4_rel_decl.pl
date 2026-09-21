rel_stmt(Decls) --> rel_stmt(Decls, _).

rel_stmt(Decls, Sites) --> rel_stmt_in([], Decls, Sites).

rel_stmt_in(Prefix, Decls, [decl_site(Rem, OwnDecls) | ChildSites]) -->
    here(Start),
    ~`rel`, ws,
    ( { Prefix == [] },
      ident(Name), #`(`, enum_variants(Variants), #`)`, #`.`,
      { OwnDecls = [enum_decl(Name, Variants)],
        ChildDecls = [], ChildSites = [],
        record_enum_column_orders(Name, Variants) }
    ; dotted_path(LocalSegs),
      { append(Prefix, LocalSegs, Segs) },
      (   generic_parameters(Parameters), #`(`,
          (   enum_variants(Variants), #`)`, #`.`,
              % A parameterized enum: the first group is generic type
              % parameters, the second the mutually-exclusive variant set.
              % Minted like a rel template but into enum_decl terms, so the
              % enum lowering phase owns the sum.
              { Prefix == [],
                OwnDecls = [rel_template_enum(Segs, Parameters, Variants)],
                ChildDecls = [], ChildSites = [] }
          ;   args(decl_a_column, Specs), #`)`,
              relation_arrow_output(Segs, Specs, ArrowSpecs, _ReturnAlias), #`.`,
              % A template mints no col_type/kind entry: this ONE term is
              % the record. No Ref exists yet to hang an alias decl on.
              { Prefix == [],
                OwnDecls = [rel_template(Segs, Parameters, ArrowSpecs)],
                ChildDecls = [], ChildSites = [] }
          )
      ;   #`(`,
          args(decl_a_column, Specs), #`)`,
          here(AfterInputs),
          ( { arrival_arrow_ahead(AfterInputs) }
          -> { Prefix == [] },
             arrival_decl_tail(Segs, Specs, OwnDecls),
             { ChildDecls = [], ChildSites = [] }
          ;  relation_arrow_output(Segs, Specs, ArrowSpecs, ReturnAlias),
             { length(ArrowSpecs, Arity),
               module_path_name(Segs, Name),
               Ref = Name/Arity,
               record_spec_names(Name, ArrowSpecs) },
             ws,
             { typed_decl_entries(Ref, ArrowSpecs, Typed) },
             rel_modifiers(Ref, Mods),
             { module_path_decls(Segs, Ref, PathDecls),
               arrow_return_alias_decl(Ref, ReturnAlias, AliasDecls),
               column_less_decls(Ref, ArrowSpecs, Mods, UnitDecls),
               append([Typed, Mods, PathDecls, AliasDecls, UnitDecls],
                      OwnDecls) },
             rel_decl_end(Segs, ChildDecls, ChildSites)
          )
      )
    ; { Prefix == [] },
      decl_b_tail(OwnDecls),
      { ChildDecls = [], ChildSites = [] }
    ),
    { length(Start, Rem),
      append(OwnDecls, ChildDecls, Decls) }.

% A block is declaration path scope. It emits the same flat declaration list
% as the dotted spelling, and its punctuation is consumed before any later
% compiler phase sees the program.
rel_decl_end(_, [], []) --> #`.`.
rel_decl_end(Path, ChildDecls, ChildSites) -->
    #`{`,
    nested_rel_stmts(Path, ChildDecls, ChildSites),
    #`}`, #`.`.

nested_rel_stmts(ParentPath, Decls, Sites) -->
    ws,
    ( peek(0'})
    -> { Decls = [], Sites = [] }
    ; ( rel_stmt_in(ParentPath, OwnDecls, OwnSites)
      -> nested_rel_stmts(ParentPath, RestDecls, RestSites),
         { append(OwnDecls, RestDecls, Decls),
           append(OwnSites, RestSites, Sites) }
      ;  { parse_failure(nested_relation_declaration) }
      )
    ).

% Relation arrows are declaration-only sugar. The output is represented by
% the same ordinary final column as the explicit spelling, so every later
% compiler phase consumes one canonical declaration shape.
relation_arrow_output(Segs, Specs, ArrowSpecs, ReturnAlias) -->
    ( ws, @`->`
    -> ws, type_expr(OutputType),
       { module_path_name(Segs, Name),
         length(Specs, InputArity),
         FinalArity is InputArity + 1,
         ( memberchk(column(return, _), Specs)
         -> throw(unsupported_construct(
                     arrow_return_column_collision(Name/FinalArity)))
         ;  true
         ),
         relation_arrow_alias(Specs, OutputType, ReturnAlias, ReturnType),
         append(Specs, [column(return, ReturnType)], ArrowSpecs) }
    ;  { ArrowSpecs = Specs, ReturnAlias = none }
    ).

relation_arrow_alias(Specs, OutputName, alias(Position), type) :-
    nth1(Position, Specs, column(OutputName, type)),
    !.
relation_arrow_alias(_, OutputType, none, OutputType).

% RULING arrival_arrow_spelling: a `( ident :` group after `->` on a rel is an
% arrival rel's response columns, desugared to sh_decl/4 with template('').
arrival_decl_tail(Segs, InSpecs, Decls) -->
    ws, @`->`, ws,
    here(Input), { response_column_group_ahead(Input) },
    { module_path_name(Segs, Name) },
    @`(`, host_output_columns(Name, OutSpecs), #`)`, ws,
    arrival_identity_decls(Name, InSpecs, IdentityDecls),
    #`.`,
    { specs_to_columns(InSpecs, Ins),
      specs_to_columns(OutSpecs, Outs),
      append(InSpecs, OutSpecs, Specs),
      record_spec_names(Name, Specs),
      record_host_signature(Name, Ins, Outs),
      record_host_path(Name, Segs),
      Decls = [sh_decl(Name, Ins, Outs, template("")) | IdentityDecls] }.

response_column_group_ahead([0'( | Rest]) :-
    whitespace_tail(Rest, [First | More]),
    ( code_type(First, alpha) ; First =:= 0'_ ),
    identifier_run(More, After),
    whitespace_tail(After, [0':, Next | _]),
    Next =\= 0':.

arrival_arrow_ahead(Input) :-
    whitespace_tail(Input, [0'-, 0'> | AfterArrow]),
    whitespace_tail(AfterArrow, Response),
    response_column_group_ahead(Response).

identifier_run([Code | Rest], After) :-
    ( code_type(Code, alnum) ; Code =:= 0'_ ),
    !,
    identifier_run(Rest, After).
identifier_run(After, After).

arrival_identity_decls(Name, InSpecs, Decls) -->
    ( key_clause(Positions)
    -> ws,
       { length(InSpecs, InputCount),
         (   forall(member(P, Positions),
                    ( integer(P), P >= 1, P =< InputCount ))
         ->  true
         ;   throw(unsupported_construct(
                     arrival_identity_out_of_range(Name, Positions)))
         ),
         Decls = [arrival_identity(Name, Positions)] }
    ;  { Decls = [] }
    ).

arrow_return_alias_decl(_, none, []).
arrow_return_alias_decl(Ref, alias(Position), [return_alias(Ref, Position)]).

% Parameters only when a SECOND group follows; the peek below decides it
% standing at the first group's closing paren.
generic_parameters(Parameters) -->
    here(Input), { generic_parameter_group_ahead(Input) },
    #`(`, args(generic_parameter, Parameters), { Parameters \== [] }, #`)`, ws,
    peek(0'(),
    { check_distinct_parameters(Parameters) }.

generic_parameter_group_ahead([0'( | Rest]) :-
    balanced_group_tail(Rest, 1, After),
    whitespace_tail(After, [0'( | _]).

balanced_group_tail([0'( | Rest], Depth0, After) :-
    !,
    Depth is Depth0 + 1,
    balanced_group_tail(Rest, Depth, After).
balanced_group_tail([0') | Rest], 1, Rest) :- !.
balanced_group_tail([0') | Rest], Depth0, After) :-
    !,
    Depth is Depth0 - 1,
    balanced_group_tail(Rest, Depth, After).
balanced_group_tail([Quote | Rest], Depth, After) :-
    memberchk(Quote, [0'\', 0'"]),
    !,
    balanced_quoted_tail(Quote, Rest, QuotedAfter),
    balanced_group_tail(QuotedAfter, Depth, After).
balanced_group_tail([0'# | Rest], Depth, After) :-
    !,
    skip_to_eol(Rest, CommentAfter),
    balanced_group_tail(CommentAfter, Depth, After).
balanced_group_tail([_ | Rest], Depth, After) :-
    balanced_group_tail(Rest, Depth, After).

balanced_quoted_tail(Quote, [Quote, Quote | Rest], After) :-
    !,
    balanced_quoted_tail(Quote, Rest, After).
balanced_quoted_tail(Quote, [Quote | Rest], Rest) :- !.
balanced_quoted_tail(Quote, [0'\\, _ | Rest], After) :-
    !,
    balanced_quoted_tail(Quote, Rest, After).
balanced_quoted_tail(Quote, [_ | Rest], After) :-
    balanced_quoted_tail(Quote, Rest, After).

whitespace_tail([C | Rest], After) :-
    code_type(C, space),
    !,
    whitespace_tail(Rest, After).
whitespace_tail([0'# | Rest], After) :-
    !,
    skip_to_eol(Rest, CommentAfter),
    whitespace_tail(CommentAfter, After).
whitespace_tail(After, After).

generic_parameter(type_parameter(Name, Constraints)) -->
    ident(Name), ws,
    ( @`:`
    -> ws, sep_plus(type_application, Constraints)
    ;  { Constraints = [] }
    ).

sep_plus(P, [X | Xs]) -->
    call(P, X), ws,
    ( @`+` -> ws, sep_plus(P, Xs) ; { Xs = [] } ).

% Decidable inside the one production, with no other declaration in hand.
check_distinct_parameters(Parameters) :-
    maplist(parameter_name, Parameters, Names),
    ( append(_, [Parameter | Tail], Names),
      memberchk(Parameter, Tail)
    -> unsupported(duplicate_generic_parameter(Parameter))
    ;  true
    ).

parameter_name(type_parameter(Name, _), Name).

interface_stmt(interface_decl(Name, Parameters)) -->
    ~`interface`, ws, ident(Name), ws,
    ( @`(`
    -> ws, args(ident, Parameters), #`)`
    ;  { Parameters = [] }
    ),
    #`.`.

type_application(Application) -->
    dotted_path(Segs), ws,
    ( @`(`
    -> ws, sep(type_expr, Arguments), #`)`,
       { type_path_application(Segs, Arguments, Application) }
    ;  { type_path_name(Segs, Application) }
    ).

column_less_decls(Ref, Specs, Mods, Decls) :-
    ( ( Specs \== [] ; memberchk(kind(Ref, _), Mods) )
    -> Decls = []
    ; Decls = [kind(Ref, set)]
    ).

module_path_name(Segs, Name) :- atomic_list_concat(Segs, '__', Name).

module_path_decls([_], _, []) :- !.
module_path_decls(Segs, Ref, [rel_path_decl(Ref, Segs)]).

rel_modifiers(Ref, Decls) -->
    ( ~`log` -> { Decl = kind(Ref, log) }
    ; keep_clause(Policy) -> { Decl = keep(Ref, Policy) }
    ; key_clause(Positions) -> { Decl = keyed(Ref, Positions) }
    ; ~`set`
    -> { unsupported(removed_word(set)), Decl = none }
    ), !,
    ws, rel_modifiers(Ref, Rest),
    { Decl == none -> Decls = Rest ; Decls = [Decl | Rest] }.
rel_modifiers(_, []) --> [].

decl_a_column(column(Name, Type)) -->
    ident(Name), ws,
    ( @`:` -> ws, type_expr(Type) ; { Type = none } ).

type_expr(Type) -->
    type_base(Base),
    ( @`?` -> { Type = option(Base) } ; { Type = Base } ).

type_base(_) -->
    @`@`, !,
    { throw(unsupported_construct(annotation_surface_removed)) }.

type_argument(named(Name, Value)) -->
    ident(Name), ws,
    here([0':, Next | _]), { Next \== 0'=, Next \== 0': }, !,
    @`:`, ws, expr(Value).
type_argument(Value) --> type_expr(Value).

type_base(T) --> { scalar_column_type(T) }, kw(T), !.
type_base(T) -->
    { type_wrapper(W, _) ; W = json_list },
    kw(W), !,
    #`(`, ws, type_expr(E), #`)`,
    { T =.. [W, E] }.
type_base(Type) -->
    dotted_path(Segs), ws,
    ( @`(`
    -> ws, sep(type_argument, Arguments), #`)`,
       { type_path_application(Segs, Arguments, Type) }
    ;  { type_path_name(Segs, Type) }
    ).
% Anonymous product and sum literals: `(a: int, b: text)` and
% `(Ok(value: T); Error(message: text))`. Both are a parenthesized group whose
% first item names the shape: `ident :` opens a product, `ident (` opens a sum.
% An empty group `()` receives a named refusal in the first slice.
type_base(Type) -->
    @`(`, ws,
    ( @`(`, ws, args(anonymous_field, Inputs), #`)`, ws, @`->`, ws,
      type_expr(Output), #`)`,
      { Type = arrow_type(Inputs, Output) }
    ; anonymous_type(Type), #`)`
    ).

anonymous_type(Type) -->
    ( peek(0'))
    -> { throw(unsupported_construct(anonymous_type_empty)) }
    ; ident(First), ws,
      ( @`:`
      -> ws, type_expr(FirstType),
         { FirstField = field(First, FirstType) },
         product_type_rest(Rest),
         { Type = product_type([FirstField | Rest]) }
      ; @`(`
      -> args(anonymous_field, FirstFields), #`)`,
         { FirstVariant = variant(First, FirstFields) },
         sum_type_rest(Rest),
         { Type = sum_type([FirstVariant | Rest]) }
      ; { parse_failure(anonymous_type) }
      )
    ).

product_type_rest(Fields) -->
    ws,
    ( @`,`
    -> ws, anonymous_field(First), product_type_rest(Rest),
       { Fields = [First | Rest] }
    ; { Fields = [] }
    ).

anonymous_field(field(Name, Type)) -->
    ident(Name), #`:`, ws, type_expr(Type).

sum_type_rest(Variants) -->
    ws,
    ( @`;`
    -> ws, sum_variant(First), sum_type_rest(Rest),
       { Variants = [First | Rest] }
    ; { Variants = [] }
    ).

sum_variant(variant(Name, Fields)) -->
    ident(Name), #`(`, args(anonymous_field, Fields), #`)`.

% Keep a mounted relation type's path until 0_dot_expand has mount scope and
% can use the same declared_path/3 lookup as a relation call.
type_path_name([Name], Name).
type_path_name(Segs, type_path(Segs)).

type_path_application([Name], Arguments, Type) :-
    !,
    Type =.. [Name | Arguments].
type_path_application(Segments, Arguments,
                      type_path_application(Segments, Arguments)).

enum_variants((First ; Rest)) -->
    enum_variant(First), #`;`, ws, enum_variants(Rest).
enum_variants(Variant) --> enum_variant(Variant).

enum_variant(Variant) -->
    ws, ident(Name), #`(`, args(enum_field, Fields), #`)`,
    { Variant =.. [Name | Fields] }.

enum_field(Col:Type) --> ident(Col), #`:`, ws, type_expr(Type).

record_enum_column_orders(Rel, Variants) :-
    tag_rel_name(Rel, Tag),
    record_cols(Tag, [id, tag]),
    forall(tree_leaf(';', Variants, V), record_enum_variant(Rel, V)).

% tree_leaf/3 is map_tree's generator twin: enumerate a ';'-tree's leaves
tree_leaf(Functor, Tree, Leaf) :-
    Tree =.. [Functor, Left, Right], !,
    ( tree_leaf(Functor, Left, Leaf) ; tree_leaf(Functor, Right, Leaf) ).
tree_leaf(_, Leaf, Leaf).

record_enum_variant(Rel, Variant) :-
    Variant =.. [Name | Fields],
    maplist([Col:_, Col]>>true, Fields, Cols),
    atomic_list_concat([Rel, Name], '_', VariantRel),
    record_cols(VariantRel, [id | Cols]).

tag_rel_name(Rel, Tag) :- atomic_list_concat([Rel, tag], '_', Tag).

record_spec_names(Name, Specs) :-
    maplist([column(N, _), N]>>true, Specs, Cols),
    record_cols(Name, Cols).

typed_decl_entries(Ref, Specs, Decls) :-
    findall(col_type(Ref, Col, Type),
            ( member(column(Col, Type), Specs), Type \== none ),
            Decls).

keep_clause(Policy) -->
    ~`keep`, #`(`, ws,
    ( ~`all` -> { Policy = all }
    ; ~`count`, #`(`, ws, int_lit(N), #`)`
    -> { Policy = count(N) }
    ),
    #`)`.

key_clause(Positions) -->
    ~`key`, #`(`, ws, sep(int_lit, Positions), #`)`.


decl_b_tail(Decls) -->
    ( @`(` -> ws, int_lit(Ret), #`)`, { HasRetention = true }
    ; { HasRetention = false }
    ),
    ws, ident(Name), #`(`,
    decl_b_columns(Name, Specs), #`)`, #`.`,
    { length(Specs, Arity),
      Ref = Name/Arity,
      record_spec_names(Name, Specs),
      ( HasRetention == true
      -> unsupported(retention_marker(Ref, Ret))
      ; true
      ),
      typed_decl_entries(Ref, Specs, Decls) }.

decl_b_columns(Rel, Specs) --> args(typed_col(decl_b_column_type(Rel)), Specs).

typed_col(TypeP, column(Col, Type)) -->
    ident(Col), #`:`, ws, call(TypeP, Col, Type).

decl_b_column_type(Rel, Col, none) -->
    coltype(W), { W \== none }, !,
    { unsupported(column_type_wrapper(Rel, Col, W)) }.
decl_b_column_type(_, _, Type) --> type_expr(Type).

coltype(W) -->
    { member(W, ['Key', 'Min', 'Max']) },
    kw(W), !,
    #`(`, ws, ident(_), #`)`.
coltype(none) --> ident(_).


