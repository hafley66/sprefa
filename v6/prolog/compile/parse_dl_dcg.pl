
:- module(parse_dl_dcg,
          [ parse_dl_dcg_entry/5,
            parse_dl/4,
            parse_dl_file/4,
            parse_dl_line_for_reason/2,
            remaining_line_column/3,
            statement_location_for_reason/3,
            statement_location_for_reference/4,
            use_item/3,
            parse_dl_source/5
          ]).

:- set_prolog_flag(back_quotes, codes).

% lists/apply/pairs ride SWI autoloading; both module imports take the whole
% export list because no name here collides with theirs.
:- use_module('../0_dot_expand/registry').
:- use_module('../0_cst_query').
:- use_module('../0_type_plane', [type_wrapper/2, column_element_type_name/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Terminal sigils: a module-local prefix-operator DSL. @Codes matches the
% literal right here, ~Codes adds a word boundary, #Codes skips ws then @Codes.
:- op(200, fy, [#, @, ~]).

:- thread_local finding_fact/1, rel_column_order_fact/2,
                host_signature_fact/3, host_path_fact/2,
                source_statement_fact/3,
                parse_marks_on/0.

% THREAD_LOCAL, not dynamic: parse_dl_source/5 retracts all four at entry and
% reads them back at exit, so two parses running at once on shared clauses would
% each erase the other's findings. The plunit battery runs units on parallel
% workers, and every unit parses.

% lex_token/2 rows sit beside the escape decoders they mirror, so the clauses
% are spread across the file on purpose.
:- discontiguous lex_token/2.
:- discontiguous type_base/3.

% Editor CST boundaries this parser erases: Nonterminal -> Node-FieldNames,
% bare = shape from clauses, ref = name only, repeat = item only, '-' = unnamed.
cst_shape(decl_a_column/1,  declaration_parameter-[name, type]).
cst_shape(enum_variants/1,  enum_variants-[]).
cst_shape(rel_modifiers/2,  repeat(relation_modifier)-[]).
cst_shape(match_stmt/1,     match_statement-[scrutinee]).
cst_shape(match_arm/1,      match_arm-[guard, arrow, head]).
cst_shape(braces_term/1,    object_pattern-[]).
cst_shape(dotted_path/1,    path-[]).
cst_shape(statement/2,      statement-[]).
cst_shape(statements/3,     source_file-[]).
cst_shape(rel_stmt/1,       relation_declaration-[]).
cst_shape(interface_stmt/1, interface_declaration-[]).
cst_shape(typed_col/2,      ref(column)-[]).
cst_shape(type_expr/1,      type-[]).
cst_shape(annotation_type/1, type_annotation-[type, applications]).
cst_shape(annotation_application/1, annotation_application-[name]).
cst_shape(annotation_list/1, annotation_list-[]).
cst_shape(annotation_argument/1-named, annotation_named_argument-[name, value]).
cst_shape(enum_variant/1,   ref(enum_variant)-[]).
cst_shape(rule_stmt/1,      rule-[head, arrow, body]).
cst_shape(query_stmt/1,     ref(query)-[]).
cst_shape(body/1,           ref(goal_list)-[]).
cst_shape(expr/1,           ref(expression)-[]).
cst_shape(head_atom/1,      atom-[name]).
cst_shape(brace_pair/1,     object_pair-[key, value, type]).
cst_shape(json_object/1,    json_object-[]).
cst_shape(json_array/1,     json_array-[]).
cst_shape(json_pair/1,      json_pair-[key, value]).
cst_shape(list_term/1,      list-[]).
cst_shape(int_lit/1,        ref(integer)-[]).
cst_shape(float_lit/1,      ref(float)-[]).
cst_shape(string_lit/1,     string-[]).
cst_shape(atom_lit/1,       quoted_atom-[]).
cst_shape(template_lit/1,   template-[]).
cst_shape(bool_lit/1,       boolean-[]).
cst_shape(ident/1,          ref(identifier)-[]).
% editor nodes the parser folds with no named nonterminal; the emitter
% renders each from its fixed editor shape (editor_* keys are not parser preds)
cst_shape(editor_paren/1,   parenthesized_expression-[]).
cst_shape(editor_literal/1, literal-[]).
cst_shape(editor_member/1,  member_expression-[]).

% Nodes the parser folds away: Nonterminal-Marker -> Node, inner = the
% marked branch alone rather than the nonterminal with that branch chosen.
cst_origin(atom_arg/1-named,    named_argument-[name, value]).
cst_origin(rule_stmt/1-true,    fact-[]).
cst_origin(brace_key/1-'$',     capture_key-[]).
cst_origin(dot_chain/2-dot_get, member_access-[]).
cst_origin(list_term/1-spread,  inner(spread_element)-[]).

unsupported(Surface) :- assertz(finding_fact(unsupported_surface(Surface))).
record_cols(Name, Cols) :-
    retractall(rel_column_order_fact(Name, _)),
    assertz(rel_column_order_fact(Name, Cols)).
lookup_column_order(Name, Cols) :- rel_column_order_fact(Name, Cols).
record_host_signature(Name, Ins, Outs) :-
    retractall(host_signature_fact(Name, _, _)),
    assertz(host_signature_fact(Name, Ins, Outs)).
record_host_path(Name, Segments) :-
    retractall(host_path_fact(Name, _)),
    assertz(host_path_fact(Name, Segments)).


parse_dl_file(File, Prog, Bindings, Findings) :-
    read_file_to_codes(File, Codes, []),
    parse_dl_source(File, Codes, Prog, Bindings, Findings).

parse_dl_dcg_entry(Source, Codes, Prog, Bindings, Findings) :-
    parse_dl_source(Source, Codes, Prog, Bindings, Findings).

parse_dl(Codes, Prog, Bindings, Findings) :-
    parse_dl_source(none, Codes, Prog, Bindings, Findings).

parse_dl_source(_, Codes, _, _, _) :-
    var(Codes), !,
    throw(dl_parse_error(invalid_input, position(1, 1))).
% Named and punned arguments resolve against a rel's declared column order,
% and a rule may precede the declaration it reads, so a first pass records
% every declaration's column order and the real pass starts from that set.
parse_dl_source(Source, Codes, Prog, Bindings, Findings) :-
    nb_setval(dl_prepass_columns, []),
    catch(( parse_dl_pass(Source, Codes, _, _, _) -> true ; true ), _, true),
    findall(Name-Cols, rel_column_order_fact(Name, Cols), Known),
    nb_setval(dl_prepass_columns, Known),
    catch(parse_dl_pass(Source, Codes, Prog, Bindings, Findings),
          dl_parse_error(Reason, _),
          parse_dl_marked_failure(Source, Codes, Reason)).

% mark/1 walks the whole remaining input at every token and parse_failure/1 is
% the only reader of what it records, so the first pass runs with marks off and
% a throwing parse is replayed once with them on.
parse_dl_marked_failure(Source, Codes, Reason) :-
    setup_call_cleanup(
        assertz(parse_marks_on),
        catch(( parse_dl_pass(Source, Codes, _, _, _) -> true ; true ),
              Ball,
              true),
        retractall(parse_marks_on)),
    (   var(Ball)
    ->  throw(dl_parse_error(Reason, position(1, 1)))
    ;   throw(Ball)
    ).

parse_dl_pass(_, Codes, Prog, Bindings, Findings) :-
    maplist(retractall,
            [ finding_fact(_), rel_column_order_fact(_, _),
              host_signature_fact(_, _, _), host_path_fact(_, _),
              source_statement_fact(_, _, _) ]),
    ( nb_current(dl_prepass_columns, Known) -> true ; Known = [] ),
    forall(member(Name-Cols, Known), assertz(rel_column_order_fact(Name, Cols))),
    length(Codes, Len),
    nb_setval(parse_input_length, Len),
    nb_setval(parse_furthest_remaining, Len),
    build_line_starts(Codes),
    b_setval(dl_vars, []),
    phrase(statements(Decls0, Rules0, Queries), Codes, Left),
    ( Left == [] -> true ; mark(Left), parse_failure(trailing_input) ),
    resolve_module_path_collisions(Decls0, Decls1),
    normalize_relation_value_decls(Decls1, Decls),
    flatten_host_paths(Decls, Rules0, Rules1),
    normalize_host_calls(Decls, Rules1, Rules),
    b_getval(dl_vars, VarsFinal),
    maplist([Name-Var, Name=Var]>>true, VarsFinal, BindingsRev),
    reverse(BindingsRev, Bindings),
    findall(F, finding_fact(F), Findings),
    ( Queries == [],
      \+ member(sh_decl(_, _, _, _), Decls)
    -> Prog = prog(Decls, Rules)
    ; Prog = program(Decls, Rules, Queries)
    ).

parse_failure(Reason) :-
    nb_getval(parse_furthest_remaining, Rem),
    remaining_line_column(Rem, Line, Col),
    throw(dl_parse_error(Reason, position(Line, Col))).

% mark/1 records the furthest-reached suffix; error positions derive from it
mark(S) :-
    parse_marks_on,
    length(S, R),
    nb_current(parse_furthest_remaining, F),
    R < F, !,
    nb_setval(parse_furthest_remaining, R).
mark(_).

remaining_line_column(Rem, Line, Col) :-
    nb_getval(parse_input_length, Len),
    Index is Len - Rem,
    nb_getval(parse_line_starts, Starts),
    nb_getval(parse_line_count, Count),
    line_containing(1, Count, Index, Starts, Line),
    arg(Line, Starts, Start),
    Col is Index - Start + 1.

line_containing(Low, High, Index, Starts, Line) :-
    ( Low >= High
    -> Line = Low
    ; Mid is (Low + High + 1) // 2,
      arg(Mid, Starts, MidStart),
      ( MidStart =< Index
      -> line_containing(Mid, High, Index, Starts, Line)
      ; Before is Mid - 1,
        line_containing(Low, Before, Index, Starts, Line)
      )
    ).

build_line_starts(Codes) :-
    findall(P, ( nth0(I, Codes, 0'\n), P is I + 1 ), Ps),
    Starts =.. [line_starts, 0 | Ps],
    functor(Starts, _, Count),
    nb_setval(parse_line_starts, Starts),
    nb_setval(parse_line_count, Count).

prolog:message(dl_parse_error(Reason, position(Line, Col))) -->
    [ 'parse error at line ~d, column ~d: ~w'-[Line, Col, Reason] ].

parse_dl_line_for_reason(Reason, Line) :-
    reason_references(Reason, Refs),
    ( member(Ref, Refs), statement_location_for_reference(rule, Ref, Line, _)
    -> true
    ; member(Ref, Refs), statement_location_for_reference(decl, Ref, Line, _)
    -> true
    ).

statement_location_for_reason(Reason, Line, Col) :-
    reason_references(Reason, Refs),
    ( member(Ref, Refs), statement_location_for_reference(rule, Ref, Line, Col)
    ; member(Ref, Refs), statement_location_for_reference(decl, Ref, Line, Col)
    ).

reason_references(Reason, Refs) :-
    findall(Name/Arity,
            ( sub_term(Name/Arity, Reason), atom(Name), integer(Arity) ),
            Refs0),
    sort(Refs0, Refs).

statement_location_for_reference(Kind, Ref, Line, Col) :-
    ( Kind == rule,
      source_statement_fact(rule, Item, Rem),
      statement_head_reference(Item, Ref)
    -> true
    ; source_statement_fact(Kind, Item, Rem),
      statement_reference(Kind, Item, Ref)
    ),
    !,
    remaining_line_column(Rem, Line, Col).

statement_head_reference(Rule, Name/Arity) :-
    Rule =.. [Op, Head, _], memberchk(Op, [<-, <+]), !,
    functor(Head, Name, Arity).

statement_reference(rule, Rule, Name/Arity) :-
    sub_term(Term, Rule),
    compound(Term),
    functor(Term, Name, Arity),
    atom(Name),
    !.
statement_reference(decl, Decls, Ref) :-
    member(Decl, Decls),
    declaration_source_ref(Decl, Ref),
    !.

record_statement(Kind, Item, Rem) :-
    memberchk(Kind-Recorded, [decl_list-decl, rule-rule]), !,
    assertz(source_statement_fact(Recorded, Item, Rem)).
record_statement(_, _, _).

declaration_source_ref(Decl, Ref) :-
    Decl =.. [F, Ref | _],
    memberchk(F, [kind, keyed, keep, col_type]).
declaration_source_ref(type_decl(Name, Specs), Name/Arity) :-
    length(Specs, Arity).
declaration_source_ref(sh_decl(Name, Ins, Outs, _), Name/Arity) :-
    append(Ins, Outs, Cols),
    length(Cols, Arity).


% A dotted host goal is a NAME with segments, never a nested rel, so it
% flattens to its module-path atom here rather than reaching the dot-expansion
% phase: normalize_host_leaf/3 below reads the goal's FUNCTOR against the sh
% declarations, and rel_path/2's functor is `rel_path`.
flatten_host_paths(Decls, Rules, Out) :-
    (   member(sh_decl(_, _, _, _), Decls)
    ->  maplist(flatten_host_path_rule(Decls), Rules, Out)
    ;   Out = Rules
    ).

flatten_host_path_rule(Decls, Rule0, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    map_tree(',', flatten_host_path_leaf(Decls), Body, Flattened),
    Rule =.. [Op, Head, Flattened].
flatten_host_path_rule(Decls, match(Source, Arms), match(Source, Flattened)) :- !,
    map_tree(';', flatten_host_path_rule(Decls), Arms, Flattened).
flatten_host_path_rule(_, Rule, Rule).

flatten_host_path_leaf(Decls, rel_path(Segments, Args), Atom) :-
    module_path_name(Segments, Name),
    member(sh_decl(Name, _, _, _), Decls),
    !,
    Atom =.. [Name | Args].
flatten_host_path_leaf(_, Item, Item).

normalize_host_calls(Decls, Rules, Out) :-
    maplist(normalize_host_rule(Decls), Rules, Out).

% map_tree/4 rewrites the leaves of a ','- or ';'-tree; the leaf goal arrives
% partially applied and is reached through call/3.
map_tree(Functor, Goal, Tree, Out) :-
    Tree =.. [Functor, Left, Right], !,
    Out =.. [Functor, NewLeft, NewRight],
    map_tree(Functor, Goal, Left, NewLeft),
    map_tree(Functor, Goal, Right, NewRight).
map_tree(_, Goal, Leaf, Out) :- call(Goal, Leaf, Out).

normalize_host_rule(Decls, Rule0, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    map_tree(',', normalize_host_leaf(Decls), Body, N),
    Rule =.. [Op, Head, N].
normalize_host_rule(Decls, match(Source, Arms), match(Source, N)) :- !,
    map_tree(';', normalize_host_rule(Decls), Arms, N).
normalize_host_rule(_, Rule, Rule).

normalize_host_leaf(_, probe(A, B, C, D), probe(A, B, C, D)) :- !.
normalize_host_leaf(_, Item, Item) :-
    body_surface_for_term(Item, _, _, _, _, _),
    !.
normalize_host_leaf(Decls, Atom, probe(Name, Ins, Outs, Salts)) :-
    compound(Atom),
    functor(Atom, Name, _),
    member(sh_decl(Name, Cols, _, _), Decls),
    !,
    Atom =.. [_ | Values],
    length(Cols, N),
    split_probe_values(N, Values, SurfaceIns, Outs),
    ( memberchk(arrival_identity(Name, Positions), Decls)
    -> arrival_roles(Cols, Positions, Roles)
    ;  host_input_roles(Name, Cols, Roles)
    ),
    ( same_length(Cols, SurfaceIns)
    -> partition_hiv(Cols, SurfaceIns, Roles, Ins, Salts)
    ; Ins = SurfaceIns, Salts = []
    ).
normalize_host_leaf(_, Item, Item).


statements(Decls, Rules, Queries) -->
    ws, here(S1),
    ( { S1 == [] }
    -> { Decls = [], Rules = [], Queries = [] }
    ; ( statement(Kind, Item, Sites)
      -> { length(S1, Rem),
           record_statement_sites(Kind, Item, Rem, Sites) }
      ; { parse_failure(statement) }
      ),
      statements(Decls1, Rules1, Queries1),
      { attach(Kind, Item, Decls1, Rules1, Queries1, Decls, Rules, Queries) }
    ).

record_statement_sites(decl_list, _, _, Sites) :-
    Sites = [_ | _],
    !,
    maplist(record_decl_site, Sites).
record_statement_sites(Kind, Item, Rem, _) :-
    record_statement(Kind, Item, Rem).

record_decl_site(decl_site(Rem, Decls)) :-
    record_statement(decl_list, Decls, Rem).

attach(decl_list, I, Ds, Rs, Qs, Ds2, Rs, Qs) :- append(I, Ds, Ds2).
attach(rule, I, Ds, Rs, Qs, Ds, [I | Rs], Qs).
attach(query, I, Ds, Rs, Qs, Ds, Rs, [I | Qs]).


% ws//0 eats comments before any consumer sees them; an editor keeps them
cst_extra(comment, '#.*').

ws(S0, S) :-
    ws_skip(S0, S),
    ( S0 == S -> true ; mark(S) ).

ws_skip(S0, S) :-
    ( S0 = [C | S1], code_type(C, space) -> ws_skip(S1, S)
    ; S0 = [0'# | S1] -> skip_to_eol(S1, S2), ws_skip(S2, S)
    ; S = S0
    ).

skip_to_eol(S0, S) :-
    ( S0 = [0'\n | S1] -> S = S1
    ; S0 = [_ | S1] -> skip_to_eol(S1, S)
    ; S = S0
    ).

@([], S, S) :- mark(S).
@([C | Cs], S0, S) :-
    S0 = [C | Rest],
    ( @(Cs, Rest, S) -> true ; mark(S0), fail ).

~(Cs, S0, S) :-
    @(Cs, S0, S),
    \+ (S = [C | _], id_code(C)).

peek(C, S, S) :- S = [C | _], !.

% kw//1: an already-chosen atom spelled as a word terminal
kw(Word) --> { atom_codes(Word, Cs) }, ~Cs.

id_code(0'_) :- !.
id_code(C) :- code_type(C, alnum).

% here//1 zero-width capture of the remaining input; back//1 pushback to it
here(S, S, S).
back(S, _, S).


ident(Name, S0, S) :-
    mark(S0),
    S0 = [C | Rest],
    ( code_type(C, alpha) ; C == 0'_ ), !,
    ident_rest(Rest, Cs, S),
    atom_codes(Name, [C | Cs]).

ident_rest([C | Cs], [C | More], S) :- id_code(C), !, ident_rest(Cs, More, S).
ident_rest(S, [], S).


int_lit(Value, S0, S) :-
    mark(S0),
    ( S0 = [0'- | S2] -> Sign = -1 ; S2 = S0, Sign = 1 ),
    S2 = [D | _], code_type(D, digit), !,
    digits0(Ds, S2, S),
    mark(S),
    number_codes(Mag, Ds),
    Value is Sign * Mag.

float_lit(Value, S0, S) :-
    mark(S0),
    phrase(float_codes(Cs), S0, S),
    number_codes(Value, Cs),
    float(Value),
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]).

float_codes(Cs) -->
    ( `-` -> { Sign = [0'-] } ; { Sign = [] } ),
    digits1(Int), float_tail(Tail),
    { append([Sign, Int, Tail], Cs) }.

digits0([C | More]) --> [C], { code_type(C, digit) }, !, digits0(More).
digits0([]) --> [].
digits1([C | More]) --> [C], { code_type(C, digit) }, !, digits0(More).

float_tail(Cs) -->
    `.`, digits1(F),
    ( exp(E) -> [] ; { E = [] } ),
    { append([0'. | F], E, Cs) }.
float_tail(Cs) --> exp(Cs).

exp([M | Cs]) -->
    here(Remaining), { mark(Remaining) },
    [M], { memberchk(M, `eE`) },
    ( [S], { memberchk(S, `+-`) } -> { Sign = [S] } ; { Sign = [] } ),
    digits1(Ds),
    { append(Sign, Ds, Cs) }.


atom_lit(Atom, S0, S) :- quoted(0'\', Cs, S0, S), atom_codes(Atom, Cs).
string_lit(Str, S0, S) :- quoted(0'", Cs, S0, S), string_codes(Str, Cs).

% quoted//4 decodes escapes; an editor wants the raw span these patterns match
lex_token(string_lit/1, '"([^"\\\\]|\\\\.)*"').
lex_token(atom_lit/1, '\'([^\'\\\\]|\\\\.)*\'').

quoted(Q, Cs, S0, S) :-
    mark(S0),
    S0 = [Q | S1], !,
    quoted_chars(Q, S1, Cs, S).

quoted_chars(Q, [Q, Q | Rest], [Q | More], S) :- !,
    mark([Q, Q | Rest]),
    quoted_chars(Q, Rest, More, S).
quoted_chars(Q, [Q | Rest], [], Rest) :- !.
quoted_chars(Q, [0'\\, E | Rest], Cs, S) :- !,
    mark([0'\\, E | Rest]),
    escape(Q, E, Cs, More),
    quoted_chars(Q, Rest, More, S).
quoted_chars(Q, [C | Rest], [C | More], S) :-
    quoted_chars(Q, Rest, More, S).

% the five recognized escapes collapse to one memberchk over Source-Decoded
% pairs; the Quote-Quote row closes over the active quote character.
escape(Quote, Source, [Decoded | M], M) :-
    memberchk(Source-Decoded,
              [ 0'n - 0'\n, 0't - 0'\t, 0'r - 0'\r,
                0'\\ - 0'\\, Quote - Quote ]), !.
escape(_, Other, [0'\\, Other | M], M).


% dl_vars: b_setval backtrackable global replaces the old V0/V accumulator
% threading; the trail unwinds it exactly as the threaded pair used to.
get_or_make_var(Name, Var) :-
    b_getval(dl_vars, Vars0),
    ( memberchk(Name-Existing, Vars0)
    -> Var = Existing
    ; b_setval(dl_vars, [Name-Var | Vars0])
    ).

hole_var('_', _) :- !.
hole_var(Name, Var) :- get_or_make_var(Name, Var).


% A quoted target is a file; a bare ident is an executor family the registry
% rosters (use_mod, resolved in executor_modules.pl).
use_item(Item) -->
    ws,
    ( ~`pub` -> ws, ~`use`, { Visibility = pub_use } ; ~`use`, { Visibility = use } ),
    ws,
    ( string_lit(Text) -> { Target = Text, F = Visibility }
    ; ident(Module), { Target = Module, module_use_functor(Visibility, F) }
    ),
    ws,
    ( ~`as`, ws, ident(Alias)
    -> { Item =.. [F, Target, Alias] }
    ; { Item =.. [F, Target] }
    ),
    ws, [0'.].

module_use_functor(use, use_mod).
module_use_functor(pub_use, pub_use_mod).


% import "spec": attaches an external spec; records its source span (0-based,
% end inclusive, the hover_note convention) via the existing col machinery.
import_stmt(import_decl(File, Line, Col, EndLine, EndCol)) -->
    here(Start), ~`import`, ws, string_lit(File), ws, [0'.], here(End),
    { length(Start, R0), length(End, R1),
      remaining_line_column(R0, L1, C1),
      remaining_line_column(R1, L2, C2),
      Line is L1 - 1, Col is C1 - 1,
      ( L2 == L1 -> EndLine is L1 - 1, EndCol is C2 - 2
      ; EndLine is L2 - 1, EndCol is C2 - 1 ) }.


statement(Kind, Item, Sites) -->
    ws,
    ( removed_world_decl_stmt(Ds)
    -> { Kind = decl_list, Item = Ds, Sites = [] }
    ; interface_stmt(D)
    -> { Kind = decl_list, Item = [D], Sites = [] }
    ; rel_stmt(Ds, Sites)
    -> { Kind = decl_list, Item = Ds }
    ; import_stmt(D)
    -> { Kind = decl_list, Item = [D], Sites = [] }
    ; query_stmt(Q)
    -> { Kind = query, Item = Q, Sites = [] }
    ; ( match_stmt(R) -> [] ; rule_stmt(R) -> [] ),
      { Kind = rule, Sites = [],
        b_getval(dl_vars, Vars), annotate_cst_item(Vars, R, Item) }
    ).


% parameterized nonterminals via call//N; one arity now that dl_vars is global
sep(P, [X | Xs]) -->
    call(P, X), ws,
    ( @`,` -> ws, sep(P, Xs) ; { Xs = [] } ).
args(P, Xs) --> ws, ( peek(0')) -> { Xs = [] } ; sep(P, Xs) ).
#Cs --> ws, @Cs.


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


resolve_module_path_collisions(Decls0, Decls) :-
    findall(Ref-Segs, member(rel_path_decl(Ref, Segs), Decls0), Paths),
    ( Paths == []
    -> Decls = Decls0
    ; reserved_rel_names(Decls0, Paths, Reserved),
      foldl(disambiguate_module_path(Reserved), Paths, Decls0, Decls)
    ).

disambiguate_module_path(Reserved, Name/Arity-Segs, Decls0, Decls) :-
    ( memberchk(Name, Reserved)
    -> variant_sha1(Segs, Sha),
       sub_atom(Sha, 0, 16, _, Digest),
       atomic_list_concat([Name, '__', Digest], Digested),
       ( lookup_column_order(Name, Cols)
       -> record_cols(Digested, Cols)
       ; true
       ),
       maplist(rename_decl_ref(Name/Arity, Digested/Arity), Decls0, Decls)
    ; Decls = Decls0
    ).

rename_decl_ref(Old, New, Decl0, Decl) :-
    Decl0 =.. [F, Old | Args],
    memberchk(F, [col_type, kind, keep, keyed, rel_path_decl]),
    !,
    Decl =.. [F, New | Args].
rename_decl_ref(_, _, Decl, Decl).

reserved_rel_names(Decls, Paths, Reserved) :-
    findall(Name,
            ( declared_rel_name(Decls, Ref, Name),
              \+ memberchk(Ref-_, Paths)
            ; minted_rel_name(Decls, Name)
            ),
            Names),
    sort(Names, Reserved).

declared_rel_name(Decls, Name/Arity, Name) :-
    member(Decl, Decls),
    Decl =.. [F, Name/Arity | _],
    memberchk(F, [col_type, kind, keyed, keep]).
declared_rel_name(Decls, enum_decl(Name), Name) :-
    member(enum_decl(Name, _), Decls).

minted_rel_name(Decls, Minted) :-
    ( member(col_type(Parent/_, Col, option(_)), Decls),
      atomic_list_concat([Parent, '__', Col], Minted)
    ; member(col_type(_, _, option(Element)), Decls),
      atom(Element),
      atomic_list_concat(['__opt_', Element], Minted)
    ; member(enum_decl(Rel, Variants), Decls),
      ( tree_leaf(';', Variants, Variant),
        Variant =.. [Name | _],
        atomic_list_concat([Rel, Name], '_', Minted)
      ; tag_rel_name(Rel, Minted)
      )
    ).


normalize_relation_value_decls(Decls0, Decls) :-
    findall(Name,
            ( declared_column_type_name(Decls0, Name),
              relation_schema(Decls0, Name, _, _) ),
            Names0),
    sort(Names0, ValueNames),
    normalize_relation_value_decls(Decls0, ValueNames, [], Decls).

normalize_relation_value_decls([], _, _, []).
normalize_relation_value_decls([Head | Rest], VNames, Seen, Out) :-
    ( Head = col_type(Name/Arity, _, _),
      memberchk(Name, VNames),
      \+ memberchk(Name, Seen)
    -> relation_schema([Head | Rest], Name, Name/Arity, Specs),
       Out = [type_decl(Name, Specs), Head | More],
       Seen1 = [Name | Seen]
    ; Out = [Head | More], Seen1 = Seen
    ),
    normalize_relation_value_decls(Rest, VNames, Seen1, More).

relation_schema(Decls, Name, Ref, Specs) :-
    once(member(col_type(Name/Arity, _, _), Decls)),
    Ref = Name/Arity,
    findall(col(Col, Type), member(col_type(Ref, Col, Type), Decls), Specs),
    length(Specs, Arity).

declared_column_type_name(Decls, Name) :-
    ( member(col_type(_, _, Type), Decls),
      ( Name = Type
      ; column_element_type_name(Type, Name)
      ; key_option_relation_type_name(Type, Name)
      )
    ; member(sh_decl(_, Ins, Outs, _), Decls),
      append(Ins, Outs, Cols),
      member(col(_, Name), Cols)
    ; member(enum_decl(_, Variants), Decls),
      tree_leaf(';', Variants, Variant),
      Variant =.. [_ | Fields],
      member(_:Name, Fields)
    ),
    \+ scalar_column_type(Name).

scalar_column_type(T) :- member(T, [int, text, json, bool, float, bytes]).

% key(option(Relation)) needs the relation mirror before option expansion.
% Descend only through key and nested option wrappers.
key_option_relation_type_name(key(Inner), Name) :- !,
    option_relation_type_name(Inner, Name).

option_relation_type_name(option(Inner), Name) :- !,
    option_relation_type_name(Inner, Name).
option_relation_type_name(Name, Name) :- atom(Name).


% RULING sh_bind_surface_removed: the whole statement is consumed with quotes
% and backticks respected, so a template's own `.` cannot end it early.
removed_world_decl_stmt([]) -->
    ( ~`sh` -> { Word = sh } ; ~`bind` -> { Word = bind } ),
    ws, consume_removed_statement,
    { unsupported(removed_word(Word)) }.

consume_removed_statement -->
    [C],
    ( { C == 0'. } -> []
    ; { memberchk(C, [0'`, 0'\', 0'"]) } -> skip_quoted_span(C), consume_removed_statement
    ; consume_removed_statement
    ).

skip_quoted_span(Quote) -->
    [C],
    ( { C == Quote } -> []
    ; { C == 0'\\ } -> ( [_] -> [] ; [] ), skip_quoted_span(Quote)
    ; skip_quoted_span(Quote)
    ).

host_output_columns(Rel, Specs) --> args(typed_col(host_col_type(Rel)), Specs).

host_col_type(Rel, Col, none) -->
    coltype(W), { W \== none },
    { unsupported(column_type_wrapper(Rel, Col, W)) }.
host_col_type(_, _, Type) --> type_expr(Type).

specs_to_columns(Specs, Cols) :- maplist([column(N, T), col(N, T)]>>true, Specs, Cols).

lex_token(template_lit/1, '`([^`\\\\]|\\\\.)*`').

template_lit(Template) -->
    [0'`], !,
    template_codes(Cs),
    { string_codes(Template, Cs) }.

template_codes([]) --> [0'`], !.
template_codes([0'` | Cs]) --> [0'\\, 0'`], !, template_codes(Cs).
template_codes([0'\\ | Cs]) --> [0'\\, 0'\\], !, template_codes(Cs).
template_codes([C | Cs]) --> [C], template_codes(Cs).


% A tail-free `?` keeps the query/1 term, so its emitted bytes cannot move.
query_stmt(Query) -->
    @`?`, ws, dotted_path(Segs), #`(`,
    head_args(Args), #`)`,
    { module_path_name(Segs, Name) },
    order_tail(Name, Args, OrderCols), #`.`,
    { path_atom(head, Segs, Args, Atom),
      ( OrderCols == [] -> Query = query(Atom)
      ; Query = query(Atom, order(OrderCols)) ) }.

% `order by defs desc, path` -- SQL words, `asc` when a direction is unwritten.
order_tail(Name, Args, OrderCols) -->
    ws, ~`order`, ws, ~`by`, !, ws,
    sep(order_col(Name, Args), OrderCols).
order_tail(_, _, []) --> [].

% The position resolves here against the QUERY's argument names, so it indexes
% the rel's own column list and no emitter repeats the lookup.
order_col(Name, Args, order_col(Position, Direction)) -->
    ident(Column), ws,
    ( ~`desc` -> { Direction = desc }
    ; ~`asc` -> { Direction = asc }
    ; { Direction = asc }
    ),
    { ( query_arg_position(Args, Column, Position)
      -> true
      ;  parse_failure(order_column_unknown(Name, Column))
      ) }.

query_arg_position(Args, Column, Position) :-
    nth1(Position, Args, Arg),
    query_arg_name(Arg, Column),
    !.

query_arg_name(named(Column, _), Column).
query_arg_name(pos(Value), Column) :-
    var(Value),
    variable_source_name(Value, Column).


match_stmt(match(Source, Arms)) -->
    ~`match`, ws, head_atom(Source), ws,
    @`(`, match_arms(Arms), #`)`, #`.`.

match_arms(Arms) -->
    ws, ( @`;` -> ws ; [] ),
    match_arm(First),
    match_arm_tail(First, Arms).

match_arm_tail(First, Arms) -->
    ws,
    ( @`;`
    -> ws, match_arm(Next),
       match_arm_tail(Next, Rest),
       { Arms = (First ; Rest) }
    ; { Arms = First }
    ).

match_arm(Arm) -->
    body(Guards), ws,
    ( @`|->` -> { Arrow = (<-) } ; @`|+>` -> { Arrow = (<+) } ),
    ws, head_atom(Head),
    { Arm =.. [Arrow, Head, Guards] }.

rule_stmt(Rule) -->
    head_atom(Head), ws,
    ( @`<-` -> { Arrow = (<-) }, ws, body(Body)
    ; @`<+` -> { Arrow = (<+) }, ws, body(Body)
    ; { Arrow = (<-), Body = true }
    ),
    #`.`,
    { Rule =.. [Arrow, Head, Body] }.


head_atom(Term) -->
    dotted_path(Segs), #`(`,
    head_args(Args), #`)`,
    { path_atom(head, Segs, Args, Term) }.

% the one spelling of "dotted path plus args becomes a term", shared by head
% atoms and body atoms; a single segment stays plain, longer ones go rel_path.
path_atom(Mode, Segs, Args, Term) :-
    module_path_name(Segs, Resolved),
    resolve_named_args(Mode, Resolved, Args, Pos),
    ( Segs = [Name] -> Term =.. [Name | Pos] ; Term = rel_path(Segs, Pos) ).

head_args(Args) --> args(atom_arg, Args).

atom_arg(named(Name, Value)) -->
    ident(Name), ws,
    here([0':, Next | _]), { Next \== 0'=, Next \== 0': }, !,
    @`:`, ws, expr(Value).
atom_arg(pos(Value)) --> expr(Value).


% A SHORT all-positional body call puns by name when EVERY argument is a
% capitalized variable naming a declared column (user 2026-08-22: "only when
% all puns are matching cap first, otherwise its ambiguous"). A full-arity
% call stays positional; a short call with one non-punning argument stays
% positional and lands on the arity check.
resolve_named_args(body, Rel, Args, Pos) :-
    \+ member(named(_, _), Args),
    lookup_column_order(Rel, Cols),
    length(Args, ArgCount), length(Cols, ColCount), ArgCount < ColCount,
    activate_keyword_puns(Args, Cols, Resolved),
    forall(member(Arg, Resolved), Arg = named(_, _)),
    !,
    resolve_mixed_args(body, Rel, Resolved, Cols, Pos).
resolve_named_args(_, _, Args, Pos) :-
    \+ member(named(_, _), Args), !,
    maplist(arg_value, Args, Pos).
resolve_named_args(Mode, Rel, Args, Pos) :-
    ( lookup_column_order(Rel, Cols)
    -> activate_keyword_puns(Args, Cols, ResolvedArgs),
       resolve_mixed_args(Mode, Rel, ResolvedArgs, Cols, Pos)
    ; unsupported(named_args_unresolved(Rel)),
      maplist(arg_value, Args, Pos)
    ).

arg_value(pos(V), V) :- !.
arg_value(named(_, V), V).

% In mixed calls, `Name` puns `name: Name` when lowercasing its first letter
% names a column. Fully positional and unmatched arguments retain their order.
capitalized_keyword_pun(Name, Column) :-
    atom_chars(Name, [First | Rest]),
    char_type(First, upper),
    downcase_atom(First, Lower),
    atom_chars(Column, [Lower | Rest]).
% `PollPeriod` also puns `poll_period`: the camel form is how every rule in
% the corpus spells a multi-word column variable.
capitalized_keyword_pun(Name, Column) :-
    atom_chars(Name, [First | Rest]),
    char_type(First, upper),
    once(( member(Upper, Rest), char_type(Upper, upper) )),
    snake_chars([First | Rest], SnakeChars),
    atom_chars(Column, SnakeChars).

snake_chars([First | Rest], [Lower | More]) :-
    downcase_atom(First, LowerAtom), atom_chars(LowerAtom, [Lower]),
    snake_tail(Rest, More).

snake_tail([], []).
snake_tail([C | Rest], Out) :-
    (   char_type(C, upper)
    ->  downcase_atom(C, LowerAtom), atom_chars(LowerAtom, [Lower]),
        snake_tail(Rest, More), Out = ['_', Lower | More]
    ;   snake_tail(Rest, More), Out = [C | More]
    ).

activate_keyword_puns([], _, []).
activate_keyword_puns([pos(Value) | Rest], Cols, [Arg | More]) :-
    var(Value),
    variable_source_name(Value, Name),
    capitalized_keyword_pun(Name, Column),
    memberchk(Column, Cols),
    !,
    Arg = named(Column, Value),
    activate_keyword_puns(Rest, Cols, More).
activate_keyword_puns([Arg | Rest], Cols, [Arg | More]) :-
    activate_keyword_puns(Rest, Cols, More).

variable_source_name(Value, Name) :-
    b_getval(dl_vars, Vars),
    member(Name-Existing, Vars),
    Existing == Value,
    !.

resolve_mixed_args(Mode, Rel, Args, Cols, Pos) :-
    length(Cols, N),
    length(Pos, N),
    validate_named_columns(Rel, Args, Cols),
    maplist(place_named(Args), Cols, Pos),
    findall(Col, member(named(Col, _), Args), NamedCols),
    findall(I, ( nth1(I, Cols, Col), \+ memberchk(Col, NamedCols) ), FreeIdxs),
    positional_values(Args, PosValues),
    fill_partial_slots(Mode, Rel, N, FreeIdxs, PosValues, Pos).

% Recursive collection preserves variable identity; findall/3 would copy the
% positional variables away from matching head variables.
positional_values([], []).
positional_values([pos(Value) | Rest], [Value | Values]) :-
    !,
    positional_values(Rest, Values).
positional_values([_ | Rest], Values) :-
    positional_values(Rest, Values).

validate_named_columns(Rel, Args, Cols) :-
    findall(Name, member(named(Name, _), Args), Names),
    ( member(Name, Names), \+ memberchk(Name, Cols)
    -> unsupported(unknown_named_arg(Rel, Name))
    ; select(Dup, Names, Rest), memberchk(Dup, Rest)
    -> unsupported(duplicate_named_arg(Rel, Dup))
    ; true
    ).

% Pos is already a fresh list as long as Cols, so maplist/3 pairs column to
% slot and the hand-rolled index walk disappears.
place_named(Args, Col, Slot) :-
    ( member(named(Col, V), Args) -> Slot = V ; true ).

fill_free_slots(Is, Vs, Pos) :-
    maplist({Pos}/[I, V]>>nth1(I, Pos, V), Is, Vs).

fill_partial_slots(Mode, Rel, Arity, FreeIdxs, PosValues, Pos) :-
    same_length(PosValues, FilledIdxs),
    append(FilledIdxs, OmittedIdxs, FreeIdxs),
    fill_free_slots(FilledIdxs, PosValues, Pos),
    finish_omitted_slots(Mode, Rel/Arity, OmittedIdxs, Pos).

% anonymous slots are free slots whose value list maplist/3 invents, so the
% second argument stays unbound and fill_anonymous_slots/2 is not needed.
finish_omitted_slots(Mode, Ref, Idxs, Pos) :-
    ( Mode == head, Idxs \== [] -> unsupported(partial_head(Ref)) ; true ),
    fill_free_slots(Idxs, _, Pos).


body(Body) -->
    ws,
    ( @`(` -> body(Inner), #`)`
    ; body_item(Inner)
    ),
    ws,
    ( @`,` -> ws, body(Rest), { Body = (Inner, Rest) }
    ; { Body = Inner }
    ).


body_item(Item) --> cst_item(Item), !.
body_item(Item) -->
    { Name = pre, Arity = 2, Shape = rel_atom_default
    ; surface(Name/Arity, _, _, LowerRole, _),
      wrapper_lower_role(LowerRole, Shape, _)
    },
    kw(Name), #`(`, balanced(Inner),
    { parse_surface_wrapper(Shape, Arity, Inner, Args) },
    !,
    { Item =.. [Name | Args] }.
body_item(Name) -->
    { surface(Name/0, _, _, word(_), _) }, kw(Name), !.
body_item(Item) --> infix_item(infix_op(bind), Item), !.
body_item(Item) --> infix_item(cmp_op, Item), !.
body_item(not(Atom)) -->
    @`!`, dotted_path(Segs), #`(`,
    head_args(Args), #`)`, !,
    { path_atom(body, Segs, Args, Atom) }.
body_item(Item) --> relatom_item(Item).


cst_item(cst(Path, Digest, Language, Query)) -->
    ~`cst`, #`(`,
    expr(Path), #`,`,
    expr(Digest), #`,`,
    ws, ident(Language), #`)`,
    #`{`,
    cst_block(Inner), here(S),
    { parse_cst_query_or_error(Inner, S, Query) }.

parse_cst_query_or_error(Codes, Rem, Query) :-
    catch(parse_cst_query(Codes, Query), _,
          ( mark(Rem), parse_failure(cst_query) )),
    !.

cst_block([]) --> [0'}], !.
cst_block(Codes) -->
    [0'"], cst_block_string(Str), !, cst_block(More),
    { append([0'" | Str], More, Codes) }.
cst_block([C | More]) --> [C], cst_block(More).

cst_block_string([0'"]) --> [0'"], !.
cst_block_string([0'\\, C | More]) --> [0'\\, C], !, cst_block_string(More).
cst_block_string([C | More]) --> [C], cst_block_string(More).

annotate_cst_item(Vars, Rule0, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    term_variables((Head, Body), RVars),
    map_tree(',', annotate_cst_leaf(Head, Vars, RVars), Body, Annotated),
    Rule =.. [Op, Head, Annotated].
annotate_cst_item(Vars, match(Source, Arms), match(Source, Annotated)) :- !,
    map_tree(';', annotate_cst_item(Vars), Arms, Annotated).
annotate_cst_item(_, Item, Item).

annotate_cst_leaf(Head, Vars, RVars,
                  cst(Path, Digest, Language, Query),
                  cst(Path, Digest, Language, Query,
                      cst_bindings(Caps, Cands, RNames))) :- !,
    ts_query_capture_names(Query, Caps),
    term_variables((Path, Digest), IVars),
    cst_variable_names(RVars, Vars, RNames),
    cst_variable_names(IVars, Vars, InNames),
    cst_body_variable_names(Head, Vars, InNames, Cands).
annotate_cst_leaf(_, _, _, Item, Item).

% the caller already derived InNames from (Path, Digest), and append(L, L, LL)
% then sort/2 is sort/2, so the exclusion set is just the sorted InNames.
cst_body_variable_names(Head, Vars, InNames, Names) :-
    term_variables(Head, HVars),
    cst_variable_names(HVars, Vars, HNames),
    sort(InNames, Excluded),
    subtract(HNames, Excluded, WithoutInputs),
    subtract(WithoutInputs, [line, end_line], Names).

cst_variable_names([], _, []).
cst_variable_names([Var | Rest], Vars, Names) :-
    ( member(Name-Candidate, Vars), Candidate == Var
    -> Names = [Name | More]
    ; Names = More
    ),
    cst_variable_names(Rest, Vars, More).


balanced(Inner, S0, S) :- bp(S0, 0, [], Rev, S), reverse(Rev, Inner).

bp([C | T], D, A, Out, S) :-
    ( memberchk(C, `"'`)
    -> bp_quoted(C, T, [C | A], A1, S1), bp(S1, D, A1, Out, S)
    ; C == 0'), D == 0
    -> Out = A, S = T
    ; ( C == 0'( -> D1 is D + 1 ; C == 0') -> D1 is D - 1 ; D1 = D ),
      bp(T, D1, [C | A], Out, S)
    ).

bp_quoted(Q, [C | T], A, Out, S) :-
    ( C == Q, T = [Q | T1] -> bp_quoted(Q, T1, [Q, Q | A], Out, S)
    ; C == Q -> Out = [Q | A], S = T
    ; C == 0'\\, T = [E | T1] -> bp_quoted(Q, T1, [E, 0'\\ | A], Out, S)
    ; bp_quoted(Q, T, [C | A], Out, S)
    ).

parse_full(Goal, Codes) :-
    call(Goal, Codes, Left),
    ws(Left, Left1),
    ( Left1 == [] -> true ; throw(dl_parse_error(trailing_input(Left1))) ).

% maplist/3 runs arg_value/2 backwards to lift positional exprs into the pos/1
% shape path_atom/4 reads; a wrapped rel atom resolves as a bare body one does.
rel_atom_term(Term) -->
    dotted_path(Segments), #`(`,
    args(expr, Values), #`)`,
    { maplist(arg_value, Args, Values),
      path_atom(body, Segments, Args, Term) }.

comma_pair(P1, P2, A, B) -->
    ws, call(P1, A), #`,`, ws, call(P2, B).

parse_surface_wrapper(atom_list, Arity, Codes, Atoms) :-
    parse_full(sep(rel_atom_term, Atoms), Codes),
    ( Arity == variadic -> true ; integer(Arity), length(Atoms, Arity) ).
parse_surface_wrapper(Shape, Arity, Codes, Args) :-
    wrapper_parser(Shape, Arity, Args, Goal),
    parse_full(Goal, Codes).

wrapper_parser(Shape, 1, [Arg], call(Parser, Arg)) :-
    memberchk(Shape-Parser,
              [rel_atom-rel_atom_term, body_item-body_item, expr-expr]).
wrapper_parser(expr_pair, 2, [A, B], comma_pair(expr, expr, A, B)).
wrapper_parser(rel_atom_default, 2, [Atom, D],
               comma_pair(rel_atom_term, expr, Atom, D)).


% infix_item//2: the operator nonterminal arrives partially applied via call//2
infix_item(OpParser, Term) -->
    expr(Lhs), ws, call(OpParser, Op), ws, expr(Rhs),
    { Term =.. [Op, Lhs, Rhs] }.

cmp_op(=<) --> @`<=`, !.
cmp_op(\==) --> @`!=`, !.
cmp_op(Op) --> infix_op(guard, Op), !.
cmp_op(==) --> @`=`.

infix_op(Axis, Op) -->
    { findall(C, surface(C/2, Axis, no_refs, infix(_), _), Cands),
      longest_first(Cands, Ordered),
      member(Op, Ordered),
      atom_codes(Op, Cs) },
    op_codes(Cs).

neg_length(Atom, NegLen) :- atom_length(Atom, Len), NegLen is -Len.
longest_first(Atoms, Sorted) :-
    map_list_to_pairs(neg_length, Atoms, Keyed),
    keysort(Keyed, Ordered),
    pairs_values(Ordered, Sorted).

op_codes([F | R]) -->
    ( { code_type(F, alpha) } -> ~[F | R] ; @[F | R] ).


% append/3 against an InCount-long prefix already fails when Values is short,
% so the explicit length comparison is redundant.
split_probe_values(InCount, Values, Ins, Outs) :-
    ( length(Ins, InCount), append(Ins, Outs, Values)
    -> true
    ; Ins = Values, Outs = []
    ).

partition_hiv([], [], [], [], []).
partition_hiv([col(Name, _) | Cols], [V | Vs], [Role | Roles], Ids, Salts) :-
    ( Role == identity -> Ids = [V | Ids1], Salts = Salts1
    ; Role == freshness -> Salts = [salt(Name, V) | Salts1], Ids = Ids1
    ),
    partition_hiv(Cols, Vs, Roles, Ids1, Salts1).

relatom_item(Item) -->
    dotted_path(Segs), ws,
    ( @`!` -> { Mut = true }, ws ; { Mut = false } ),
    @`(`, head_args(Args), #`)`,
    { ( Mut == true
      -> last(Segs, Name),
         length(Args, Arity),
         unsupported(mutation(Name/Arity)),
         module_path_name(Segs, Resolved),
         resolve_named_args(body, Resolved, Args, Pos),
         Item =.. [Name | Pos]
      ; path_atom(body, Segs, Args, Item)
      ) }.


expr(E) --> { arithmetic_tiers(Tiers) }, tier_expr(Tiers, E).

arithmetic_tiers(Tiers) :-
    findall(P, expression(_/2, arithmetic, P, _, _), Ps),
    sort(Ps, Tiers).

tier_operators(Prec, Ops) :-
    findall(Op, expression(Op/2, arithmetic, Prec, _, _), Ops0),
    longest_first(Ops0, Ops).

tier_expr([], E) --> factor(E).
tier_expr([P | Tighter], E) -->
    tier_expr(Tighter, Acc),
    { tier_operators(P, Ops) },
    tier_rest(Ops, Tighter, Acc, E).

tier_rest(Ops, Tighter, Acc, E) -->
    here(Start), ws,
    ( tier_op(Ops, Op)
    -> ws, tier_expr(Tighter, Rhs),
       { Next =.. [Op, Acc, Rhs] },
       tier_rest(Ops, Tighter, Next, E)
    ; { E = Acc }, back(Start)
    ).

tier_op([Op | Rest], Matched) -->
    { atom_codes(Op, Cs) },
    ( op_codes(Cs) -> { Matched = Op } ; tier_op(Rest, Matched) ).


factor(E) -->
    ws, here(S), { no_tagged_brace(S) },
    ( @`(` -> ws, expr(E), #`)`
    ; bool_lit(E) -> []
    ; float_lit(E) -> []
    ; int_lit(E) -> []
    ; atom_lit(E) -> []
    ; string_lit(E) -> []
    ; dollar_var(E)
    ; json_literal(E) -> []
    ; braces_term(E)
    ; list_term(E)
    ; wildcard_var(E) -> []
    ; compound_or_var(E)
    ).

% Native JSON keeps a separate AST from object patterns and relation-value
% braces. A JSON object is selected by its required double-quoted key; bare,
% capture, descent, and typed brace pairs continue through braces_term//1.
json_literal(json_object(Pairs)) -->
    @`{`, ws, string_lit(KeyText), #`:`, ws, json_value(Value),
    { atom_string(Key, KeyText) },
    json_object_pairs(Key-Value, Pairs), #`}`.
json_literal(json_array(Values)) -->
    @`[`, ws, json_value(First), json_array_rest(First, Values), #`]`.

json_object_pairs(First, Pairs) -->
    ws,
    ( @`,` -> ws, string_lit(KeyText), #`:`, ws, json_value(Value),
       { atom_string(Key, KeyText) }, json_object_pairs(Key-Value, Rest),
       { Pairs = [First | Rest] }
    ; { Pairs = [First] }
    ).

json_array_rest(First, Values) -->
    ws,
    ( @`,` -> ws, json_value(Value), json_array_rest(Value, Rest),
       { Values = [First | Rest] }
    ; { Values = [First] }
    ).

json_value(json_object(Pairs)) -->
    @`{`, ws,
    ( peek(0'}) -> { Pairs = [] }
    ; string_lit(KeyText), #`:`, ws, json_value(Value),
      { atom_string(Key, KeyText) }, json_object_pairs(Key-Value, Pairs)
    ), #`}`.
json_value(json_array(Values)) -->
    @`[`, ws,
    ( peek(0']) -> { Values = [] }
    ; json_value(First), json_array_rest(First, Values)
    ), #`]`.
json_value(json_null) --> ~`null`.
json_value(Value) --> bool_lit(Value).
json_value(Value) --> float_lit(Value).
json_value(Value) --> int_lit(Value).
json_value(Value) --> string_lit(Value).

no_tagged_brace(S) :-
    ( ident(Name, S, [0'{ | _])
    -> throw(unsupported_construct(tagged_brace_reserved(Name)))
    ; true
    ).

dollar_var(Var) -->
    [0'$], ident(Name),
    { hole_var(Name, Var) }.

bool_lit(bool_lit(B)) -->
    { member(B, [true, false]) }, kw(B), !.

wildcard_var(_) --> ~`_`.

compound_or_var(E) -->
    here(Start),
    ( dotted_path(Segs), ws, peek(0'()
    -> @`(`, args(expr, Args), #`)`,
       { expression_path_application(Segs, Args, E) }
    ; back(Start), ident(Name), here(AfterName), ws,
      { get_or_make_var(Name, Rec) },
      back(AfterName), dot_chain(Rec, E)
    ).

expression_path_application([Name], Args, E) :-
    !,
    E =.. [Name | Args].
expression_path_application(Segs, Args, rel_path(Segs, Args)).

% Slash and dot are one path spelling (ruling executor_modules_use_import) and
% both reach module_path_name/2's `__` join, as `use soopy.` plus a leaf does.
dotted_path(Segments) -->
    ( peek(0'/) -> slash_path(Segments) ; dotted_path_segments(Segments) ).

dotted_path_segments([Segment | Rest]) -->
    ident(Segment),
    ( dot_then_ident -> dotted_path_segments(Rest) ; { Rest = [] } ).

slash_path([Segment | Rest]) -->
    slash_then_ident, ident(Segment),
    ( peek(0'/) -> slash_path(Rest) ; { Rest = [] } ).

slash_then_ident([0'/ | S], S) :-
    S = [C | _],
    ( code_type(C, alpha) ; C == 0'_ ),
    !.

dot_chain(Rec, Final) -->
    ( dot_then_ident
    -> ident(Field), dot_chain(dot_get(Rec, Field), Final)
    ; { Final = Rec }
    ).

dot_then_ident([0'. | S], S) :-
    S = [C | _],
    ( code_type(C, alpha) ; C == 0'_ ),
    !.


braces_term(Term) -->
    @`{`, !, ws,
    ( peek(0'})
    -> { Term = '{}' }
    ; { Term = '{}'(Pairs) },
      brace_pairs(Pairs)
    ),
    #`}`.

brace_pairs((Pair, Rest)) -->
    brace_pair(Pair), #`,`, !, ws,
    brace_pairs(Rest).
brace_pairs(Pair) --> brace_pair(Pair).

brace_pair(Key:Typed) -->
    brace_key(Key), #`:`, ws,
    expr(Value),
    ( #`:` -> ws, ident(Type), { Typed = Value:Type } ; { Typed = Value } ).

brace_key('**') --> @`**`, !.
brace_key($(Var)) -->
    [0'$], !, ident(Name),
    { hole_var(Name, Var) }.
brace_key(Key) --> atom_lit(Key), !.
brace_key(Key) --> string_lit(Text), !, { atom_string(Key, Text) }.
brace_key(Key) --> ident(Key).


list_term(Term) -->
    @`[`, !, ws,
    ( @`...` -> ws, expr(Element), { Term = spread(Element) }
    ; peek(0']) -> { Term = [] }
    ; sep(expr, Term)
    ),
    #`]`.
