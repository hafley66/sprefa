#!/usr/bin/env swipl
:- encoding(utf8).
:- use_module(library(http/json)).
:- use_module(library(lists)).
:- use_module(library(pairs)).
:- prolog_load_context(directory, EmitterDirectory),
   asserta(emitter_directory(EmitterDirectory)).
:- initialization(main, main).

% The four code_type/2 classes used by parse_dl_dcg.pl.  This table is the
% only handwritten character-class bridge in the emitter.
character_class(space, '\\s').
character_class(alnum, 'A-Za-z0-9').
character_class(alpha, 'A-Za-z').
character_class(digit, '0-9').

main(Argv) :-
    ( append(_, [Input0, Output0], Argv)
    -> absolute_file_name(Input0, Input, [access(read)]),
       absolute_file_name(Output0, Output, [access(write), file_errors(fail)])
    ; format(user_error, 'usage: emit_grammar.pl INPUT.pl OUTPUT.js~n', []),
      halt(2)
    ),
    file_directory_name(Input, CompileDir),
    directory_file_path(CompileDir, 'registry.pl', Registry),
    emitter_directory(LabDir),
    directory_file_path(LabDir, 'grammar.js', Overlay),
    read_source_terms(Input, Terms),
    load_files(Registry, [if(changed), silent(true)]),
    operator_inventory(Operators),
    specialization_inventory(Terms, Specializations, UnboundArgs),
    read_file_to_string(Overlay, HandGrammar, []),
    generated_replacements(Operators, Replacements),
    foldl(replace_checked, Replacements, HandGrammar, Rewritten),
    specialization_rules(Specializations, SpecializedRules),
    format(string(Replacement), '~s  },\n});\n', [SpecializedRules]),
    replace_checked('  },\n});\n'-Replacement,
                    Rewritten,
                    Rewritten2),
    generated_bodies(Terms, Bodies),
    rewrite_rule_bodies(Bodies, Rewritten2, FinalGrammar),
    pairs_keys(Bodies, GeneratedNames),
    setup_call_cleanup(
        open(Output, write, Stream, [encoding(utf8)]),
        emit_file(Stream, Input, Registry, Overlay, Operators,
                  Specializations, UnboundArgs, GeneratedNames, FinalGrammar),
        close(Stream)),
    length(Specializations, SpecializedCount),
    length(UnboundArgs, UnboundCount),
    length(Bodies, BodyCount),
    format('DCG_EMIT_V3 specializations=~d unbound_args=~d rule_bodies=~d output=~w~n',
           [SpecializedCount, UnboundCount, BodyCount, Output]).

read_source_terms(Path, Terms) :-
    setup_call_cleanup(
        open(Path, read, Stream, [encoding(utf8)]),
        read_terms(Stream, Terms),
        close(Stream)).

read_terms(Stream, Terms) :-
    read_term(Stream, Term, [module(user), syntax_errors(error)]),
    ( Term == end_of_file
    -> Terms = []
    ; apply_reader_directive(Term),
      Terms = [Term | Rest],
      read_terms(Stream, Rest)
    ).

apply_reader_directive((:- op(Priority, Type, Name))) :- !,
    op(Priority, Type, Name).
apply_reader_directive((:- set_prolog_flag(back_quotes, Value))) :- !,
    set_prolog_flag(back_quotes, Value).
apply_reader_directive(_).

operator_inventory(operators(Bind, Compare, Arithmetic)) :-
    findall(Op, registry:surface(Op/2, bind, no_refs, infix(_), _), Bind0),
    longest_first(Bind0, Bind),
    findall(Op, registry:surface(Op/2, guard, no_refs, infix(_), _), Compare0),
    longest_first(Compare0, Compare),
    findall(P, registry:expression(_/2, arithmetic, P, _, _), Precs0),
    sort(Precs0, Precs),
    findall(P-Ops,
            ( member(P, Precs),
              findall(Op, registry:expression(Op/2, arithmetic, P, _, _), Ops0),
              longest_first(Ops0, Ops)
            ),
            Arithmetic).

longest_first(Atoms, Sorted) :-
    map_list_to_pairs(neg_length, Atoms, Keyed),
    keysort(Keyed, Ordered),
    pairs_values(Ordered, Sorted).

neg_length(Atom, NegLen) :- atom_length(Atom, Len), NegLen is -Len.

specialization_inventory(Terms, Specializations, UnboundArgs) :-
    findall(Kind-Parser,
            ( member(Term, Terms),
              sub_term(Call, Term),
              specialization_call(Call, Kind, Parser),
              nonvar(Parser)
            ),
            Concrete0),
    findall(sep-Parser, member(args-Parser, Concrete0), DerivedSeps),
    append(Concrete0, DerivedSeps, WithDerivedSeps),
    sort(WithDerivedSeps, Specializations),
    findall(Kind-Parser,
            ( member(Term, Terms),
              sub_term(Call, Term),
              specialization_call(Call, Kind, Parser),
              var(Parser)
            ),
            Unbound0),
    maplist(canonical_pair, Unbound0, CanonicalUnbound),
    sort(CanonicalUnbound, UnboundArgs).

canonical_pair(Pair, Canonical) :-
    copy_term(Pair, Canonical),
    numbervars(Canonical, 0, _).

specialization_call(Call, sep, Parser) :-
    nonvar(Call),
    Call = sep(Parser, _).
specialization_call(Call, args, Parser) :-
    nonvar(Call),
    Call = args(Parser, _).

generated_replacements(operators(Bind, Compare, Arithmetic), Replacements) :-
    js_choice(Bind, BindChoice),
    js_choice(Compare, CompareChoice),
    memberchk(1-Add, Arithmetic),
    memberchk(2-Multiply, Arithmetic),
    js_choice(Add, AddChoice),
    js_choice(Multiply, MultiplyChoice),
    character_class(digit, Digit),
    character_class(alpha, Alpha),
    character_class(alnum, Alnum),
    format(string(IntegerRegex), '/[~w]+/', [Digit]),
    format(string(FloatRegex), '/[~w]+\\.[~w]+([eE][+-]?[~w]+)?/',
           [Digit, Digit, Digit]),
    format(string(IdentifierRegex), '/_*[a-z][~w_]*/', [Alnum]),
    format(string(VariableRegex),
           'choice(/[A-Z][~w_]*/, /_+[A-Z][~w_]*/, "_")',
           [Alnum, Alnum]),
    Arithmetic = [LoosePrec-_, TightPrec-_],
    AddPrec is LoosePrec + 2,
    MultiplyPrec is TightPrec + 2,
    UnaryPrec is MultiplyPrec + 1,
    CallPrec is UnaryPrec + 1,
    PrecNeedle = 'const PREC = {\n  bind: 1,\n  compare: 2,\n  add: 3,\n  multiply: 4,\n  unary: 5,\n  call: 6,\n};',
    format(string(PrecBlock),
           'const PREC = {\n  bind: 1,\n  compare: 2,\n  add: ~d,\n  multiply: ~d,\n  unary: ~d,\n  call: ~d,\n};',
           [AddPrec, MultiplyPrec, UnaryPrec, CallPrec]),
    Replacements = [
        PrecNeedle-PrecBlock,
        'choice(":=", "is")'-BindChoice,
        'choice("==", "\\\\==", ">", "<", ">=", "=<", "=:=", "=\\\\=")'-CompareChoice,
        'choice("+", "-")'-AddChoice,
        'choice("*", "/", "mod")'-MultiplyChoice,
        '/[0-9]+/'-IntegerRegex,
        '/[0-9]+\\.[0-9]+([eE][+-]?[0-9]+)?/'-FloatRegex,
        'choice(/[A-Z][A-Za-z0-9_]*/, /_+[A-Z][A-Za-z0-9_]*/, "_")'-VariableRegex,
        '/_*[a-z][A-Za-z0-9_]*/'-IdentifierRegex
    ],
    character_class(space, _),
    character_class(alpha, Alpha).

js_choice(Atoms, Js) :-
    maplist(json_atom, Atoms, Json),
    atomic_list_concat(Json, ', ', Joined),
    format(string(Js), 'choice(~w)', [Joined]).

json_atom(Atom, Json) :-
    atom_string(Atom, String),
    with_output_to(string(Json), json_write(current_output, String)).

replace_checked(Needle-Replacement, Input, Output) :-
    ( sub_string(Input, Before, _, After, Needle)
    -> sub_string(Input, 0, Before, _, Prefix),
       sub_string(Input, _, After, 0, Suffix),
       string_concat(Prefix, Replacement, Half),
       string_concat(Half, Suffix, Output)
    ; format(user_error, 'required overlay fragment absent: ~s~n', [Needle]),
      halt(3)
    ).

specialization_rules(Specializations, Rules) :-
    maplist(specialization_rule, Specializations, RuleStrings),
    atomics_to_string(RuleStrings, '', Rules).

specialization_rule(Kind-Parser, Rule) :-
    specialization_name(Kind, Parser, Name),
    parser_rule(Parser, Target),
    ( Kind == sep
    -> format(string(Body), 'seq(~w, repeat(seq(",", ~w)))', [Target, Target])
    ; specialization_name(sep, Parser, SepName),
      format(string(Body), 'optional($.~w)', [SepName])
    ),
    format(string(Rule), '    ~w: $ => ~w,\n', [Name, Body]).

specialization_name(Kind, Parser, Name) :-
    copy_term(Parser, StableParser),
    numbervars(StableParser, 0, _),
    term_to_atom(StableParser, ParserAtom),
    atom_codes(ParserAtom, Codes),
    maplist(identifier_code, Codes, SafeCodes),
    atom_codes(Safe, SafeCodes),
    format(atom(Name), '~w_~w', [Kind, Safe]).

identifier_code(Code, Code) :- code_type(Code, alnum), !.
identifier_code(0'_, 0'_) :- !.
identifier_code(_, 0'_).

parser_rule(typed_col(_), '$.column') :- !.
parser_rule(atom_arg, '$.expression') :- !.
parser_rule(decl_a_column, '$.declaration_parameter') :- !.
parser_rule(enum_field, '$.column') :- !.
parser_rule(expr, '$.expression') :- !.
parser_rule(int_lit, '$.integer') :- !.
parser_rule(rel_atom_term, '$.atom') :- !.
parser_rule(Parser, Target) :-
    callable(Parser),
    functor(Parser, Name, _),
    format(atom(Target), '$.~w', [Name]).

% Clause-shape reader: every predicate below answers a question about
% parse_dl_dcg.pl's clause bodies and carries no Tree-sitter answer.

dcg_clause(Terms, Name/Arity, Body) :-
    member(Clause, Terms),
    nonvar(Clause),
    Clause = (Head --> Body),
    nonvar(Head),
    functor(Head, Name, Arity).

plain_clause(Terms, Head, Body) :-
    member(Clause, Terms),
    nonvar(Clause),
    Clause \= (:- _),
    Clause \= (_ --> _),
    ( Clause = (Head :- Body) -> true ; Head = Clause, Body = true ).

conjunction_goals(Body, Goals) :-
    ( nonvar(Body), Body = (Left, Right)
    -> conjunction_goals(Left, LeftGoals),
       conjunction_goals(Right, RightGoals),
       append(LeftGoals, RightGoals, Goals)
    ; Goals = [Body]
    ).

% {}/1, cuts and the empty body carry no input position
noise_goal(Goal) :- nonvar(Goal), ( Goal = {_} ; Goal == ! ; Goal == [] ).

goal_sequence(Body, Sequence) :-
    conjunction_goals(Body, Goals),
    exclude(noise_goal, Goals, Sequence).

% every sequence reachable through if-then-else branches, outermost first
nested_sequence(Body, Sequence) :- goal_sequence(Body, Sequence).
nested_sequence(Body, Sequence) :-
    conjunction_goals(Body, Goals),
    member(Goal, Goals),
    nonvar(Goal),
    branch_of(Goal, Branch),
    nested_sequence(Branch, Sequence).

branch_of((Condition -> Then ; Else), Branch) :- !,
    member(Branch, [(Condition, Then), Else]).
branch_of((Condition -> Then), Branch) :- !,
    Branch = (Condition, Then).
branch_of((Left ; Right), Branch) :- !,
    member(Branch, [Left, Right]).

% ---------------------------------------------------------------- terminals

% the sigils are prefix operators inside parse_dl_dcg.pl only, so this file
% takes them apart with =.. rather than spelling them
terminal_sigil('@').
terminal_sigil('~').
terminal_sigil('#').

literal_codes(Goal, Codes) :-
    nonvar(Goal),
    Goal =.. [Sigil, Codes],
    terminal_sigil(Sigil),
    is_list(Codes).
literal_codes(Goal, Codes) :-
    nonvar(Goal),
    Goal = kw(Word),
    atom(Word),
    atom_codes(Word, Codes).
literal_codes(Goal, Codes) :-
    is_list(Goal),
    Goal \== [],
    forall(member(C, Goal), integer(C)),
    Codes = Goal.

% a zero-argument nonterminal defined as Name([Code | S], S) is a one-char
% terminal spelled as a predicate
literal_text(_, Goal, Text) :-
    literal_codes(Goal, Codes), !,
    string_codes(Text, Codes).
literal_text(Terms, Goal, Text) :-
    atom(Goal),
    plain_clause(Terms, Head, _),
    nonvar(Head),
    Head =.. [Goal, Front, Rest],
    nonvar(Front),
    Front = [Code | Tail],
    integer(Code),
    Tail == Rest,
    !,
    string_codes(Text, [Code]).

% ident//1 and id_code/1 name their classes with code_type/2 and literal
% codes; the class strings come from character_class/2 and nothing else.

class_pieces(Body, Pieces) :-
    findall(Piece,
            ( sub_term(code_type(_, Kind), Body),
              nonvar(Kind),
              character_class(Kind, Piece)
            ),
            ClassPieces),
    findall(Piece,
            ( sub_term(Sub, Body),
              nonvar(Sub),
              literal_code_of(Sub, Code),
              char_code(Char, Code),
              regex_class_char(Char, Piece)
            ),
            LiteralPieces),
    append(ClassPieces, LiteralPieces, All),
    list_to_set(All, Pieces).

literal_code_of(_ == Code, Code) :- integer(Code).

regex_class_char(Char, Escaped) :-
    ( memberchk(Char, ['\\', ']', '^', '-'])
    -> atom_concat('\\', Char, Escaped)
    ; Escaped = Char
    ).

ident_start_class(Terms, Class) :-
    once(( plain_clause(Terms, Head, Body),
           nonvar(Head),
           Head = ident(_, _, _) )),
    class_pieces(Body, Pieces),
    atomic_list_concat(Pieces, Class).

ident_rest_class(Terms, Class) :-
    findall(Pieces,
            ( plain_clause(Terms, Head, Body),
              nonvar(Head),
              Head = id_code(Arg),
              class_pieces((Head, Body), Pieces0),
              ( integer(Arg)
              -> char_code(Char, Arg), regex_class_char(Char, Piece),
                 Pieces = [Piece]
              ; Pieces = Pieces0
              )
            ),
            Nested),
    append(Nested, Flat),
    list_to_set(Flat, Set),
    partition(class_range_piece, Set, Ranges, Singles),
    append(Ranges, Singles, Ordered),
    atomic_list_concat(Ordered, Class).

class_range_piece(Piece) :- sub_atom(Piece, _, 1, _, '-').

ident_regex(Terms, Regex) :-
    ident_start_class(Terms, Start),
    ident_rest_class(Terms, Rest),
    format(string(Regex), '[~w][~w]*', [Start, Rest]).

regex_escape(Text, Escaped) :-
    string_chars(Text, Chars),
    maplist(regex_escape_char, Chars, Parts),
    atomic_list_concat(Parts, Escaped).

regex_escape_char(Char, Escaped) :-
    ( memberchk(Char, ['\\', '.', '$', '^', '*', '+', '?', '(', ')',
                       '[', ']', '{', '}', '|', '/'])
    -> atom_concat('\\', Char, Escaped)
    ; Escaped = Char
    ).

% A terminal immediately followed by ident//1, no ws//0 between, is one token.
% token.immediate applies when the call site rewinds past ws//0 with back//1.

adjacent_goals(Sequence, First, Second) :-
    append(_, [First, Second | _], Sequence),
    nonvar(First),
    nonvar(Second).

ident_suffix_token(Terms, Nonterminal, Regex) :-
    dcg_clause(Terms, Nonterminal, Body),
    nested_sequence(Body, Sequence),
    adjacent_goals(Sequence, First, Second),
    Second = ident(_),
    literal_text(Terms, First, Text),
    regex_escape(Text, Escaped),
    ident_regex(Terms, IdentRegex),
    format(string(Regex), '~w~w', [Escaped, IdentRegex]).

entered_after_rewind(Terms, Name/Arity) :-
    dcg_clause(Terms, _, Body),
    nested_sequence(Body, Sequence),
    adjacent_goals(Sequence, Previous, Call),
    Previous = back(_),
    functor(Call, Name, Arity),
    !.

token_body(Terms, Nonterminal, Js) :-
    once(ident_suffix_token(Terms, Nonterminal, Regex)),
    ( entered_after_rewind(Terms, Nonterminal)
    -> format(string(Js), 'token.immediate(/~w/)', [Regex])
    ; format(string(Js), 'token(/~w/)', [Regex])
    ).

% Clause pair: Rule --> Item, Sep, Rule beside Rule --> Item.  Single clause:
% Rule --> Item, ( Sep -> Rule ; {} ).  Both mean the same repetition.

repetition(Terms, Nonterminal, Item, Separator) :-
    findall(Body, dcg_clause(Terms, Nonterminal, Body), Bodies),
    repetition_shape(Terms, Nonterminal, Bodies, Item, Separator).

repetition_shape(Terms, Name/Arity, [Recursive, Base], Item, Separator) :-
    goal_sequence(Base, [Item]),
    goal_sequence(Recursive, RecursiveSequence),
    append(Prefix, [Tail], RecursiveSequence),
    nonvar(Tail),
    functor(Tail, Name, Arity),
    Prefix = [Leading | SeparatorGoals],
    same_shape(Leading, Item),
    separator_goal(Terms, SeparatorGoals, Separator).
repetition_shape(Terms, Name/Arity, [Only], Item, Separator) :-
    goal_sequence(Only, Sequence),
    append(Prefix, [Conditional], Sequence),
    nonvar(Conditional),
    Conditional = (Guard -> Then ; _),
    goal_sequence(Then, ThenSequence),
    append(_, [Tail], ThenSequence),
    nonvar(Tail),
    functor(Tail, Name, Arity),
    literal_text(Terms, Guard, Separator),
    exclude(==(ws), Prefix, [Item]).

same_shape(Left, Right) :-
    nonvar(Left), nonvar(Right),
    functor(Left, Name, Arity),
    functor(Right, Name, Arity).

separator_goal(Terms, Goals, Separator) :-
    exclude(==(ws), Goals, [Goal]),
    literal_text(Terms, Goal, Separator).

% ------------------------------------------------------------- js rendering

js_string(Text, Js) :- json_atom(Text, Js).

gen_sequence(Terms, Goals, Js) :-
    exclude(==(ws), Goals, Meaningful),
    maplist(gen_goal(Terms), Meaningful, Parts),
    ( Parts = [Single]
    -> Js = Single
    ; atomic_list_concat(Parts, ', ', Joined),
      format(string(Js), 'seq(~w)', [Joined])
    ).

gen_goal(Terms, Goal, Js) :-
    literal_text(Terms, Goal, Text), !,
    js_string(Text, Js).
gen_goal(Terms, Goal, Js) :-
    nonvar(Goal),
    Goal = (Guard -> Then ; Else),
    !,
    ( nonvar(Guard), Guard = peek(_), goal_sequence(Then, [])
    -> goal_sequence(Else, ElseSequence),
       gen_sequence(Terms, ElseSequence, ElseJs),
       format(string(Js), 'optional(~w)', [ElseJs])
    ; goal_sequence(Then, ThenSequence),
      goal_sequence(Else, ElseSequence),
      gen_sequence(Terms, ThenSequence, ThenJs),
      gen_sequence(Terms, ElseSequence, ElseJs),
      format(string(Js), 'choice(~w, ~w)', [ThenJs, ElseJs])
    ).
gen_goal(_, Goal, Js) :-
    nonvar(Goal),
    functor(Goal, Name, Arity),
    cst_reference(Name/Arity, Js), !.
gen_goal(Terms, Goal, Js) :-
    nonvar(Goal),
    functor(Goal, Name, Arity),
    gen_body(Terms, Name/Arity, Js).

gen_body(Terms, Nonterminal, Js) :-
    repetition(Terms, Nonterminal, Item, Separator), !,
    gen_goal(Terms, Item, ItemJs),
    js_string(Separator, SeparatorJs),
    format(string(Js), 'seq(~w, repeat(seq(~w, ~w)))',
           [ItemJs, SeparatorJs, ItemJs]).
gen_body(Terms, Nonterminal, Js) :-
    findall(Body, dcg_clause(Terms, Nonterminal, Body), [Body]),
    goal_sequence(Body, Sequence),
    gen_sequence(Terms, Sequence, Js).

% Editor node names for DCG nonterminals; every name here is already spelled
% in the hand grammar and the emitter coins none.

cst_reference(ident/1, '$.identifier').
cst_reference(enum_variant/1, '$.enum_variant').
cst_reference(brace_pair/1, '$.object_pair').

cst_rule_body(dotted_path/1, path).
cst_rule_body(enum_variants/1, enum_variants).
cst_rule_body(braces_term/1, object_pattern).

generated_bodies(Terms, Bodies) :-
    findall(Rule-Js,
            ( cst_rule_body(Nonterminal, Rule),
              gen_body(Terms, Nonterminal, Js)
            ),
            Structural),
    findall(Rule-Js,
            ( member(Nonterminal-Rule,
                     [brace_key/1-capture_key, dot_chain/2-member_access]),
              token_body(Terms, Nonterminal, Js)
            ),
            Tokens),
    append(Structural, Tokens, Pairs),
    keysort(Pairs, Bodies).

rewrite_rule_bodies(Bodies, GrammarIn, GrammarOut) :-
    split_string(GrammarIn, "\n", "", Lines),
    rewrite_lines(Lines, Bodies, Rewritten),
    atomic_list_concat(Rewritten, '\n', GrammarOut).

rewrite_lines([], _, []).
rewrite_lines([Line | Rest], Bodies, Out) :-
    ( rule_key_line(Line, Name),
      memberchk(Name-Js, Bodies)
    -> format(atom(New), '    ~w: $ => ~w,', [Name, Js]),
       skip_rule_body(Rest, Tail),
       Out = [New | More],
       rewrite_lines(Tail, Bodies, More)
    ; Out = [Line | More],
      rewrite_lines(Rest, Bodies, More)
    ).

skip_rule_body([], []).
skip_rule_body([Line | Rest], Tail) :-
    ( rule_key_line(Line, _) ; rules_end_line(Line) ),
    !,
    Tail = [Line | Rest].
skip_rule_body([_ | Rest], Tail) :- skip_rule_body(Rest, Tail).

rule_key_line(Line, Name) :-
    string_concat("    ", Body, Line),
    sub_string(Body, Before, _, _, ": $ =>"),
    !,
    sub_string(Body, 0, Before, _, NameText),
    string_chars(NameText, Chars),
    Chars = [_ | _],
    forall(member(Char, Chars), identifier_char(Char)),
    atom_string(Name, NameText).

identifier_char(Char) :- char_type(Char, alnum), !.
identifier_char('_').

rules_end_line(Line) :- string_concat("  },", _, Line).

% Header paths are repo-relative so the cmp gate holds in any worktree.
repo_relative(Path, Relative) :-
    emitter_directory(LabDir),
    absolute_file_name('../../..', Root, [relative_to(LabDir)]),
    atom_concat(Root, '/', Prefix),
    ( atom_concat(Prefix, Relative0, Path) -> Relative = Relative0
    ; Relative = Path
    ).

emit_file(Stream, Input, Registry, Overlay, Operators,
          Specializations, UnboundArgs, GeneratedNames, Grammar) :-
    maplist(repo_relative, [Input, Registry, Overlay], [RelIn, RelReg, RelOv]),
    format(Stream, '// Generated by emit_grammar.pl v3.\n', []),
    format(Stream, '// Inputs: ~w ; ~w ; ~w\n', [RelIn, RelReg, RelOv]),
    format(Stream, '// Registry inventory: ~q\n', [Operators]),
    format(Stream, '// Specialized calls: ~q\n', [Specializations]),
    format(Stream, '// Unbound args/sep calls: ~q\n', [UnboundArgs]),
    format(Stream, '// Generated rule bodies: ~q\n', [GeneratedNames]),
    format(Stream, '~s', [Grammar]).
