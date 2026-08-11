:- module(parse_dl_dcg,
          [ parse_dl_dcg_entry/5,
            parse_dl_file/4,
            parse_dl_source/5
          ]).

:- set_prolog_flag(back_quotes, codes).

:- use_module(registry, [expression/5, surface/5]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700, xfx, :=).

parse_dl_file(FilePath, Program, Bindings, Findings) :-
    read_file_to_codes(FilePath, Codes, []),
    parse_dl_source(FilePath, Codes, Program, Bindings, Findings).

parse_dl_dcg_entry(Source, Codes, Program, Bindings, Findings) :-
    parse_dl_source(Source, Codes, Program, Bindings, Findings).

parse_dl_source(_Source, Codes, _Program, _Bindings, _Findings) :-
    var(Codes),
    !,
    throw(dl_parse_error(invalid_input, position(1, 1))).
parse_dl_source(_Source, Codes, Program, Bindings, []) :-
    ( phrase(program(Program, [], FinalVariables), Codes)
    -> bindings(FinalVariables, Bindings)
    ; phrase(unmigrated_statement, Codes)
    ).

bindings(Variables, Bindings) :-
    reverse(Variables, Ordered),
    maplist(variable_binding, Ordered, Bindings).

variable_binding(Name-Variable, Name=Variable).

program(prog([], Rules), Variables0, Variables) -->
    ws,
    rule_statements(Rules, Variables0, Variables),
    ws,
    eos.

rule_statements([], Variables, Variables) -->
    eos,
    !.
rule_statements([Rule | Rules], Variables0, Variables) -->
    rule_statement(Rule, Variables0, Variables1),
    ws,
    rule_statements(Rules, Variables1, Variables).

rule_statement(Rule, Variables0, Variables) -->
    head_atom(Head, Variables0, Variables1),
    ws,
    rule_tail(Head, Rule, Variables1, Variables),
    ws,
    `.`.

rule_tail(Head, Head <- Body, Variables0, Variables) -->
    `<-`,
    !,
    ws,
    body(Body, Variables0, Variables).
rule_tail(Head, Head <+ Body, Variables0, Variables) -->
    `<+`,
    !,
    ws,
    body(Body, Variables0, Variables).
rule_tail(Head, Head <- true, Variables, Variables) -->
    [].

head_atom(Term, Variables0, Variables) -->
    identifier(Name),
    ws,
    `(`,
    arguments(Arguments, Variables0, Variables),
    ws,
    `)`,
    { Term =.. [Name | Arguments] }.

arguments([], Variables, Variables) -->
    ws,
    peek(0')),
    !.
arguments([Argument | Arguments], Variables0, Variables) -->
    ws,
    expression_term(Argument, Variables0, Variables1),
    ws,
    argument_tail(Arguments, Variables1, Variables).

argument_tail(Arguments, Variables0, Variables) -->
    `,`,
    !,
    arguments(Arguments, Variables0, Variables).
argument_tail([], Variables, Variables) -->
    [].

body((Item, Rest), Variables0, Variables) -->
    body_item(Item, Variables0, Variables1),
    ws,
    `,`,
    !,
    ws,
    body(Rest, Variables1, Variables).
body(Item, Variables0, Variables) -->
    body_item(Item, Variables0, Variables).

body_item(Item, Variables0, Variables) -->
    bind_item(Item, Variables0, Variables),
    !.
body_item(Item, Variables0, Variables) -->
    comparison_item(Item, Variables0, Variables),
    !.
body_item(true, Variables, Variables) -->
    keyword(`true`),
    !.
body_item(Item, Variables0, Variables) -->
    relation_atom(Item, Variables0, Variables).

relation_atom(Term, Variables0, Variables) -->
    identifier(Name),
    ws,
    `(`,
    arguments(Arguments, Variables0, Variables),
    ws,
    `)`,
    { Term =.. [Name | Arguments] }.

bind_item(Term, Variables0, Variables) -->
    expression_term(Left, Variables0, Variables1),
    ws,
    registered_operator(bind, Operator),
    ws,
    expression_term(Right, Variables1, Variables),
    { Term =.. [Operator, Left, Right] }.

comparison_item(Term, Variables0, Variables) -->
    expression_term(Left, Variables0, Variables1),
    ws,
    comparison_operator(Operator),
    ws,
    expression_term(Right, Variables1, Variables),
    { Term =.. [Operator, Left, Right] }.

comparison_operator(=<) --> `<=`, !.
comparison_operator(\==) --> `!=`, !.
comparison_operator(Operator) --> registered_operator(guard, Operator), !.
comparison_operator(==) --> `=`, !.

registered_operator(Axis, Operator, Input, Rest) :-
    findall(NegatedLength-(Codes-Candidate),
            ( surface(Candidate/2, Axis, no_refs, infix(_), _),
              atom_codes(Candidate, Codes),
              length(Codes, Length),
              NegatedLength is -Length
            ),
            Candidates),
    keysort(Candidates, Ordered),
    member(_-(Codes-Operator), Ordered),
    phrase(operator_codes(Codes), Input, Rest).

operator_codes(Codes) -->
    { Codes = [First | _] },
    ( { code_type(First, alpha) }
    -> keyword(Codes)
    ; Codes
    ).

expression_term(Expression, Variables0, Variables) -->
    { arithmetic_tiers(Tiers) },
    tier_expression(Tiers, Expression, Variables0, Variables).

arithmetic_tiers(Tiers) :-
    findall(Precedence, expression(_/2, arithmetic, Precedence, _, _), Values),
    sort(Values, Tiers).

tier_operators(Precedence, Operators) :-
    findall(NegatedLength-Operator,
            ( expression(Operator/2, arithmetic, Precedence, _, _),
              atom_length(Operator, Length),
              NegatedLength is -Length
            ),
            Candidates),
    keysort(Candidates, Ordered),
    findall(Operator, member(_-Operator, Ordered), Operators).

tier_expression([], Expression, Variables0, Variables) -->
    factor(Expression, Variables0, Variables).
tier_expression([Precedence | Tighter], Expression, Variables0, Variables) -->
    tier_expression(Tighter, First, Variables0, Variables1),
    { tier_operators(Precedence, Operators) },
    tier_expression_rest(Operators, Tighter, First, Expression,
                         Variables1, Variables).

tier_expression_rest(Operators, Tighter, Accumulator, Expression,
                     Variables0, Variables) -->
    ws,
    ( tier_operator(Operators, Operator)
    -> ws,
       tier_expression(Tighter, Right, Variables0, Variables1),
       { Next =.. [Operator, Accumulator, Right] },
       tier_expression_rest(Operators, Tighter, Next, Expression,
                            Variables1, Variables)
    ; { Expression = Accumulator, Variables = Variables0 }
    ).

tier_operator([Operator | Operators], Matched) -->
    { atom_codes(Operator, Codes) },
    ( operator_codes(Codes)
    -> { Matched = Operator }
    ; tier_operator(Operators, Matched)
    ).

factor(Expression, Variables0, Variables) -->
    ws,
    ( `(`
    -> ws,
       expression_term(Expression, Variables0, Variables),
       ws,
       `)`
    ; bool_literal(Expression)
    -> { Variables = Variables0 }
    ; float_literal(Expression)
    -> { Variables = Variables0 }
    ; integer_literal(Expression)
    -> { Variables = Variables0 }
    ; quoted_atom_literal(Expression)
    -> { Variables = Variables0 }
    ; string_literal(Expression)
    -> { Variables = Variables0 }
    ; compound_or_variable(Expression, Variables0, Variables)
    ).

compound_or_variable(Expression, Variables0, Variables) -->
    identifier(Name),
    ws,
    ( `(`
    -> arguments(Arguments, Variables0, Variables),
       ws,
       `)`,
       { Expression =.. [Name | Arguments] }
    ; { get_or_make_variable(Name, Variables0, Expression, Variables) }
    ).

get_or_make_variable(Name, Variables0, Variable, Variables) :-
    ( memberchk(Name-Existing, Variables0)
    -> Variable = Existing,
       Variables = Variables0
    ; Variables = [Name-Variable | Variables0]
    ).

bool_literal(bool_lit(true)) --> keyword(`true`), !.
bool_literal(bool_lit(false)) --> keyword(`false`).

integer_literal(Value) -->
    optional_minus(Sign),
    digits(Digits),
    { number_codes(Magnitude, Digits),
      ( Sign == negative -> Value is -Magnitude ; Value = Magnitude )
    }.

optional_minus(negative) --> `-`, !.
optional_minus(positive) --> [].

float_literal(Value, Input, Rest) :-
    phrase(float_codes(Codes), Input, Rest),
    number_codes(Value, Codes),
    float(Value),
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]).

float_codes(Codes) -->
    optional_minus_codes(Sign),
    digits(Integer),
    float_tail(Tail),
    { append([Sign, Integer, Tail], Codes) }.

optional_minus_codes([0'-]) --> `-`, !.
optional_minus_codes([]) --> [].

float_tail(Codes) -->
    `.`,
    digits(Fraction),
    exponent_codes(Exponent),
    { append([[0'.], Fraction, Exponent], Codes) }.
float_tail(Codes) --> exponent_codes_required(Codes).

exponent_codes(Codes) --> exponent_codes_required(Codes), !.
exponent_codes([]) --> [].

exponent_codes_required([Marker | Codes]) -->
    [Marker],
    { memberchk(Marker, [0'e, 0'E]) },
    exponent_sign(Sign),
    digits(Digits),
    { append(Sign, Digits, Codes) }.

exponent_sign([Sign]) --> [Sign], { memberchk(Sign, [0'+, 0'-]) }, !.
exponent_sign([]) --> [].

digits([Digit | Digits]) -->
    [Digit],
    { code_type(Digit, digit) },
    !,
    digits_rest(Digits).

digits_rest([Digit | Digits]) -->
    [Digit],
    { code_type(Digit, digit) },
    !,
    digits_rest(Digits).
digits_rest([]) --> [].

quoted_atom_literal(Atom) -->
    `'`,
    quoted_chars(0'\', Codes),
    { atom_codes(Atom, Codes) }.

string_literal(String) -->
    `"`,
    quoted_chars(0'", Codes),
    { string_codes(String, Codes) }.

quoted_chars(Quote, [Quote | Codes]) -->
    [Quote, Quote],
    !,
    quoted_chars(Quote, Codes).
quoted_chars(Quote, []) --> [Quote], !.
quoted_chars(Quote, Codes) -->
    `\\`,
    [Escaped],
    !,
    { escape_codes(Quote, Escaped, Codes, More) },
    quoted_chars(Quote, More).
quoted_chars(Quote, [Code | Codes]) -->
    [Code],
    quoted_chars(Quote, Codes).

escape_codes(_, 0'n, [0'\n | More], More) :- !.
escape_codes(_, 0't, [0'\t | More], More) :- !.
escape_codes(_, 0'r, [0'\r | More], More) :- !.
escape_codes(_, 0'\\, [0'\\ | More], More) :- !.
escape_codes(Quote, Quote, [Quote | More], More) :- !.
escape_codes(_, Other, [0'\\, Other | More], More).

identifier(Name) -->
    [First],
    { code_type(First, alpha) ; First == 0'_ },
    !,
    identifier_rest(Rest),
    { atom_codes(Name, [First | Rest]) }.

identifier_rest([Code | Codes]) -->
    [Code],
    { code_type(Code, alnum) ; Code == 0'_ },
    !,
    identifier_rest(Codes).
identifier_rest([]) --> [].

keyword(Codes) -->
    Codes,
    keyword_boundary.

keyword_boundary(Input, Input) :-
    \+ ( Input = [Code | _],
         ( code_type(Code, alnum) ; Code == 0'_ ) ).

ws --> [Code], { code_type(Code, space) }, !, ws.
ws --> `#`, !, comment, ws.
ws --> [].

comment --> [0'\n], !.
comment --> [_], !, comment.
comment --> [].

peek(Code, Input, Input) :- Input = [Code | _].
eos([], []).
