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
                    FinalGrammar),
    setup_call_cleanup(
        open(Output, write, Stream, [encoding(utf8)]),
        emit_file(Stream, Input, Registry, Overlay, Operators,
                  Specializations, UnboundArgs, FinalGrammar),
        close(Stream)),
    length(Specializations, SpecializedCount),
    length(UnboundArgs, UnboundCount),
    format('DCG_EMIT_V2 specializations=~d unbound_args=~d output=~w~n',
           [SpecializedCount, UnboundCount, Output]).

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

emit_file(Stream, Input, Registry, Overlay, Operators,
          Specializations, UnboundArgs, Grammar) :-
    format(Stream, '// Generated by emit_grammar.pl v2.\n', []),
    format(Stream, '// Inputs: ~w ; ~w ; ~w\n', [Input, Registry, Overlay]),
    format(Stream, '// Registry inventory: ~q\n', [Operators]),
    format(Stream, '// Specialized calls: ~q\n', [Specializations]),
    format(Stream, '// Unbound args/sep calls: ~q\n', [UnboundArgs]),
    format(Stream, '~s', [Grammar]).
