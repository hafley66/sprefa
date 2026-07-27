% astgrep_patterns.pl : the quoted-DSL pipeline for ast-grep-style patterns,
% end to end, in one file.
%
%   Run:   swipl -q -l v6/prolog/labs/astgrep_patterns.pl -g go -g halt
%
% The four passes a quoted DSL owes (surface-boil.md), each one graded:
%
%   IMPORT   node_types_fixture.json  ->  node_kind/1, node_field/3,
%            node_children/2 + the multiplicity flags. This is the ONLY
%            grammar knowledge in the file; nothing below hardcodes JS.
%   PARSE    a DCG over ast-grep pattern text -> pattern terms.
%            `foo($A)`, `$OBJ.method($$$)`, `foo($A, $$$REST)`.
%            Also reachable as SWI quasiquotation: {|sg||foo($A)|}.
%   CHECK    pattern term x grammar facts -> annotated pattern term, or a
%            named refusal. Every node position in a pattern gets a concrete
%            node kind resolved from the field's declared type list; a
%            pattern that names an impossible shape has no such resolution.
%   LOWER    ONE annotated term, TWO backends (the babel two-path):
%              1. reference: match by unification against a CST term.
%                 A metavariable is a hole, non-linear is the same hole
%                 twice, $$$ is a sibling-list hole.
%              2. emission: a tree-sitter query s-expression string with
%                 @capture syntax, for handing to the native matcher.
%            The two are NOT equally expressive and the lab grades the gap:
%            emission REFUSES a named $$$ because tree-sitter queries have
%            no list capture.
%
% Deviations from the sexp_cst.pl shape are listed in astgrep_patterns.md.

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(http/json)).
:- use_module(library(quasi_quotations)).

:- dynamic fixture_path/1.
:- dynamic node_kind/1.
:- dynamic node_field/3.
:- dynamic node_field_multiple/3.
:- dynamic node_children/2.
:- dynamic node_children_multiple/2.

:- prolog_load_context(directory, LabDir),
   atomic_list_concat([LabDir, '/node_types_fixture.json'], FixtureFile),
   assertz(fixture_path(FixtureFile)).

% ═════════════════════════════════════════════════════════════════════════════
% PASS 1 — GRAMMAR IMPORT : node-types.json -> facts
% ═════════════════════════════════════════════════════════════════════════════
%
% Real node-types.json is a flat array of entries. Anonymous entries
% ("named": false, the punctuation) carry no structure and are dropped: a
% pattern never names them, and keeping them would let the checker accept
% `(` as a node kind.

load_grammar :-
    retractall(node_kind(_)),
    retractall(node_field(_, _, _)),
    retractall(node_field_multiple(_, _, _)),
    retractall(node_children(_, _)),
    retractall(node_children_multiple(_, _)),
    fixture_path(FixtureFile),
    setup_call_cleanup(open(FixtureFile, read, Stream),
                       json_read_dict(Stream, Entries),
                       close(Stream)),
    forall(member(Entry, Entries), import_entry(Entry)).

import_entry(Entry) :-
    get_dict(named, Entry, false), !.          % punctuation: not a node kind
import_entry(Entry) :-
    get_dict(type, Entry, KindText),
    atom_string(Kind, KindText),
    assertz(node_kind(Kind)),
    import_fields(Kind, Entry),
    import_children(Kind, Entry).

import_fields(Kind, Entry) :-
    (   get_dict(fields, Entry, FieldDict)
    ->  dict_pairs(FieldDict, _, Pairs),
        forall(member(FieldName-Slot, Pairs), import_field(Kind, FieldName, Slot))
    ;   true
    ).

import_field(Kind, FieldName, Slot) :-
    slot_kinds(Slot, AllowedKinds),
    get_dict(multiple, Slot, Multiple),
    assertz(node_field(Kind, FieldName, AllowedKinds)),
    assertz(node_field_multiple(Kind, FieldName, Multiple)).

import_children(Kind, Entry) :-
    (   get_dict(children, Entry, Slot)
    ->  slot_kinds(Slot, AllowedKinds),
        get_dict(multiple, Slot, Multiple),
        assertz(node_children(Kind, AllowedKinds)),
        assertz(node_children_multiple(Kind, Multiple))
    ;   true
    ).

% Only named types in a slot's type list are reachable from a pattern.
slot_kinds(Slot, AllowedKinds) :-
    get_dict(types, Slot, TypeEntries),
    findall(Kind,
            ( member(TypeEntry, TypeEntries),
              get_dict(named, TypeEntry, true),
              get_dict(type, TypeEntry, KindText),
              atom_string(Kind, KindText) ),
            AllowedKinds).

:- load_grammar.

% Every kind mentioned in any slot must itself be a declared kind. A real
% node-types.json satisfies this by construction; a hand-trimmed subset does
% not, and an unclosed grammar makes the checker silently reject good
% patterns instead of naming the missing kind.
grammar_dangling_kinds(Dangling) :-
    findall(Kind,
            ( ( node_field(_, _, AllowedKinds) ; node_children(_, AllowedKinds) ),
              member(Kind, AllowedKinds),
              \+ node_kind(Kind) ),
            Dangling0),
    sort(Dangling0, Dangling).

% The one bridge the grammar cannot supply: which kinds a bare pattern name
% is allowed to denote. Real ast-grep gets this for free because it parses
% the pattern text WITH the target grammar, so every pattern token already
% carries a kind. A hand-rolled pattern DCG has to say it out loud.
pattern_leaf_kind(identifier).
pattern_leaf_kind(property_identifier).

% ═════════════════════════════════════════════════════════════════════════════
% PASS 2 — PATTERN SYNTAX : a DCG over ast-grep pattern text
% ═════════════════════════════════════════════════════════════════════════════
%
%   pattern  ::= primary trailer*
%   trailer  ::= '.' name | '(' args ')'
%   primary  ::= '$$$' UPPER*  | '$' UPPER+ | string | number | name
%   args     ::= (pattern (',' pattern)*)?
%
% $UPPER binds one node (pmeta), $$$ binds a sibling list (pellipsis), an
% anonymous $$$ binds nothing and is written pellipsis(anon).

parse_pattern(PatternText, Pattern) :-
    text_to_string(PatternText, Text),
    string_codes(Text, Codes),
    once(phrase((pattern_ws, pattern_expr(Pattern), pattern_ws), Codes)).

pattern_expr(Pattern) -->
    pattern_primary(Head),
    pattern_trailers(Head, Pattern).

pattern_trailers(Accumulated, Pattern) -->
    ".", pattern_ws, pattern_name(PropertyName), !,
    pattern_trailers(pmember(Accumulated, pident(PropertyName)), Pattern).
pattern_trailers(Accumulated, Pattern) -->
    "(", pattern_ws, pattern_args(Arguments), pattern_ws, ")", !,
    pattern_trailers(pcall(Accumulated, Arguments), Pattern).
pattern_trailers(Pattern, Pattern) --> [].

pattern_primary(pellipsis(Name)) -->
    "$$$", pattern_meta_name_opt(Name).
pattern_primary(pmeta(Name)) -->
    "$", pattern_meta_name(Name).
pattern_primary(pstring(Value)) -->
    "\"", pattern_string_body(Codes), "\"", { atom_codes(Value, Codes) }.
pattern_primary(pnumber(Value)) -->
    pattern_digits(Codes), { Codes \== [], number_codes(Value, Codes) }.
pattern_primary(pident(Name)) -->
    pattern_name(Name).

pattern_args([Argument | Rest]) -->
    pattern_expr(Argument), !, pattern_args_rest(Rest).
pattern_args([]) --> [].

pattern_args_rest([Argument | Rest]) -->
    pattern_ws, ",", pattern_ws, pattern_expr(Argument), !, pattern_args_rest(Rest).
pattern_args_rest([]) --> [].

pattern_meta_name(Name) -->
    pattern_meta_codes(Codes), { Codes \== [], atom_codes(Name, Codes) }.

pattern_meta_name_opt(Name) -->
    pattern_meta_codes(Codes),
    { ( Codes == [] -> Name = anon ; atom_codes(Name, Codes) ) }.

pattern_meta_codes([Code | Rest]) -->
    [Code], { pattern_meta_char(Code) }, !, pattern_meta_codes(Rest).
pattern_meta_codes([]) --> [].

pattern_meta_char(Code) :- code_type(Code, upper).
pattern_meta_char(Code) :- code_type(Code, digit).
pattern_meta_char(0'_).

pattern_name(Name) -->
    pattern_csyms(Codes), { Codes \== [], atom_codes(Name, Codes) }.

pattern_csyms([Code | Rest]) -->
    [Code], { code_type(Code, csym) }, !, pattern_csyms(Rest).
pattern_csyms([]) --> [].

pattern_digits([Code | Rest]) -->
    [Code], { code_type(Code, digit) }, !, pattern_digits(Rest).
pattern_digits([]) --> [].

pattern_string_body([Code | Rest]) -->
    [Code], { Code \== 0'" }, !, pattern_string_body(Rest).
pattern_string_body([]) --> [].

pattern_ws --> [Code], { code_type(Code, space) }, !, pattern_ws.
pattern_ws --> [].

% ── surface embedding option A: SWI quasiquotation ──────────────────────────
% Must be defined before any clause that USES {|sg|| ... |}, because the
% quotation is expanded at read time.

:- quasi_quotation_syntax(sg).

sg(Content, _SyntaxArgs, _VariableNames, Pattern) :-
    with_quasi_quotation_input(Content, Stream, read_string(Stream, _, Text)),
    parse_pattern(Text, Pattern).

% ═════════════════════════════════════════════════════════════════════════════
% PASS 3 — CHECK : pattern term x grammar facts -> annotated term | refusal
% ═════════════════════════════════════════════════════════════════════════════
%
% Result is ok(Annotated) or bad(Reason); the predicate is total, so a
% refusal carries a name instead of just failing. Annotated terms are
% ann(Kind, Shape) with the same Shape constructors as the parse output.
% Kind is `any` for a metavariable and `any_list` for an ellipsis.
%
% AllowedKinds is the enclosing slot's declared type list, or any_kind at
% the root. This is a typing environment made entirely of grammar facts.

check_top(Pattern, Result) :- check_pattern(Pattern, any_kind, Result).

check_pattern(pmeta(Name), _AllowedKinds, ok(ann(any, pmeta(Name)))).

check_pattern(pellipsis(Name), _AllowedKinds, bad(ellipsis_outside_list(Name))).

check_pattern(pident(Name), AllowedKinds, Result) :-
    (   resolve_leaf_kind(AllowedKinds, Kind)
    ->  Result = ok(ann(Kind, pident(Name)))
    ;   Result = bad(no_name_kind_here(Name, AllowedKinds))
    ).

check_pattern(pstring(Value), AllowedKinds, Result) :-
    (   kind_allowed(string, AllowedKinds)
    ->  Result = ok(ann(string, pstring(Value)))
    ;   Result = bad(kind_not_allowed(string, AllowedKinds))
    ).

check_pattern(pnumber(Value), AllowedKinds, Result) :-
    (   kind_allowed(number, AllowedKinds)
    ->  Result = ok(ann(number, pnumber(Value)))
    ;   Result = bad(kind_not_allowed(number, AllowedKinds))
    ).

check_pattern(pcall(Callee, Arguments), AllowedKinds, Result) :-
    (   \+ kind_allowed(call_expression, AllowedKinds)
    ->  Result = bad(kind_not_allowed(call_expression, AllowedKinds))
    ;   node_field(call_expression, function, CalleeAllowed),
        check_pattern(Callee, CalleeAllowed, CalleeResult),
        (   CalleeResult = bad(CalleeReason)
        ->  Result = bad(CalleeReason)
        ;   CalleeResult = ok(CalleeAnn),
            node_field(call_expression, arguments, ArgsSlotKinds),
            memberchk(ArgsKind, ArgsSlotKinds),
            node_children(ArgsKind, ChildAllowed),
            node_children_multiple(ArgsKind, ChildMultiple),
            check_list(Arguments, ChildAllowed, ChildMultiple, ArgsResult),
            (   ArgsResult = bad(ArgsReason)
            ->  Result = bad(ArgsReason)
            ;   ArgsResult = ok(ArgumentAnns),
                Result = ok(ann(call_expression, pcall(CalleeAnn, ArgumentAnns)))
            )
        )
    ).

check_pattern(pmember(Object, Property), AllowedKinds, Result) :-
    (   \+ kind_allowed(member_expression, AllowedKinds)
    ->  Result = bad(kind_not_allowed(member_expression, AllowedKinds))
    ;   node_field(member_expression, object, ObjectAllowed),
        check_pattern(Object, ObjectAllowed, ObjectResult),
        (   ObjectResult = bad(ObjectReason)
        ->  Result = bad(ObjectReason)
        ;   ObjectResult = ok(ObjectAnn),
            node_field(member_expression, property, PropertyAllowed),
            check_pattern(Property, PropertyAllowed, PropertyResult),
            (   PropertyResult = bad(PropertyReason)
            ->  Result = bad(PropertyReason)
            ;   PropertyResult = ok(PropertyAnn),
                Result = ok(ann(member_expression, pmember(ObjectAnn, PropertyAnn)))
            )
        )
    ).

% An ellipsis is legal only where the grammar says the slot holds many.
check_list([], _AllowedKinds, _Multiple, ok([])).
check_list([pellipsis(Name) | Rest], AllowedKinds, Multiple, Result) :-
    !,
    (   Multiple \== true
    ->  Result = bad(ellipsis_in_single_slot(Name))
    ;   check_list(Rest, AllowedKinds, Multiple, RestResult),
        (   RestResult = bad(Reason)
        ->  Result = bad(Reason)
        ;   RestResult = ok(RestAnns),
            Result = ok([ann(any_list, pellipsis(Name)) | RestAnns])
        )
    ).
check_list([Pattern | Rest], AllowedKinds, Multiple, Result) :-
    check_pattern(Pattern, AllowedKinds, HeadResult),
    (   HeadResult = bad(Reason)
    ->  Result = bad(Reason)
    ;   HeadResult = ok(HeadAnn),
        check_list(Rest, AllowedKinds, Multiple, RestResult),
        (   RestResult = bad(RestReason)
        ->  Result = bad(RestReason)
        ;   RestResult = ok(RestAnns),
            Result = ok([HeadAnn | RestAnns])
        )
    ).

kind_allowed(Kind, any_kind) :- !, node_kind(Kind).
kind_allowed(Kind, AllowedKinds) :- memberchk(Kind, AllowedKinds).

resolve_leaf_kind(any_kind, Kind) :- !, pattern_leaf_kind(Kind).
resolve_leaf_kind(AllowedKinds, Kind) :-
    member(Kind, AllowedKinds),
    pattern_leaf_kind(Kind).

% ═════════════════════════════════════════════════════════════════════════════
% THE CST : sexp DCG (sexp_cst.pl) + a normalizing pass
% ═════════════════════════════════════════════════════════════════════════════
%
% Verbatim `tree-sitter parse` output shape for source_lines/1 below.

source_lines([ "f(a);"
             , "f(b, c);"
             , "log(x, x);"
             , "log(x, y);"
             , "obj.method(1, \"two\");"
             ]).

cst_sexp(Text) :-
    atomic_list_concat(
      [ '(program [0, 0] - [5, 0]'
      , '  (expression_statement [0, 0] - [0, 5]'
      , '    (call_expression [0, 0] - [0, 4]'
      , '      function: (identifier [0, 0] - [0, 1])'
      , '      arguments: (arguments [0, 1] - [0, 4]'
      , '        (identifier [0, 2] - [0, 3]))))'
      , '  (expression_statement [1, 0] - [1, 8]'
      , '    (call_expression [1, 0] - [1, 7]'
      , '      function: (identifier [1, 0] - [1, 1])'
      , '      arguments: (arguments [1, 1] - [1, 7]'
      , '        (identifier [1, 2] - [1, 3])'
      , '        (identifier [1, 5] - [1, 6]))))'
      , '  (expression_statement [2, 0] - [2, 10]'
      , '    (call_expression [2, 0] - [2, 9]'
      , '      function: (identifier [2, 0] - [2, 3])'
      , '      arguments: (arguments [2, 3] - [2, 9]'
      , '        (identifier [2, 4] - [2, 5])'
      , '        (identifier [2, 7] - [2, 8]))))'
      , '  (expression_statement [3, 0] - [3, 10]'
      , '    (call_expression [3, 0] - [3, 9]'
      , '      function: (identifier [3, 0] - [3, 3])'
      , '      arguments: (arguments [3, 3] - [3, 9]'
      , '        (identifier [3, 4] - [3, 5])'
      , '        (identifier [3, 7] - [3, 8]))))'
      , '  (expression_statement [4, 0] - [4, 21]'
      , '    (call_expression [4, 0] - [4, 20]'
      , '      function: (member_expression [4, 0] - [4, 10]'
      , '        object: (identifier [4, 0] - [4, 3])'
      , '        property: (property_identifier [4, 4] - [4, 10]))'
      , '      arguments: (arguments [4, 10] - [4, 20]'
      , '        (number [4, 11] - [4, 12])'
      , '        (string [4, 14] - [4, 19])))))'
      ], '\n', Text).

sexp_cst(Root) :-
    cst_sexp(Text), atom_codes(Text, Codes),
    once(phrase((sexp_ws, sexp_node(Root), sexp_ws), Codes)).

sexp_node(snode(Kind, span(StartRow, StartCol, EndRow, EndCol), Items)) -->
    "(", sexp_ident(Kind), sexp_ws,
    sexp_span(StartRow, StartCol, EndRow, EndCol),
    sexp_items(Items), sexp_ws, ")".

sexp_items([Item | Rest]) --> sexp_ws, sexp_item(Item), !, sexp_items(Rest).
sexp_items([]) --> [].

sexp_item(field(Name, Node)) --> sexp_ident(Name), ":", sexp_ws, sexp_node(Node).
sexp_item(Node) --> sexp_node(Node).

sexp_span(StartRow, StartCol, EndRow, EndCol) -->
    "[", sexp_int(StartRow), ",", sexp_ws, sexp_int(StartCol), "]", sexp_ws,
    "-", sexp_ws,
    "[", sexp_int(EndRow), ",", sexp_ws, sexp_int(EndCol), "]".

sexp_ident(Name) --> sexp_csyms(Codes), { Codes \== [], atom_codes(Name, Codes) }.
sexp_csyms([Code | Rest]) --> [Code], { code_type(Code, csym) }, !, sexp_csyms(Rest).
sexp_csyms([]) --> [].
sexp_int(Number) --> sexp_digits(Codes), { Codes \== [], number_codes(Number, Codes) }.
sexp_digits([Code | Rest]) --> [Code], { code_type(Code, digit) }, !, sexp_digits(Rest).
sexp_digits([]) --> [].
sexp_ws --> [Code], { code_type(Code, space) }, !, sexp_ws.
sexp_ws --> [].

% ── normalize : snode(Kind, Span, Items) -> node(Kind, Fields, Children) ────
%
% Two deliberate moves, both explained in the .md:
%   - the span is RESOLVED to source text on leaves and then DROPPED, so
%     that structural equality of two node terms means "same subtree". That
%     equality is what makes a non-linear pattern work by unification alone.
%   - field items and bare child items are split into separate argument
%     positions, matching how node_field/3 and node_children/2 are shaped.

normalize_cst(snode(Kind, Span, Items), node(Kind, Fields, Children)) :-
    findall(FieldName-NormalChild,
            ( member(field(FieldName, Child), Items),
              normalize_cst(Child, NormalChild) ),
            FieldPairs),
    findall(NormalChild,
            ( member(Child, Items), Child = snode(_, _, _),
              normalize_cst(Child, NormalChild) ),
            Children),
    (   Items == []
    ->  node_span_text(Span, Text), Fields = [text-Text]
    ;   Fields = FieldPairs
    ).

node_span_text(span(Row, StartCol, Row, EndCol), Text) :-
    source_lines(Lines), nth0(Row, Lines, Line),
    Length is EndCol - StartCol,
    sub_string(Line, StartCol, Length, _, Text).

cst(Root) :- sexp_cst(Sexp), normalize_cst(Sexp, Root).

node_text(node(_, Fields, _), Text) :- memberchk(text-Text, Fields).

% ═════════════════════════════════════════════════════════════════════════════
% LOWERING 1 (reference) — matching by unification
% ═════════════════════════════════════════════════════════════════════════════
%
% Bindings are an association list Name-node_value(Node) / Name-list_value(Nodes).
% Re-binding an already-bound name demands term identity: that IS the
% non-linear pattern, and it is the same join the datalog tier does.

match(ann(any, pmeta(Name)), Node, Bindings0, Bindings) :-
    bind_node(Name, Node, Bindings0, Bindings).

match(ann(Kind, pident(Name)), node(Kind, Fields, _), Bindings, Bindings) :-
    memberchk(text-Text, Fields),
    atom_string(Name, Text).

match(ann(string, pstring(Value)), node(string, Fields, _), Bindings, Bindings) :-
    memberchk(text-Text, Fields),
    format(atom(Quoted), '"~w"', [Value]),
    atom_string(Quoted, Text).

match(ann(number, pnumber(Value)), node(number, Fields, _), Bindings, Bindings) :-
    memberchk(text-Text, Fields),
    number_string(Value, Text).

match(ann(call_expression, pcall(Callee, Arguments)),
      node(call_expression, Fields, _), Bindings0, Bindings) :-
    memberchk(function-CalleeNode, Fields),
    match(Callee, CalleeNode, Bindings0, Bindings1),
    memberchk(arguments-node(arguments, _, ArgumentNodes), Fields),
    match_list(Arguments, ArgumentNodes, Bindings1, Bindings).

match(ann(member_expression, pmember(Object, Property)),
      node(member_expression, Fields, _), Bindings0, Bindings) :-
    memberchk(object-ObjectNode, Fields),
    match(Object, ObjectNode, Bindings0, Bindings1),
    memberchk(property-PropertyNode, Fields),
    match(Property, PropertyNode, Bindings1, Bindings).

match_list([], [], Bindings, Bindings).
match_list([ann(any_list, pellipsis(Name)) | PatternRest], Nodes, Bindings0, Bindings) :-
    !,
    append(Taken, NodeRest, Nodes),
    match_list(PatternRest, NodeRest, Bindings0, Bindings1),
    bind_list(Name, Taken, Bindings1, Bindings).
match_list([Pattern | PatternRest], [Node | NodeRest], Bindings0, Bindings) :-
    match(Pattern, Node, Bindings0, Bindings1),
    match_list(PatternRest, NodeRest, Bindings1, Bindings).

bind_node(Name, Node, Bindings0, Bindings) :-
    (   memberchk(Name-Existing, Bindings0)
    ->  Existing == node_value(Node), Bindings = Bindings0
    ;   Bindings = [Name-node_value(Node) | Bindings0]
    ).

bind_list(anon, _Nodes, Bindings, Bindings) :- !.
bind_list(Name, Nodes, Bindings0, Bindings) :-
    (   memberchk(Name-Existing, Bindings0)
    ->  Existing == list_value(Nodes), Bindings = Bindings0
    ;   Bindings = [Name-list_value(Nodes) | Bindings0]
    ).

subnode(Node, Node).
subnode(node(_, Fields, Children), Descendant) :-
    (   member(_-Child, Fields), Child = node(_, _, _)
    ;   member(Child, Children)
    ),
    subnode(Child, Descendant).

find_matches(Annotated, Root, AllBindings) :-
    findall(Bindings,
            ( subnode(Root, Node), match(Annotated, Node, [], Bindings) ),
            AllBindings).

bound_text(Bindings, Name, Text) :-
    memberchk(Name-node_value(Node), Bindings),
    node_text(Node, Text).

bound_list_texts(Bindings, Name, Texts) :-
    memberchk(Name-list_value(Nodes), Bindings),
    findall(Text, ( member(Node, Nodes), node_text(Node, Text) ), Texts).

% ═════════════════════════════════════════════════════════════════════════════
% LOWERING 2 (emission) — the same term as a tree-sitter query string
% ═════════════════════════════════════════════════════════════════════════════
%
% Capture policy:
%   $NAME used once            -> @NAME
%   $NAME used N > 1 times     -> @NAME_1 .. @NAME_N plus (#eq? @NAME_i @NAME_i+1)
%   a literal name/number/str  -> @litK plus (#eq? @litK "text")
%   anonymous $$$              -> nothing, plus the ANCHOR rule below
%   named $$$                  -> BLOCKED. Tree-sitter queries capture nodes,
%                                 never sibling lists, so there is no string
%                                 that carries the binding.
%
% Anchor rule, and it is not optional. A tree-sitter node pattern matches its
% children as a SUBSEQUENCE: `(arguments (_) @A)` matches `f(a, b)` too. The
% reference matcher's match_list consumes the whole list, so `foo($A)` means
% exactly one argument. The `.` anchor is what closes that gap: it pins the
% first child, the last child, and adjacency between two non-ellipsis
% neighbours. An arg list of exactly [] stays inexpressible (a query cannot
% say "no children at all"), so that case is blocked rather than emitted wrong.

emit_query(Annotated, blocked(named_ellipsis(Name))) :-
    ellipsis_names(Annotated, Names),
    member(Name, Names), Name \== anon, !.
emit_query(Annotated, blocked(empty_child_list_inexpressible)) :-
    has_empty_child_list(Annotated), !.
emit_query(Annotated, query(Query)) :-
    meta_names(Annotated, Names),
    msort(Names, Sorted),
    clumped(Sorted, Totals),
    emit_shape(Annotated, Text, ShapePredicates, Totals, state(0, []), _State),
    nonlinear_predicates(Totals, EqualityPredicates),
    append(ShapePredicates, EqualityPredicates, Predicates),
    (   Predicates == []
    ->  Query = Text
    ;   atomic_list_concat([Text | Predicates], ' ', Inner),
        format(atom(Query), '(~w)', [Inner])
    ).

emit_shape(ann(_Kind, pmeta(Name)), Text, [], Totals, State0, State) :-
    meta_capture(Name, Totals, State0, Capture, State),
    format(atom(Text), '(_) @~w', [Capture]).
emit_shape(ann(Kind, pident(Name)), Text, [Predicate], _Totals, State0, State) :-
    lit_capture(State0, Capture, State),
    format(atom(Text), '(~w) @~w', [Kind, Capture]),
    format(atom(Predicate), '(#eq? @~w "~w")', [Capture, Name]).
emit_shape(ann(number, pnumber(Value)), Text, [Predicate], _Totals, State0, State) :-
    lit_capture(State0, Capture, State),
    format(atom(Text), '(number) @~w', [Capture]),
    format(atom(Predicate), '(#eq? @~w "~w")', [Capture, Value]).
emit_shape(ann(string, pstring(Value)), Text, [Predicate], _Totals, State0, State) :-
    lit_capture(State0, Capture, State),
    format(atom(Text), '(string) @~w', [Capture]),
    format(atom(Predicate), '(#eq? @~w "\\"~w\\"")', [Capture, Value]).
emit_shape(ann(call_expression, pcall(Callee, Arguments)), Text, Predicates,
           Totals, State0, State) :-
    emit_shape(Callee, CalleeText, CalleePredicates, Totals, State0, State1),
    emit_list(Arguments, ArgumentTexts, ArgumentPredicates, Totals, State1, State),
    args_query_text(Arguments, ArgumentTexts, ArgumentsPart),
    format(atom(Text), '(call_expression function: ~w arguments: ~w)',
           [CalleeText, ArgumentsPart]),
    append(CalleePredicates, ArgumentPredicates, Predicates).
emit_shape(ann(member_expression, pmember(Object, Property)), Text, Predicates,
           Totals, State0, State) :-
    emit_shape(Object, ObjectText, ObjectPredicates, Totals, State0, State1),
    emit_shape(Property, PropertyText, PropertyPredicates, Totals, State1, State),
    format(atom(Text), '(member_expression object: ~w property: ~w)',
           [ObjectText, PropertyText]),
    append(ObjectPredicates, PropertyPredicates, Predicates).

emit_list([], [], [], _Totals, State, State).
emit_list([ann(any_list, pellipsis(anon)) | Rest], Texts, Predicates,
          Totals, State0, State) :-
    !,
    emit_list(Rest, Texts, Predicates, Totals, State0, State).
emit_list([Item | Rest], [Text | Texts], Predicates, Totals, State0, State) :-
    emit_shape(Item, Text, ItemPredicates, Totals, State0, State1),
    emit_list(Rest, Texts, RestPredicates, Totals, State1, State),
    append(ItemPredicates, RestPredicates, Predicates).

% Interleave `.` anchors so the emitted query means the same child count the
% reference matcher means. Prev tracks what the previous item was: an
% ellipsis relaxes the neighbouring anchor, anything else demands one.
args_query_text(Arguments, ArgumentTexts, ArgumentsPart) :-
    args_tokens(Arguments, ArgumentTexts, start, Tokens),
    (   Tokens == []
    ->  ArgumentsPart = '(arguments)'
    ;   atomic_list_concat(Tokens, ' ', Joined),
        format(atom(ArgumentsPart), '(arguments ~w)', [Joined])
    ).

args_tokens([], _Texts, Previous, Tokens) :-
    ( Previous == node -> Tokens = ['.'] ; Tokens = [] ).
args_tokens([ann(any_list, pellipsis(_)) | Rest], Texts, _Previous, Tokens) :-
    !,
    args_tokens(Rest, Texts, ellipsis, Tokens).
args_tokens([_Argument | Rest], [Text | Texts], Previous, Tokens) :-
    (   Previous == ellipsis
    ->  Tokens = [Text | TokensRest]
    ;   Tokens = ['.', Text | TokensRest]
    ),
    args_tokens(Rest, Texts, node, TokensRest).

has_empty_child_list(ann(_, pcall(_Callee, []))) :- !.
has_empty_child_list(ann(_, pcall(Callee, Arguments))) :-
    !,
    ( has_empty_child_list(Callee)
    ; member(Argument, Arguments), has_empty_child_list(Argument) ), !.
has_empty_child_list(ann(_, pmember(Object, Property))) :-
    !,
    ( has_empty_child_list(Object) ; has_empty_child_list(Property) ), !.

lit_capture(state(LitCount0, MetaUsed), Capture, state(LitCount, MetaUsed)) :-
    LitCount is LitCount0 + 1,
    format(atom(Capture), 'lit~w', [LitCount]).

meta_capture(Name, Totals, state(LitCount, MetaUsed0), Capture,
             state(LitCount, MetaUsed)) :-
    memberchk(Name-Total, Totals),
    (   Total =:= 1
    ->  Capture = Name, MetaUsed = MetaUsed0
    ;   (   selectchk(Name-Used, MetaUsed0, MetaUsed1)
        ->  Index is Used + 1
        ;   Index = 1, MetaUsed1 = MetaUsed0
        ),
        format(atom(Capture), '~w_~w', [Name, Index]),
        MetaUsed = [Name-Index | MetaUsed1]
    ).

nonlinear_predicates(Totals, Predicates) :-
    findall(Predicate,
            ( member(Name-Total, Totals), Total > 1,
              between(1, Total, Index), Next is Index + 1, Next =< Total,
              format(atom(Predicate), '(#eq? @~w_~w @~w_~w)',
                     [Name, Index, Name, Next]) ),
            Predicates).

meta_names(ann(_, pmeta(Name)), [Name]) :- !.
meta_names(ann(_, pcall(Callee, Arguments)), Names) :- !,
    meta_names(Callee, CalleeNames),
    maplist(meta_names, Arguments, ArgumentNames),
    append([CalleeNames | ArgumentNames], Names).
meta_names(ann(_, pmember(Object, Property)), Names) :- !,
    meta_names(Object, ObjectNames), meta_names(Property, PropertyNames),
    append(ObjectNames, PropertyNames, Names).
meta_names(_, []).

ellipsis_names(ann(_, pellipsis(Name)), [Name]) :- !.
ellipsis_names(ann(_, pcall(Callee, Arguments)), Names) :- !,
    ellipsis_names(Callee, CalleeNames),
    maplist(ellipsis_names, Arguments, ArgumentNames),
    append([CalleeNames | ArgumentNames], Names).
ellipsis_names(ann(_, pmember(Object, Property)), Names) :- !,
    ellipsis_names(Object, ObjectNames), ellipsis_names(Property, PropertyNames),
    append(ObjectNames, PropertyNames, Names).
ellipsis_names(_, []).

% ── the pipeline, both exits from one term ─────────────────────────────────

pattern_ok(PatternText, Annotated) :-
    parse_pattern(PatternText, Pattern),
    check_top(Pattern, ok(Annotated)).

pattern_refused(PatternText, Reason) :-
    parse_pattern(PatternText, Pattern),
    check_top(Pattern, bad(Reason)).

% ── derived patterns: a codemod is term surgery, then a re-check ───────────

rename_callee(pcall(pident(_Old), Arguments), NewName,
              pcall(pident(NewName), Arguments)).

% A derivation that puts a literal in callee position is legal term surgery
% and an illegal pattern; the grammar check is what catches it.
literalize_callee(pcall(pident(Old), Arguments), pcall(pstring(Old), Arguments)).

% ═════════════════════════════════════════════════════════════════════════════
% GRADER
% ═════════════════════════════════════════════════════════════════════════════

% ── pass 1: grammar import ────────────────────────────────────────────────
check(grammar_kinds, ( findall(Kind, node_kind(Kind), Kinds),
                       msort(Kinds, Sorted),
                       Sorted == [arguments, call_expression, expression_statement,
                                  formal_parameters, function_declaration, identifier,
                                  member_expression, number, program,
                                  property_identifier, statement_block, string] )).
check(grammar_drops_anonymous,
                     ( \+ node_kind('('), \+ node_kind(function) )).
check(grammar_fields, ( node_field(call_expression, function, CalleeKinds),
                        CalleeKinds == [identifier, member_expression],
                        node_field(call_expression, arguments, [arguments]),
                        node_field_multiple(call_expression, function, false),
                        node_field(function_declaration, body, [statement_block]) )).
check(grammar_children, ( node_children(arguments, ArgumentKinds),
                          msort(ArgumentKinds, Sorted),
                          Sorted == [call_expression, identifier, member_expression,
                                     number, string],
                          node_children_multiple(arguments, true) )).
check(grammar_closed, ( grammar_dangling_kinds([]) )).

% ── pass 2: pattern syntax ────────────────────────────────────────────────
check(parse_call, ( parse_pattern("foo($A)", Pattern),
                    Pattern == pcall(pident(foo), [pmeta('A')]) )).
check(parse_member_ellipsis,
                  ( parse_pattern("$OBJ.method($$$)", Pattern),
                    Pattern == pcall(pmember(pmeta('OBJ'), pident(method)),
                                     [pellipsis(anon)]) )).
check(parse_named_ellipsis,
                  ( parse_pattern("foo($A, $$$REST)", Pattern),
                    Pattern == pcall(pident(foo),
                                     [pmeta('A'), pellipsis('REST')]) )).
check(parse_literals,
                  ( parse_pattern("obj.method(1, \"two\")", Pattern),
                    Pattern == pcall(pmember(pident(obj), pident(method)),
                                     [pnumber(1), pstring(two)]) )).
check(parse_nonlinear,
                  ( parse_pattern("log($SAME, $SAME)", Pattern),
                    Pattern == pcall(pident(log), [pmeta('SAME'), pmeta('SAME')]) )).
% quasiquotation reads the same term at COMPILE time, no runtime parse call.
check(quasiquote_same_term,
                  ( Quoted = {|sg||foo($A, $$$REST)|},
                    parse_pattern("foo($A, $$$REST)", Parsed),
                    Quoted == Parsed )).

% ── pass 3: check against the grammar ─────────────────────────────────────
check(check_accepts_call,
                  ( pattern_ok("foo($A)", Annotated),
                    Annotated == ann(call_expression,
                                     pcall(ann(identifier, pident(foo)),
                                           [ann(any, pmeta('A'))])) )).
% the property slot declares property_identifier, so the SAME pattern syntax
% resolves to a different node kind than the callee slot did.
check(check_resolves_property_kind,
                  ( pattern_ok("$OBJ.method($$$)", Annotated),
                    Annotated = ann(call_expression, pcall(Callee, [Ellipsis])),
                    Callee == ann(member_expression,
                                  pmember(ann(any, pmeta('OBJ')),
                                          ann(property_identifier, pident(method)))),
                    Ellipsis == ann(any_list, pellipsis(anon)) )).
check(check_accepts_named_ellipsis,
                  ( pattern_ok("foo($A, $$$REST)", _) )).
% BROKEN 1: a string can never sit in the function slot of a call.
check(check_refuses_string_callee,
                  ( pattern_refused("\"hello\"($A)", Reason),
                    Reason == kind_not_allowed(string, [identifier, member_expression]) )).
% BROKEN 2: $$$ in a slot the grammar declares single-valued.
check(check_refuses_ellipsis_in_field,
                  ( pattern_refused("$$$.method()", Reason),
                    Reason == ellipsis_outside_list(anon) )).
% BROKEN 3: a bare name where no allowed kind is a name leaf.
check(check_refuses_name_in_args_slot,
                  ( check_pattern(pident(oops), [statement_block], bad(Reason)),
                    Reason == no_name_kind_here(oops, [statement_block]) )).

% ── the CST ───────────────────────────────────────────────────────────────
check(cst_normalizes, ( cst(node(program, [], Statements)),
                        length(Statements, 5) )).
check(cst_leaf_text,  ( cst(Root),
                        subnode(Root, node(property_identifier, Fields, [])),
                        memberchk(text-Text, Fields), Text == "method" )).
% spans are gone by design: two occurrences of `x` are the SAME term.
check(cst_span_free,  ( cst(Root),
                        findall(Node, ( subnode(Root, Node),
                                        Node = node(identifier, Fields, _),
                                        memberchk(text-"x", Fields) ), Nodes),
                        Nodes = [First, Second | _], First == Second )).

% ── lowering 1: reference matching ────────────────────────────────────────
check(match_all_call_sites,
                  ( pattern_ok("f($$$ARGS)", Annotated), cst(Root),
                    find_matches(Annotated, Root, AllBindings),
                    length(AllBindings, 2),
                    findall(Texts, ( member(Bindings, AllBindings),
                                     bound_list_texts(Bindings, 'ARGS', Texts) ),
                            ArgTexts),
                    ArgTexts == [["a"], ["b", "c"]] )).
check(match_binds_subtree,
                  ( pattern_ok("f($ONLY)", Annotated), cst(Root),
                    find_matches(Annotated, Root, [Bindings]),
                    bound_text(Bindings, 'ONLY', Text), Text == "a" )).
% non-linear: log(x, x) matches, log(x, y) does not.
check(match_nonlinear,
                  ( pattern_ok("log($SAME, $SAME)", Annotated), cst(Root),
                    find_matches(Annotated, Root, [Bindings]),
                    bound_text(Bindings, 'SAME', Text), Text == "x" )).
check(match_linear_finds_both,
                  ( pattern_ok("log($LEFT, $RIGHT)", Annotated), cst(Root),
                    find_matches(Annotated, Root, AllBindings),
                    length(AllBindings, 2),
                    findall(Right, ( member(Bindings, AllBindings),
                                     bound_text(Bindings, 'RIGHT', Right) ), Rights),
                    Rights == ["x", "y"] )).
check(match_member_call,
                  ( pattern_ok("$OBJ.method($$$)", Annotated), cst(Root),
                    find_matches(Annotated, Root, [Bindings]),
                    bound_text(Bindings, 'OBJ', Text), Text == "obj" )).
check(match_ellipsis_tail,
                  ( pattern_ok("$OBJ.method($FIRST, $$$REST)", Annotated), cst(Root),
                    find_matches(Annotated, Root, [Bindings]),
                    bound_text(Bindings, 'FIRST', First), First == "1",
                    bound_list_texts(Bindings, 'REST', Rest), Rest == ["\"two\""] )).
check(match_literals,
                  ( pattern_ok("obj.method(1, \"two\")", Annotated), cst(Root),
                    find_matches(Annotated, Root, [[]]) )).
check(match_rejects_wrong_callee,
                  ( pattern_ok("nosuchfn($A)", Annotated), cst(Root),
                    find_matches(Annotated, Root, []) )).

% ── lowering 2: tree-sitter query emission ────────────────────────────────
check(emit_call,  ( pattern_ok("foo($A)", Annotated),
                    emit_query(Annotated, query(Query)),
                    Query == '((call_expression function: (identifier) @lit1 arguments: (arguments . (_) @A .)) (#eq? @lit1 "foo"))' )).
check(emit_member_ellipsis,
                  ( pattern_ok("$OBJ.method($$$)", Annotated),
                    emit_query(Annotated, query(Query)),
                    Query == '((call_expression function: (member_expression object: (_) @OBJ property: (property_identifier) @lit1) arguments: (arguments)) (#eq? @lit1 "method"))' )).
check(emit_nonlinear,
                  ( pattern_ok("log($SAME, $SAME)", Annotated),
                    emit_query(Annotated, query(Query)),
                    Query == '((call_expression function: (identifier) @lit1 arguments: (arguments . (_) @SAME_1 . (_) @SAME_2 .)) (#eq? @lit1 "log") (#eq? @SAME_1 @SAME_2))' )).
check(emit_literals,
                  ( pattern_ok("obj.method(1, \"two\")", Annotated),
                    emit_query(Annotated, query(Query)),
                    Query == '((call_expression function: (member_expression object: (identifier) @lit1 property: (property_identifier) @lit2) arguments: (arguments . (number) @lit3 . (string) @lit4 .)) (#eq? @lit1 "obj") (#eq? @lit2 "method") (#eq? @lit3 "1") (#eq? @lit4 "\\"two\\""))' )).
% the two expressiveness gaps between the lowerings, named at emit time
% instead of silently emitting a query that means something else.
check(emit_blocks_named_ellipsis,
                  ( pattern_ok("foo($A, $$$REST)", Annotated),
                    emit_query(Annotated, blocked(Reason)),
                    Reason == named_ellipsis('REST') )).
check(emit_blocks_empty_arg_list,
                  ( pattern_ok("foo()", Annotated),
                    emit_query(Annotated, blocked(Reason)),
                    Reason == empty_child_list_inexpressible )).

% ── patterns as terms: derived patterns (codemod route) ───────────────────
check(derived_pattern_lowers_both_ways,
                  ( parse_pattern("f($ONLY)", Source),
                    rename_callee(Source, g, Derived),
                    check_top(Derived, ok(Annotated)),
                    cst(Root), find_matches(Annotated, Root, []),
                    emit_query(Annotated, query(Query)),
                    Query == '((call_expression function: (identifier) @lit1 arguments: (arguments . (_) @ONLY .)) (#eq? @lit1 "g"))' )).
check(derived_pattern_recheck_catches_bad_surgery,
                  ( parse_pattern("f($ONLY)", Source),
                    literalize_callee(Source, Derived),
                    check_top(Derived, bad(Reason)),
                    Reason == kind_not_allowed(string, [identifier, member_expression]) )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
