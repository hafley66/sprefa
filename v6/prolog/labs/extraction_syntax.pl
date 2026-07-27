% extraction_syntax.pl : the SPELLING lab for v6 extraction.
%
% Run:  swipl -q -l v6/prolog/labs/extraction_syntax.pl -g go -g halt
%
% AUDIT finding 17 is the largest open surface gap: 139 of 173 v5 files use
% extraction and the candidate surface has no syntax for it. The semantics are
% already ruled elsewhere (transform law: extraction rels are lazy world-fed
% rels keyed by (digest, pattern); input-digest salt; effects are adorned rels
% with the -> program/world split; FileSpan is the one location value). The
% machinery behind them (git crawling, ast extraction, watchers) is solved v5
% rust territory that enters v6 at link time. Nothing here re-derives any of
% that. This lab decides only HOW A PROGRAM SPELLS IT.
%
% What is graded:
%   1. the quoted-region lexer for pattern literals, with the adversarial law
%      (no single-character fence perturbation silently yields another legal
%      spelling) and the pairwise Hamming property of the language tag set;
%   2. capture-name extraction per pattern language (the names ARE the output
%      column names);
%   3. desugaring of each candidate spelling into the ruled kernel shapes
%      (a world-filled lazy rel + its key columns + its salt + the body goals),
%      checked by unification against expected kernel facts;
%   4. the FileSpan mint point, and why content-addressed dedup only works
%      because the kernel rel returns content-relative rows;
%   5. the three v5 transcriptions' construct census against the keep-list.
%
% Descriptive variable names throughout. The surface text of everything here
% lives in extraction_syntax.md.

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('../src/grader').

:- op(990, xfx, <-).
:- discontiguous check/2.

% ═══════════════════════════════════════════════════════════════════════════
% 1. THE CLOSED PATTERN-LANGUAGE SET
%
% A pattern literal is a quoted DSL region, the construct AGGREGATE already
% keeps: {|lang|| ... |} with an explicit closing delimiter, never brace
% balanced. Extraction adds NO new grammar; it adds language tags to a set the
% construct already has, and that set is CLOSED at link time.
%
% pattern_language(Tag, SubjectKind, CaptureRule, NeedsGrammar)
% ═══════════════════════════════════════════════════════════════════════════

pattern_language(re,       content, named_groups,  no).
pattern_language(sg,       content, metavars,      yes).
pattern_language(ts,       content, at_captures,   yes).
pattern_language(json,     content, dollar_names,  no).
pattern_language(jsonpath, content, fixed([value]), no).
pattern_language(glob,     tree,    none,          no).
pattern_language(path,     tree,    none,          no).

known_language(Tag) :- pattern_language(Tag, _, _, _).

% The grammar names a `sg`/`ts` region may carry. This set is CLOSED at link
% time by the grammar import (node-types.json -> facts), exactly like the tag
% set, and for the same reason: the adversarial check below FOUND the hole
% when the grammar name was an open identifier ({|sg:rust||  ->  {|sg:rest||
% lexes fine and means something else). Closing the set is what discharges the
% law, and closing it forces the rename off ast-grep's short tags, because
% `ts` and `js` are one character apart.
imported_grammar(rust).
imported_grammar(typescript).
imported_grammar(tsx).
imported_grammar(javascript).
imported_grammar(python).
imported_grammar(go).
imported_grammar(json).
imported_grammar(c).
imported_grammar(cpp).
imported_grammar(kotlin).

% ═══════════════════════════════════════════════════════════════════════════
% 2. THE REGION LEXER
%
% Concrete syntax:  {| Tag [: Grammar] || Body |}
% The body ends at the FIRST "|}". There is no escape; a body containing "|}"
% is refused by name (ambiguity 7 in the .md).
% ═══════════════════════════════════════════════════════════════════════════

lex_region(Text, Result) :-
    string_codes(Text, Codes),
    (   phrase(open_fence(TagCodes), Codes, AfterOpen)
    ->  (   split_tag(TagCodes, Language, Grammar)
        ->  (   known_language(Language)
            ->  (   grammar_verdict(Language, Grammar, ok)
                ->  (   split_close(AfterOpen, BodyCodes, AfterClose)
                    ->  (   AfterClose == []
                        ->  string_codes(Body, BodyCodes),
                            Result = ok(region(Language, Grammar, Body))
                        ;   Result = bad(text_after_close_fence)
                        )
                    ;   Result = bad(unterminated_region)
                    )
                ;   grammar_verdict(Language, Grammar, GrammarVerdict),
                    Result = bad(GrammarVerdict)
                )
            ;   Result = bad(unknown_language(Language))
            )
        ;   Result = bad(malformed_language_tag)
        )
    ;   Result = bad(malformed_open_fence)
    ).

open_fence(TagCodes) --> "{|", tag_upto_bars(TagCodes).

tag_upto_bars([]) --> "||", !.
tag_upto_bars([Code|Rest]) --> [Code], { tag_code(Code) }, tag_upto_bars(Rest).

tag_code(Code) :- code_type(Code, alnum), !.
tag_code(0'_).
tag_code(0':).
tag_code(0'-).

split_tag(TagCodes, Language, Grammar) :-
    TagCodes \== [],
    (   append(LangCodes, [0':|GrammarCodes], TagCodes)
    ->  LangCodes \== [], GrammarCodes \== [],
        atom_codes(Language, LangCodes),
        atom_codes(Grammar, GrammarCodes)
    ;   atom_codes(Language, TagCodes),
        Grammar = none
    ).

% A language that needs a target grammar must carry one, and it must be a
% grammar the link-time import registered.
grammar_verdict(Language, none, Verdict) :- !,
    (   pattern_language(Language, _, _, yes)
    ->  Verdict = missing_grammar(Language)
    ;   Verdict = ok
    ).
grammar_verdict(Language, Grammar, Verdict) :-
    (   pattern_language(Language, _, _, no)
    ->  Verdict = grammar_not_applicable(Language)
    ;   imported_grammar(Grammar)
    ->  Verdict = ok
    ;   Verdict = unknown_grammar(Grammar)
    ).

split_close(Codes, BodyCodes, AfterCodes) :-
    split_close_(Codes, [], BodyCodes, AfterCodes).

split_close_([0'|, 0'}|Rest], Acc, BodyCodes, Rest) :- !,
    reverse(Acc, BodyCodes).
split_close_([Code|Rest], Acc, BodyCodes, AfterCodes) :-
    split_close_(Rest, [Code|Acc], BodyCodes, AfterCodes).

% Round trip: a region term printed back to its concrete spelling.
region_text(region(Language, none, Body), Text) :- !,
    format(string(Text), "{|~w||~w|}", [Language, Body]).
region_text(region(Language, Grammar, Body), Text) :-
    format(string(Text), "{|~w:~w||~w|}", [Language, Grammar, Body]).

% ═══════════════════════════════════════════════════════════════════════════
% 3. CAPTURE NAMES ARE OUTPUT COLUMN NAMES
%
% One law across every pattern language: the names a pattern declares are the
% names the extraction atom binds, and they are the only names it binds
% besides the reserved `at`. v5 injected same-named dl variables implicitly;
% here the binding is an explicit named-column argument, so adding a capture
% to a pattern cannot silently mint or shadow a rule variable.
% ═══════════════════════════════════════════════════════════════════════════

reserved_output(at).

region_captures(region(Language, _, Body), Names) :-
    pattern_language(Language, _, CaptureRule, _),
    string_codes(Body, Codes),
    capture_names(CaptureRule, Codes, Raw),
    dedupe(Raw, Names).

capture_names(none, _, []).
capture_names(fixed(Names), _, Names).
capture_names(named_groups, Codes, Names) :- collect_named_groups(Codes, Names).
capture_names(metavars, Codes, Names)     :- collect_metavars(Codes, Names).
capture_names(at_captures, Codes, Names)  :- collect_at_captures(Codes, Names).
capture_names(dollar_names, Codes, Names) :- collect_dollar_names(Codes, Names).

collect_named_groups([], []).
collect_named_groups(Codes, Names) :-
    (   append(`(?<`, AfterOpen, Codes),
        ident_upto(AfterOpen, 0'>, IdentCodes, AfterIdent),
        IdentCodes \== []
    ->  atom_codes(Name, IdentCodes),
        collect_named_groups(AfterIdent, More),
        Names = [Name|More]
    ;   Codes = [_|Tail],
        collect_named_groups(Tail, Names)
    ).

collect_metavars([], []).
collect_metavars([0'$|Rest0], Names) :- !,
    strip_dollars(Rest0, Rest1),
    ident_run(Rest1, upper, IdentCodes, Rest2),
    (   IdentCodes == []
    ->  collect_metavars(Rest2, Names)
    ;   lowercase_atom(IdentCodes, Name),
        collect_metavars(Rest2, More),
        Names = [Name|More]
    ).
collect_metavars([_|Rest], Names) :- collect_metavars(Rest, Names).

collect_at_captures([], []).
collect_at_captures([0'@|Rest0], Names) :- !,
    ident_run(Rest0, any, IdentCodes, Rest1),
    (   IdentCodes == []
    ->  collect_at_captures(Rest1, Names)
    ;   lowercase_atom(IdentCodes, Name),
        collect_at_captures(Rest1, More),
        Names = [Name|More]
    ).
collect_at_captures([_|Rest], Names) :- collect_at_captures(Rest, Names).

collect_dollar_names([], []).
collect_dollar_names([0'$|Rest0], Names) :- !,
    ident_run(Rest0, any, IdentCodes, Rest1),
    (   IdentCodes == []
    ->  collect_dollar_names(Rest1, Names)
    ;   lowercase_atom(IdentCodes, Name),
        collect_dollar_names(Rest1, More),
        Names = [Name|More]
    ).
collect_dollar_names([_|Rest], Names) :- collect_dollar_names(Rest, Names).

strip_dollars([0'$|Rest0], Rest) :- !, strip_dollars(Rest0, Rest).
strip_dollars(Codes, Codes).

ident_run([Code|Rest0], Class, [Code|Ident], Rest) :-
    ident_code(Class, Code), !,
    ident_run(Rest0, Class, Ident, Rest).
ident_run(Codes, _, [], Codes).

ident_code(upper, Code) :- code_type(Code, upper), !.
ident_code(upper, 0'_).
ident_code(any, Code) :- code_type(Code, csym).

ident_upto([Code|Rest], Stop, [], Rest) :- Code == Stop, !.
ident_upto([Code|Rest0], Stop, [Code|Ident], Rest) :-
    ident_upto(Rest0, Stop, Ident, Rest).

lowercase_atom(Codes, Name) :-
    atom_codes(Raw, Codes),
    downcase_atom(Raw, Name).

dedupe([], []).
dedupe([Item|Rest], [Item|Out]) :-
    exclude(==(Item), Rest, Filtered),
    dedupe(Filtered, Out).

% The full set of names an extraction atom may bind for a given pattern.
legal_output_names(Region, [at|Captures]) :- region_captures(Region, Captures).

% ═══════════════════════════════════════════════════════════════════════════
% 4. PATTERN IDENTITY
%
% A pattern literal is a compile-time constant, so its identity is its own
% content. Two rules writing the same pattern text get the same PatternId,
% hence the same kernel rel and one demand row. This is the content-addressed
% dedup the effect model already rules, applied to patterns instead of URLs.
% ═══════════════════════════════════════════════════════════════════════════

pattern_id(region(Language, Grammar, Body), pattern_id(Language, Grammar, Body)).

% ═══════════════════════════════════════════════════════════════════════════
% 5. DESUGARING: candidate spelling -> ruled kernel shapes
%
% Kernel facts produced:
%   kernel_rel(Name, Columns, KeyColumns, Fill, Salt)
%     Fill = world      (the ruled from-world / -> world-filled rel)
%     Salt = input_digest | watch | none
% Kernel goals produced:
%   katom(Name, Args) | kbind(var(Name), Expr)
%
% Salt law, restated per rel family (two-salt law, already ruled: clock bucket
% = time recurrence, input digest = change recurrence):
%   live-tree enumeration : watch        (the bind pushes; no salt column)
%   pinned-tree enumeration: none        (a rev's tree is immutable; never refires)
%   extraction            : input_digest (re-extract exactly on content change)
% ═══════════════════════════════════════════════════════════════════════════

% --- selection over the live tree: no rev, no repo, no WORK ------------------
%   file({|glob||src/**/*.rs|}, source_file)
desugar_goal(atom(file, [Region, var(Out)]), _Env, _Index, Rels, Goals) :- !,
    pattern_id(Region, PatId),
    Rels = [ kernel_rel(enumerate(live, PatId),
                        [col(pattern, 'PatternId'), col(found, 'File')],
                        [pattern], world, watch) ],
    Goals = [ katom(enumerate(live, PatId), [PatId, var(Out)]) ].

% --- selection over a pinned tree: the rev flows in from data ----------------
%   tree_file(rev, {|glob||go.mod|}, manifest)
desugar_goal(atom(tree_file, [var(Rev), Region, var(Out)]), _Env, _Index, Rels, Goals) :- !,
    pattern_id(Region, PatId),
    Rels = [ kernel_rel(enumerate(tree, PatId),
                        [col(rev, 'GitRev'), col(pattern, 'PatternId'), col(found, 'File')],
                        [rev, pattern], world, none) ],
    Goals = [ katom(enumerate(tree, PatId), [var(Rev), PatId, var(Out)]) ].

% --- extraction over a File subject ------------------------------------------
%   match(source_file, {|re||eprintln!|}, at: at)
desugar_goal(natom(match, [var(Subject), Region], Named), Env, Index, Rels, Goals) :-
    subject_kind(Subject, Env, file), !,
    Region = region(Language, _, _),
    pattern_id(Region, PatId),
    region_captures(Region, Captures),
    check_output_names(Region, Named),
    maplist(capture_column, Captures, CaptureCols),
    maplist(capture_arg(Named), Captures, CaptureArgs),
    fresh(digest, Index, DigestVar),
    fresh(start, Index, StartVar),
    fresh(end, Index, EndVar),
    append([col(content, 'Digest'), col(pattern, 'PatternId'),
            col(start, 'Int'), col(end, 'Int')], CaptureCols, Columns),
    Rels = [ kernel_rel(extract(Language, PatId), Columns,
                        [content, pattern], world, input_digest) ],
    append([var(DigestVar), PatId, var(StartVar), var(EndVar)], CaptureArgs, Args),
    span_bind(Named, var(Subject), StartVar, EndVar, SpanGoals),
    append([ kbind(var(DigestVar), field(var(Subject), digest)),
             katom(extract(Language, PatId), Args) ], SpanGoals, Goals).

% --- extraction inside an already-located span --------------------------------
%   match(comment_at, {|re||@eprintln-ok:|}, at: at)
% The window enters the KEY: the same content matched inside two different
% windows is two different demands.
desugar_goal(natom(match, [var(Subject), Region], Named), Env, Index, Rels, Goals) :-
    subject_kind(Subject, Env, span), !,
    Region = region(Language, _, _),
    pattern_id(Region, PatId),
    region_captures(Region, Captures),
    check_output_names(Region, Named),
    maplist(capture_column, Captures, CaptureCols),
    maplist(capture_arg(Named), Captures, CaptureArgs),
    fresh(digest, Index, DigestVar),
    fresh(window_start, Index, WindowStartVar),
    fresh(window_end, Index, WindowEndVar),
    fresh(start, Index, StartVar),
    fresh(end, Index, EndVar),
    append([col(content, 'Digest'), col(window_start, 'Int'), col(window_end, 'Int'),
            col(pattern, 'PatternId'), col(start, 'Int'), col(end, 'Int')],
           CaptureCols, Columns),
    Rels = [ kernel_rel(extract(Language, PatId), Columns,
                        [content, window_start, window_end, pattern],
                        world, input_digest) ],
    append([var(DigestVar), var(WindowStartVar), var(WindowEndVar),
            PatId, var(StartVar), var(EndVar)], CaptureArgs, Args),
    span_bind(Named, field(var(Subject), file), StartVar, EndVar, SpanGoals),
    append([ kbind(var(DigestVar), field(field(var(Subject), file), digest)),
             kbind(var(WindowStartVar), field(var(Subject), start)),
             kbind(var(WindowEndVar), field(var(Subject), end)),
             katom(extract(Language, PatId), Args) ], SpanGoals, Goals).

% --- the comment lexical view: comment regions of a file ----------------------
%   comment_span(source_file, at: comment_at)
desugar_goal(natom(comment_span, [var(Subject)], Named), _Env, Index, Rels, Goals) :- !,
    fresh(digest, Index, DigestVar),
    fresh(start, Index, StartVar),
    fresh(end, Index, EndVar),
    Rels = [ kernel_rel(comment_span,
                        [col(content, 'Digest'), col(start, 'Int'), col(end, 'Int')],
                        [content], world, input_digest) ],
    span_bind(Named, var(Subject), StartVar, EndVar, SpanGoals),
    append([ kbind(var(DigestVar), field(var(Subject), digest)),
             katom(comment_span, [var(DigestVar), var(StartVar), var(EndVar)]) ],
           SpanGoals, Goals).

% --- paired BEGIN/END comment regions: TWO pattern columns, both in the key ---
%   comment_region(source_file, {|re||BEGIN: (?<name>.*)|}, {|re||END:|},
%                  at: block_at, label: name)
desugar_goal(natom(comment_region, [var(Subject), OpenRegion, CloseRegion], Named),
             _Env, Index, Rels, Goals) :- !,
    pattern_id(OpenRegion, OpenId),
    pattern_id(CloseRegion, CloseId),
    fresh(digest, Index, DigestVar),
    fresh(start, Index, StartVar),
    fresh(end, Index, EndVar),
    fresh(label, Index, LabelVar),
    Rels = [ kernel_rel(comment_region(OpenId, CloseId),
                        [col(content, 'Digest'), col(open, 'PatternId'),
                         col(close, 'PatternId'), col(start, 'Int'),
                         col(end, 'Int'), col(label, 'Str')],
                        [content, open, close], world, input_digest) ],
    named_arg(Named, label, var(LabelVar), LabelArg),
    span_bind(Named, var(Subject), StartVar, EndVar, SpanGoals),
    append([ kbind(var(DigestVar), field(var(Subject), digest)),
             katom(comment_region(OpenId, CloseId),
                   [var(DigestVar), OpenId, CloseId,
                    var(StartVar), var(EndVar), LabelArg]) ],
           SpanGoals, Goals).

% --- the line/col view over a span (presentation edge only) -------------------
desugar_goal(natom(line_of, [var(Subject)], Named), _Env, Index, Rels, Goals) :- !,
    fresh(digest, Index, DigestVar),
    fresh(offset, Index, OffsetVar),
    fresh(line, Index, LineVar),
    fresh(col, Index, ColVar),
    Rels = [ kernel_rel(line_of,
                        [col(content, 'Digest'), col(offset, 'Int'),
                         col(line, 'Int'), col(col, 'Int')],
                        [content, offset], world, input_digest) ],
    named_arg(Named, line, var(LineVar), LineArg),
    named_arg(Named, col, var(ColVar), ColArg),
    Goals = [ kbind(var(DigestVar), field(field(var(Subject), file), digest)),
              kbind(var(OffsetVar), field(var(Subject), start)),
              katom(line_of, [var(DigestVar), var(OffsetVar), LineArg, ColArg]) ].

% --- everything else is an ordinary body goal, passed through -----------------
desugar_goal(Goal, _Env, _Index, [], [kgoal(Goal)]).

capture_column(Name, col(Name, 'Str')).

capture_arg(Named, Name, Arg) :- named_arg(Named, Name, wild, Arg).

named_arg(Named, Name, Default, Arg) :-
    (   member(Name-Bound, Named)
    ->  Arg = Bound
    ;   Arg = Default
    ).

% The FileSpan is minted AT THE JOIN, not inside the kernel rel. That is what
% makes content-addressed dedup correct: the kernel row is content-relative,
% so N files sharing one digest consume ONE extract row and each mints its own
% FileSpan against its own File value.
span_bind(Named, FileExpr, StartVar, EndVar, Goals) :-
    (   member(at-var(AtVar), Named)
    ->  Goals = [ kbind(var(AtVar),
                        struct('FileSpan', [file-FileExpr,
                                            start-var(StartVar),
                                            end-var(EndVar)])) ]
    ;   Goals = []
    ).

check_output_names(Region, Named) :-
    legal_output_names(Region, Legal),
    forall(member(Name-_, Named), memberchk(Name, Legal)).

fresh(Base, Index, Name) :-
    atomic_list_concat([Base, '_', Index], Name).

subject_kind(Name, Env, Kind) :-
    (   memberchk(Name-'FileSpan', Env)
    ->  Kind = span
    ;   Kind = file
    ).

% Whole-body desugar with a left-to-right type environment.
desugar_body(Body, Rels, Goals) :-
    body_type_env(Body, Env),
    desugar_body_(Body, Env, 1, Rels, Goals).

desugar_body_([], _, _, [], []).
desugar_body_([Goal|Rest], Env, Index, Rels, Goals) :-
    desugar_goal(Goal, Env, Index, RelsHere, GoalsHere),
    NextIndex is Index + 1,
    desugar_body_(Rest, Env, NextIndex, RelsRest, GoalsRest),
    append(RelsHere, RelsRest, Rels),
    append(GoalsHere, GoalsRest, Goals).

body_type_env(Body, Env) :- foldl(collect_types, Body, [], Env).

collect_types(atom(file, [_, var(Name)]), Env, [Name-'File'|Env]) :- !.
collect_types(atom(tree_file, [_, _, var(Name)]), Env, [Name-'File'|Env]) :- !.
collect_types(natom(_, _, Named), Env0, Env) :- !,
    findall(Name-'FileSpan', member(at-var(Name), Named), Pairs),
    append(Pairs, Env0, Env).
collect_types(_, Env, Env).

% ═══════════════════════════════════════════════════════════════════════════
% 6. A TINY WORLD EVALUATOR
%
% Enough to run desugared kernel goals against canned world rows, so the
% content-addressing claims are measured rather than narrated.
% Values: file(Repo, Path, Digest) and fspan(File, Start, End).
% ═══════════════════════════════════════════════════════════════════════════

wrow(enumerate(live, pattern_id(glob, none, "src/**/*.rs")),
     [pattern_id(glob, none, "src/**/*.rs"), file(sprefa, 'src/a.rs', digest_one)]).
wrow(enumerate(live, pattern_id(glob, none, "src/**/*.rs")),
     [pattern_id(glob, none, "src/**/*.rs"), file(sprefa, 'src/b.rs', digest_one)]).
wrow(enumerate(live, pattern_id(glob, none, "src/**/*.rs")),
     [pattern_id(glob, none, "src/**/*.rs"), file(sprefa, 'src/c.rs', digest_two)]).

wrow(extract(re, pattern_id(re, none, "eprintln!")),
     [digest_one, pattern_id(re, none, "eprintln!"), 12, 21]).
wrow(extract(re, pattern_id(re, none, "eprintln!")),
     [digest_two, pattern_id(re, none, "eprintln!"), 40, 49]).

run_goals([], Env, Env).
run_goals([kbind(var(Name), Expr)|Rest], Env0, Env) :-
    eval_expr(Expr, Env0, Value),
    bind_var(Name, Value, Env0, Env1),
    run_goals(Rest, Env1, Env).
run_goals([katom(Rel, Args)|Rest], Env0, Env) :-
    wrow(Rel, Row),
    unify_args(Args, Row, Env0, Env1),
    run_goals(Rest, Env1, Env).
run_goals([kgoal(_)|Rest], Env0, Env) :-
    run_goals(Rest, Env0, Env).

unify_args([], [], Env, Env).
unify_args([Arg|Args], [Value|Values], Env0, Env) :-
    unify_arg(Arg, Value, Env0, Env1),
    unify_args(Args, Values, Env1, Env).

unify_arg(wild, _, Env, Env) :- !.
unify_arg(var(Name), Value, Env0, Env) :- !, bind_var(Name, Value, Env0, Env).
unify_arg(Constant, Value, Env, Env) :- Constant == Value.

bind_var(Name, Value, Env, Env) :- memberchk(Name-Existing, Env), !, Existing == Value.
bind_var(Name, Value, Env, [Name-Value|Env]).

eval_expr(var(Name), Env, Value) :- !, memberchk(Name-Value, Env).
eval_expr(field(Inner, Field), Env, Value) :- !,
    eval_expr(Inner, Env, Base),
    project(Base, Field, Value).
eval_expr(struct('FileSpan', Fields), Env, fspan(File, Start, End)) :- !,
    memberchk(file-FileExpr, Fields), eval_expr(FileExpr, Env, File),
    memberchk(start-StartExpr, Fields), eval_expr(StartExpr, Env, Start),
    memberchk(end-EndExpr, Fields), eval_expr(EndExpr, Env, End).
eval_expr(Constant, _, Constant).

project(file(Repo, _, _), repo, Repo).
project(file(_, Path, _), path, Path).
project(file(_, _, Digest), digest, Digest).
project(fspan(File, _, _), file, File).
project(fspan(_, Start, _), start, Start).
project(fspan(_, _, End), end, End).

% ═══════════════════════════════════════════════════════════════════════════
% 7. THE ADVERSARIAL LAW ON THE OUTER GRAMMAR
%
% "No single-character perturbation of a legal spelling silently yields a
% different legal spelling." Scope: the OUTER grammar, i.e. the fence and the
% language tag. Inside a region the sublanguage owns its own semantics and a
% one-character edit legitimately changes meaning, exactly as inside a string
% literal (deviation 2 in the .md).
% ═══════════════════════════════════════════════════════════════════════════

perturbation_alphabet([0'a, 0'e, 0'g, 0'j, 0'l, 0'n, 0'o, 0'p, 0'r, 0's,
                       0't, 0'b, 0'h, 0'|, 0'{, 0'}, 0':, 0'0]).

% Every single-character deletion and substitution inside the fence region of
% Text either fails to lex or lexes to the SAME region term.
fence_perturbations_safe(Text, Region) :-
    string_codes(Text, Codes),
    fence_positions(Codes, Positions),
    forall(( member(Position, Positions),
             perturb(Codes, Position, PerturbedCodes) ),
           ( string_codes(Perturbed, PerturbedCodes),
             lex_region(Perturbed, Result),
             ( Result = ok(Other) -> Other == Region ; true ) )).

% Fence positions: the opening "{|tag||" prefix and the closing "|}" suffix.
% Body positions are excluded on purpose (deviation 2: the sublanguage owns
% its own one-character meanings, as any string literal does).
fence_positions(Codes, Positions) :-
    length(Codes, Length),
    once(( append(OpenPart, _, Codes), append(_, `||`, OpenPart) )),
    length(OpenPart, OpenLength),
    CloseStart is Length - 2,
    LastIndex is Length - 1,
    numlist(0, LastIndex, AllPositions),
    exclude(inside_body(OpenLength, CloseStart), AllPositions, Positions).

inside_body(OpenLength, CloseStart, Position) :-
    Position >= OpenLength, Position < CloseStart.

perturb(Codes, Position, Perturbed) :- delete_at(Codes, Position, Perturbed).
perturb(Codes, Position, Perturbed) :-
    perturbation_alphabet(Alphabet),
    member(Code, Alphabet),
    substitute_at(Codes, Position, Code, Perturbed).

delete_at(Codes, Position, Perturbed) :-
    length(Prefix, Position),
    append(Prefix, [_|Suffix], Codes),
    append(Prefix, Suffix, Perturbed).

substitute_at(Codes, Position, Code, Perturbed) :-
    length(Prefix, Position),
    append(Prefix, [Old|Suffix], Codes),
    Old \== Code,
    append(Prefix, [Code|Suffix], Perturbed).

% The tag set property that makes the substitution case hold: equal-length
% tags are pairwise at Hamming distance >= 2, so no one-character typo turns a
% legal tag into another legal tag.
tag_hamming_ok :- name_set_hamming_ok(known_language).
grammar_hamming_ok :- name_set_hamming_ok(imported_grammar).

name_set_hamming_ok(SetGoal) :-
    forall(( call(SetGoal, Left), call(SetGoal, Right), Left \== Right,
             atom_length(Left, Length), atom_length(Right, Length) ),
           ( hamming(Left, Right, Distance), Distance >= 2 )).

% The ast-grep short tags do NOT have the property (`ts` and `js` are one
% character apart), which is why the imported grammar set spells the full
% names. Measured, not asserted.
ast_grep_short_tags_fail_hamming :-
    \+ forall(( member(Left, [rust, ts, tsx, js, py, go, json, c, cpp, kotlin]),
                member(Right, [rust, ts, tsx, js, py, go, json, c, cpp, kotlin]),
                Left \== Right,
                atom_length(Left, Length), atom_length(Right, Length) ),
              ( hamming(Left, Right, Distance), Distance >= 2 )).

hamming(Left, Right, Distance) :-
    atom_codes(Left, LeftCodes),
    atom_codes(Right, RightCodes),
    foldl(hamming_step, LeftCodes, RightCodes, 0, Distance).

hamming_step(LeftCode, RightCode, Acc, Out) :-
    ( LeftCode == RightCode -> Out = Acc ; Out is Acc + 1 ).

% ═══════════════════════════════════════════════════════════════════════════
% 8. THE v5 ARITY-DEFAULT PATHOLOGY, MODELLED
%
% v5 `scan` defaults by arity: scan/3 = (glob, path, rev_out), scan/4 =
% (rev, glob, path, rev_out), scan/5 = (repo, rev, glob, path, rev_out). One
% deleted argument turns a legal call into another legal call with a different
% column meaning and no diagnostic. Same shape in `comment`: one regex means
% sequential dividers, two means paired BEGIN/END.
% ═══════════════════════════════════════════════════════════════════════════

v5_scan_shape(3, [glob, path, rev_out]).
v5_scan_shape(4, [rev, glob, path, rev_out]).
v5_scan_shape(5, [repo, rev, glob, path, rev_out]).

v5_comment_shape(6, [path, rev, open, line_start, line_end, label]).
v5_comment_shape(7, [path, rev, open, close, line_start, line_end, label]).

% A shape family admits a silent reinterpretation when dropping one argument
% from a legal call leaves another legal call whose column meanings differ.
silent_reinterpretation(ShapeGoal) :-
    call(ShapeGoal, Arity, Columns),
    Smaller is Arity - 1,
    call(ShapeGoal, Smaller, OtherColumns),
    Columns \== OtherColumns.

% The v6 replacement: two differently NAMED rels, no arity defaulting.
v6_selection_shape(file, [pattern, found]).
v6_selection_shape(tree_file, [rev, pattern, found]).

no_arity_overlap :-
    forall(( v6_selection_shape(NameLeft, ColumnsLeft),
             v6_selection_shape(NameRight, ColumnsRight),
             NameLeft \== NameRight ),
           ( length(ColumnsLeft, LengthLeft),
             length(ColumnsRight, LengthRight),
             LengthLeft \== LengthRight )).

% ═══════════════════════════════════════════════════════════════════════════
% 9. CONSTRUCT CENSUS
%
% Everything a transcription uses must be on AGGREGATE's keep-list, or carry a
% proposal row with its budget cost.
% ═══════════════════════════════════════════════════════════════════════════

kept(rel_decl).       kept(level_rule).      kept(fact).
kept(negation).       kept(aggregate_head).  kept(comparison).
kept(arithmetic).     kept(bind_goal).       kept(fn_application).
kept(named_column_atom). kept(wildcard).     kept(snapshot_ask).
kept(from_world).     kept(effect_arrow).    kept(quoted_region).
kept(match_relation). kept(key_wrapper).     kept(dot_access).
kept(struct_literal). kept(enum_decl).       kept(interpolation).
kept(graph_operator). kept(module_import).   kept(option_type).
kept(struct_pattern).

% proposed(Name, GrammarConstructCost, Rationale)
proposed(pattern_language_set, 0,
         'tags added to the kept quoted-region construct; CLOSES the still-missing "regex + path literals" T1 row at zero grammar cost').
proposed(grammar_tag_parameter, 0,
         '{|sg:rust|| ... |}: the target grammar must be known to PARSE the region, so it cannot be an argument outside it (astgrep_patterns.md:99-110)').
proposed(reserved_output_at, 0,
         'one reserved output name; every extraction atom exposes at: FileSpan').
proposed(capture_names_are_columns, 0,
         'static law: a pattern capture name IS an output column name; replaces v5 implicit variable injection').
proposed(demand_key_is_left_of_arrow, 0,
         'reading that removes Key() wrappers from effect left sides; feeds ruling Q8').
proposed(multiplicity_at_link_time, 0,
         'many-rows-per-demand is a bind obligation, not a Set(T) result wrapper').
proposed(stdlib_extraction_rels, 0,
         'file, tree_file, content, match, comment_span, comment_region, line_of: library rel names, no grammar').

budgeted(Construct) :- kept(Construct), !.
budgeted(Construct) :- proposed(Construct, _, _).

node_construct(rel_decl(_, _, world), from_world).
node_construct(rel_decl(_, _, arrow(_)), effect_arrow).
node_construct(rel_decl(_, _, _), rel_decl).
node_construct(enum_decl(_, _), enum_decl).
node_construct(fact(_), fact).
node_construct((_ <- _), level_rule).
node_construct(neg(_), negation).
node_construct(bind(_, _), bind_goal).
node_construct(cmp(_, _, _), comparison).
node_construct(arith(_, _, _), arithmetic).
node_construct(region(_, _, _), quoted_region).
node_construct(region(Language, Grammar, _), grammar_tag_parameter) :-
    Grammar \== none, known_language(Language).
node_construct(region(Language, _, _), pattern_language_set) :- known_language(Language).
node_construct(field(_, _), dot_access).
node_construct(struct(_, _), struct_literal).
node_construct(agg(_, _), aggregate_head).
node_construct(call(_, _), fn_application).
node_construct(ctor(_), enum_decl).
node_construct(pat(_, _), struct_pattern).
node_construct(natom(match, _, _), match_relation).
node_construct(natom(Name, _, _), stdlib_extraction_rels) :- stdlib_rel(Name).
node_construct(natom(_, _, _), named_column_atom).
node_construct(atom(Name, _), stdlib_extraction_rels) :- stdlib_rel(Name).
node_construct(ask(_, _), snapshot_ask).
node_construct(wild, wildcard).
node_construct(col(_, key(_, _)), key_wrapper).
node_construct(use_module(_), module_import).
node_construct(closure(_), graph_operator).
node_construct(Named-_, named_column_atom) :- atom(Named).

stdlib_rel(file).           stdlib_rel(tree_file).
stdlib_rel(content).        stdlib_rel(comment_span).
stdlib_rel(comment_region). stdlib_rel(line_of).

walk_term(Term, Construct) :- node_construct(Term, Construct).
walk_term(Term, Construct) :-
    compound(Term), arg(_, Term, Argument), walk_term(Argument, Construct).

constructs_used(Term, Constructs) :-
    findall(Construct, walk_term(Term, Construct), Raw),
    sort(Raw, Constructs).

all_constructs_budgeted(Term) :-
    constructs_used(Term, Constructs),
    forall(member(Construct, Constructs), budgeted(Construct)).

% ═══════════════════════════════════════════════════════════════════════════
% 10. THE THREE TRANSCRIPTIONS
%
% Surface text lives in extraction_syntax.md; these are the same programs as
% terms, so the census and the no-coordinate laws are mechanical.
% ═══════════════════════════════════════════════════════════════════════════

% ---- .dl/no-new-eprintln.dl -------------------------------------------------
program(eprintln_rail, [
    enum_decl('Severity', ['Error', 'Warning']),

    rel_decl(eprintln_hit, [col(at, 'FileSpan')], none),
    ( atom(eprintln_hit, [var(at)]) <-
      [ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]),
        natom(match, [var(source_file), region(re, none, "eprintln!")],
              [at-var(at)]) ] ),

    rel_decl(eprintln_waiver, [col(at, 'FileSpan')], none),
    ( atom(eprintln_waiver, [var(at)]) <-
      [ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]),
        natom(comment_span, [var(source_file)], [at-var(comment_at)]),
        natom(match, [var(comment_at), region(re, none, "@eprintln-ok:")],
              [at-var(at)]) ] ),

    rel_decl(eprintln_waived, [col(at, 'FileSpan')], none),
    ( atom(eprintln_waived, [var(hit_at)]) <-
      [ atom(eprintln_hit, [var(hit_at)]),
        atom(eprintln_waiver, [var(waiver_at)]),
        cmp('==', field(var(hit_at), file), field(var(waiver_at), file)),
        natom(line_of, [var(hit_at)], [line-var(hit_line)]),
        natom(line_of, [var(waiver_at)], [line-var(waiver_line)]),
        cmp('>=', var(waiver_line), arith('-', var(hit_line), int(1))),
        cmp('=<', var(waiver_line), var(hit_line)) ] ),

    rel_decl(eprintln_counted, [col(at, 'FileSpan')], none),
    ( atom(eprintln_counted, [var(at)]) <-
      [ atom(eprintln_hit, [var(at)]),
        neg(atom(eprintln_waived, [var(at)])) ] ),

    rel_decl(eprintln_count, [col(source_file, 'File'), col(hits, 'Int')], none),
    ( atom(eprintln_count, [var(source_file), agg(count, var(at))]) <-
      [ atom(eprintln_counted, [var(at)]),
        bind(var(source_file), field(var(at), file)) ] ),

    rel_decl(eprintln_baseline, [col(path, key('Path', 1)), col(allowed, 'Int')], none),
    fact(atom(eprintln_baseline, [region(path, none, "src/config.rs"), int(1)])),
    fact(atom(eprintln_baseline, [region(path, none, "src/daemon/client.rs"), int(2)])),
    fact(atom(eprintln_baseline, [region(path, none, "src/setup/vscode.rs"), int(1)])),
    fact(atom(eprintln_baseline, [region(path, none, "src/setup/wire.rs"), int(1)])),

    ( natom(diag, [],
            [ at-struct('FileSpan', [file-var(source_file), start-int(0), end-int(0)]),
              severity-ctor('Warning'),
              code-str("eprintln-exceeded"),
              msg-str("eprintln! use grew past this file's grandfathered baseline") ]) <-
      [ atom(eprintln_count, [var(source_file), var(hits)]),
        bind(var(baseline_path), field(var(source_file), path)),
        atom(eprintln_baseline, [var(baseline_path), var(allowed)]),
        cmp('>', var(hits), var(allowed)) ] ),

    ( natom(diag, [],
            [ at-var(at),
              severity-ctor('Warning'),
              code-str("eprintln-new-file"),
              msg-str("new eprintln! outside the grandfathered baseline") ]) <-
      [ atom(eprintln_counted, [var(at)]),
        bind(var(baseline_path), field(field(var(at), file), path)),
        neg(atom(eprintln_baseline, [var(baseline_path), wild])) ] ),

    fact(atom(diag_stage, [str("eprintln-exceeded"), str("agent-turn")])),
    fact(atom(diag_stage, [str("eprintln-exceeded"), str("commit")])),
    fact(atom(diag_stage, [str("eprintln-new-file"), str("agent-turn")])),
    fact(atom(diag_stage, [str("eprintln-new-file"), str("commit")])),

    ask(diag, [at, severity, code, msg])
]).

% ---- examples/pin-skew.dl ---------------------------------------------------
program(pin_skew, [
    enum_decl('RevLookup', ['Found', 'Unresolvable']),

    rel_decl(git_repo, [col(repo, key('GitRepo', 1))], none),
    rel_decl(repo_default_ref, [col(repo, key('GitRepo', 1)), col(ref_name, 'RefName')], world),
    rel_decl(repo_ref, [col(repo, key('GitRepo', 1)), col(ref_name, key('RefName', 2)),
                        col(rev, 'GitRev')], world),
    rel_decl(resolve_rev, [col(repo, 'GitRepo'), col(ref_text, 'Str')], arrow(['RevLookup'])),
    rel_decl(rev_behind, [col(repo, 'GitRepo'), col(rev, 'GitRev'), col(base, 'GitRev')],
             arrow([col(behind, 'Int'), col(ahead, 'Int')])),

    rel_decl(default_tree, [col(repo, 'GitRepo'), col(rev, 'GitRev')], none),
    ( atom(default_tree, [var(repo), var(rev)]) <-
      [ atom(git_repo, [var(repo)]),
        atom(repo_default_ref, [var(repo), var(ref_name)]),
        atom(repo_ref, [var(repo), var(ref_name), var(rev)]) ] ),

    rel_decl(module_id, [col(repo, 'GitRepo'), col(module_path, 'Str')], none),
    ( atom(module_id, [var(repo), var(module_path)]) <-
      [ atom(default_tree, [var(repo), var(rev)]),
        atom(tree_file, [var(rev), region(glob, none, "go.mod"), var(manifest)]),
        natom(match, [var(manifest),
                      region(re, none, "^module\\s+(?<module_path>\\S+)")],
              [module_path-var(module_path)]) ] ),

    rel_decl(gomod_pin, [col(consumer, 'GitRepo'), col(module_path, 'Str'),
                         col(version, 'Str')], none),
    ( atom(gomod_pin, [var(consumer), var(module_path), var(version)]) <-
      [ atom(default_tree, [var(consumer), var(rev)]),
        atom(tree_file, [var(rev), region(glob, none, "go.mod"), var(manifest)]),
        natom(match, [var(manifest),
                      region(re, none, "(?<module_path>\\S+/\\S+)\\s+(?<version>v[0-9]\\S*)")],
              [module_path-var(module_path), version-var(version)]) ] ),

    rel_decl(pin, [col(consumer, 'GitRepo'), col(dep, 'GitRepo'), col(ref_text, 'Str')], none),
    ( atom(pin, [var(consumer), var(dep), var(version)]) <-
      [ atom(gomod_pin, [var(consumer), var(module_path), var(version)]),
        atom(module_id, [var(dep), var(module_path)]),
        cmp('!=', var(consumer), var(dep)) ] ),

    rel_decl(pinned_rev, [col(dep, 'GitRepo'), col(ref_text, 'Str'), col(rev, 'GitRev')], none),
    ( atom(pinned_rev, [var(dep), var(ref_text), var(rev)]) <-
      [ atom(pin, [wild, var(dep), var(ref_text)]),
        atom(resolve_rev, [var(dep), var(ref_text),
                           pat(ctor('Found'), [rev-var(rev)])]) ] ),

    rel_decl(unresolvable_pin, [col(dep, 'GitRepo'), col(ref_text, 'Str'),
                                col(reason, 'Str')], none),
    ( atom(unresolvable_pin, [var(dep), var(ref_text), var(reason)]) <-
      [ atom(pin, [wild, var(dep), var(ref_text)]),
        atom(resolve_rev, [var(dep), var(ref_text),
                           pat(ctor('Unresolvable'), [reason-var(reason)])]) ] ),

    rel_decl(stale_pin, [col(consumer, 'GitRepo'), col(dep, 'GitRepo'),
                         col(ref_text, 'Str'), col(behind, 'Int')], none),
    ( atom(stale_pin, [var(consumer), var(dep), var(ref_text), var(behind)]) <-
      [ atom(pin, [var(consumer), var(dep), var(ref_text)]),
        atom(pinned_rev, [var(dep), var(ref_text), var(pinned)]),
        atom(default_tree, [var(dep), var(tip)]),
        natom(rev_behind, [var(dep), var(pinned), var(tip)], [behind-var(behind)]),
        cmp('>', var(behind), int(0)) ] ),

    rel_decl(diverged_pin, [col(consumer, 'GitRepo'), col(dep, 'GitRepo'),
                            col(ref_text, 'Str')], none),
    ( atom(diverged_pin, [var(consumer), var(dep), var(ref_text)]) <-
      [ atom(pin, [var(consumer), var(dep), var(ref_text)]),
        atom(pinned_rev, [var(dep), var(ref_text), var(pinned)]),
        atom(default_tree, [var(dep), var(tip)]),
        natom(rev_behind, [var(dep), var(pinned), var(tip)], [ahead-var(ahead)]),
        cmp('>', var(ahead), int(0)) ] ),

    ask(stale_pin, [consumer, dep, ref_text, behind]),
    ask(diverged_pin, [consumer, dep, ref_text]),
    ask(unresolvable_pin, [dep, ref_text, reason])
]).

% ---- examples/flow-services.dl ----------------------------------------------
program(flow_services, [
    use_module("std/flow"),

    rel_decl(service_op, [col(op, 'Str')], none),
    ( atom(service_op, [var(op)]) <-
      [ atom(file, [region(glob, none, "**/openapi.yaml"), var(spec)]),
        natom(match, [var(spec), region(jsonpath, none, "paths.*.*.operationId")],
              [value-var(op)]) ] ),

    rel_decl(op_endpoint, [col(op, 'Str'), col(fn_bare, 'Str'), col(sym, 'Str')], none),
    ( atom(op_endpoint, [var(op), var(fn_bare), var(sym)]) <-
      [ atom(service_op, [var(op)]),
        atom(call_name, [var(sym), var(op)]),
        bind(var(fn_bare),
             call(replace_re, [var(sym), region(re, none, "^[^:]*::"), str("")])) ] ),

    rel_decl(wire_call, [col(op, 'Str'), col(at, 'FileSpan')], none),
    ( atom(wire_call, [var(op), var(at)]) <-
      [ atom(service_op, [var(op)]),
        atom(call_node, [wild, var(op), var(at)]) ] ),

    ( atom(flow_edge, [var(argument), var(parameter)]) <-
      [ atom(service_op, [var(op)]),
        atom(call_node, [var(call), var(op), wild]),
        atom(df_arg, [var(call), var(position), var(argument)]),
        atom(op_endpoint, [var(op), var(fn_bare), wild]),
        natom(df_node, [var(parameter)], [kind-str("param"), fn-var(fn_bare)]),
        atom(df_param, [var(parameter), var(position)]) ] ),

    ( atom(flow_edge, [var(returned), var(call)]) <-
      [ atom(service_op, [var(op)]),
        atom(call_node, [var(call), var(op), wild]),
        atom(op_endpoint, [var(op), var(fn_bare), wild]),
        natom(df_node, [var(returned)], [kind-str("ret"), fn-var(fn_bare)]) ] ),

    rel_decl(service_reach, [col(from_node, 'Str'), col(to_node, 'Str')], none),
    ( atom(service_reach, [var(from_node), var(to_node)]) <-
      [ closure(flow_edge) ] ),

    ask(service_op, [op]),
    ask(wire_call, [op, at]),
    ask(op_endpoint, [op, fn_bare, sym]),
    ask(service_reach, [from_node, to_node])
]).

% No rev coordinate and no world alias may appear as program text.
banned_literal("WORK").
banned_literal("HEAD").
banned_literal('WORK').
banned_literal('HEAD').

contains_banned_literal(Term) :-
    walk_value(Term, Value),
    banned_literal(Value).

walk_value(Term, Term) :- atomic(Term).
walk_value(Term, Value) :- compound(Term), arg(_, Term, Argument), walk_value(Argument, Value).

% ═══════════════════════════════════════════════════════════════════════════
% 11. CHECKS
% ═══════════════════════════════════════════════════════════════════════════

% ---- region lexing ----------------------------------------------------------

check(region_lexes_plain,
      ( lex_region("{|re||eprintln!|}", Result),
        Result == ok(region(re, none, "eprintln!")) )).

check(region_lexes_grammar_parameter,
      ( lex_region("{|sg:rust||$RECEIVER.unwrap()|}", Result),
        Result == ok(region(sg, rust, "$RECEIVER.unwrap()")) )).

% The v5 waiver regex is /\/\/.*@eprintln-ok:/ : two escapes forced by the
% slash delimiter. The region form carries the same pattern with zero escapes.
check(region_body_carries_slashes_unescaped,
      ( lex_region("{|re||//.*@eprintln-ok:|}", Result),
        Result == ok(region(re, none, "//.*@eprintln-ok:")),
        Result = ok(Region),
        region_text(Region, Text),
        Text == "{|re||//.*@eprintln-ok:|}" )).

check(region_rejects_unknown_language,
      ( lex_region("{|rx||eprintln!|}", Result),
        Result == bad(unknown_language(rx)) )).

check(region_rejects_unknown_grammar,
      ( lex_region("{|sg:rest||$RECEIVER.unwrap()|}", Result),
        Result == bad(unknown_grammar(rest)) )).

check(region_rejects_missing_grammar,
      ( lex_region("{|sg||$RECEIVER.unwrap()|}", Result),
        Result == bad(missing_grammar(sg)) )).

check(region_rejects_grammar_on_a_grammarless_language,
      ( lex_region("{|re:rust||eprintln!|}", Result),
        Result == bad(grammar_not_applicable(re)) )).

check(region_rejects_single_bar_opener,
      ( lex_region("{|re|eprintln!|}", Result),
        Result == bad(malformed_open_fence) )).

check(region_rejects_missing_close,
      ( lex_region("{|re||eprintln!}", Result),
        Result == bad(unterminated_region) )).

check(region_rejects_close_fence_inside_body,
      ( lex_region("{|re||a|}b|}", Result),
        Result == bad(text_after_close_fence) )).

% ---- the adversarial law ----------------------------------------------------

check(language_tags_pairwise_hamming_two, tag_hamming_ok).
check(grammar_names_pairwise_hamming_two, grammar_hamming_ok).
check(ast_grep_short_tags_would_break_the_law, ast_grep_short_tags_fail_hamming).

check(fence_perturbation_never_silently_legal,
      forall( member(Text, ["{|re||eprintln!|}",
                            "{|sg:rust||$RECEIVER.unwrap()|}",
                            "{|glob||src/**/*.rs|}"]),
              ( lex_region(Text, ok(Region)),
                fence_perturbations_safe(Text, Region) ) )).

% ---- capture names ----------------------------------------------------------

check(captures_regex_named_groups,
      ( region_captures(region(re, none, "(?<module_path>\\S+/\\S+)\\s+(?<version>v[0-9]\\S*)"),
                        Names),
        Names == [module_path, version] )).

check(captures_sg_metavars_lowercased,
      ( region_captures(region(sg, rust, "$RECEIVER.unwrap($$$ARGS)"), Names),
        Names == [receiver, args] )).

check(captures_ts_at_captures,
      ( region_captures(region(ts, rust, "(call_expression function: (identifier) @callee)"), Names),
        Names == [callee] )).

check(captures_jsonpath_is_value,
      ( region_captures(region(jsonpath, none, "paths.*.*.operationId"), Names),
        Names == [value] )).

check(captures_glob_has_none,
      ( region_captures(region(glob, none, "src/**/*.rs"), Names),
        Names == [] )).

check(captures_dedupe_nonlinear_pattern,
      ( region_captures(region(sg, rust, "$LEFT == $LEFT"), Names),
        Names == [left] )).

% ---- desugaring round trips -------------------------------------------------

check(desugar_live_selection,
      ( desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]) ],
                     Rels, Goals),
        Rels == [ kernel_rel(enumerate(live, pattern_id(glob, none, "src/**/*.rs")),
                             [col(pattern, 'PatternId'), col(found, 'File')],
                             [pattern], world, watch) ],
        Goals == [ katom(enumerate(live, pattern_id(glob, none, "src/**/*.rs")),
                         [pattern_id(glob, none, "src/**/*.rs"), var(source_file)]) ] )).

check(desugar_pinned_selection,
      ( desugar_body([ atom(tree_file, [var(rev), region(glob, none, "go.mod"), var(manifest)]) ],
                     Rels, Goals),
        Rels == [ kernel_rel(enumerate(tree, pattern_id(glob, none, "go.mod")),
                             [col(rev, 'GitRev'), col(pattern, 'PatternId'), col(found, 'File')],
                             [rev, pattern], world, none) ],
        Goals == [ katom(enumerate(tree, pattern_id(glob, none, "go.mod")),
                         [var(rev), pattern_id(glob, none, "go.mod"), var(manifest)]) ] )).

check(desugar_regex_extraction,
      ( desugar_body([ atom(file, [region(glob, none, "*.go"), var(manifest)]),
                       natom(match, [var(manifest),
                                     region(re, none, "^module\\s+(?<module_path>\\S+)")],
                             [at-var(at), module_path-var(module_path)]) ],
                     Rels, Goals),
        memberchk(kernel_rel(extract(re, pattern_id(re, none, "^module\\s+(?<module_path>\\S+)")),
                             [col(content, 'Digest'), col(pattern, 'PatternId'),
                              col(start, 'Int'), col(end, 'Int'),
                              col(module_path, 'Str')],
                             [content, pattern], world, input_digest),
                  Rels),
        memberchk(kbind(var(digest_2), field(var(manifest), digest)), Goals),
        memberchk(katom(extract(re, pattern_id(re, none, "^module\\s+(?<module_path>\\S+)")),
                        [var(digest_2), pattern_id(re, none, "^module\\s+(?<module_path>\\S+)"),
                         var(start_2), var(end_2), var(module_path)]),
                  Goals),
        memberchk(kbind(var(at), struct('FileSpan', [file-var(manifest),
                                                     start-var(start_2),
                                                     end-var(end_2)])),
                  Goals) )).

check(desugar_sg_extraction_carries_grammar,
      ( desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]),
                       natom(match, [var(source_file), region(sg, rust, "$RECEIVER.unwrap()")],
                             [at-var(at), receiver-var(receiver)]) ],
                     Rels, _Goals),
        memberchk(kernel_rel(extract(sg, pattern_id(sg, rust, "$RECEIVER.unwrap()")),
                             [col(content, 'Digest'), col(pattern, 'PatternId'),
                              col(start, 'Int'), col(end, 'Int'), col(receiver, 'Str')],
                             [content, pattern], world, input_digest),
                  Rels) )).

check(desugar_jsonpath_extraction,
      ( desugar_body([ atom(file, [region(glob, none, "**/openapi.yaml"), var(spec)]),
                       natom(match, [var(spec), region(jsonpath, none, "paths.*.*.operationId")],
                             [value-var(op)]) ],
                     Rels, Goals),
        memberchk(kernel_rel(extract(jsonpath, pattern_id(jsonpath, none, "paths.*.*.operationId")),
                             [col(content, 'Digest'), col(pattern, 'PatternId'),
                              col(start, 'Int'), col(end, 'Int'), col(value, 'Str')],
                             [content, pattern], world, input_digest),
                  Rels),
        % `at` was not asked for, so no FileSpan is minted.
        \+ member(kbind(_, struct('FileSpan', _)), Goals) )).

check(desugar_span_subject_keys_the_window,
      ( desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]),
                       natom(comment_span, [var(source_file)], [at-var(comment_at)]),
                       natom(match, [var(comment_at), region(re, none, "@eprintln-ok:")],
                             [at-var(at)]) ],
                     Rels, _Goals),
        memberchk(kernel_rel(extract(re, pattern_id(re, none, "@eprintln-ok:")),
                             [col(content, 'Digest'), col(window_start, 'Int'),
                              col(window_end, 'Int'), col(pattern, 'PatternId'),
                              col(start, 'Int'), col(end, 'Int')],
                             [content, window_start, window_end, pattern],
                             world, input_digest),
                  Rels) )).

check(desugar_comment_region_two_pattern_columns,
      ( desugar_body([ atom(file, [region(glob, none, "**/*.dl"), var(source_file)]),
                       natom(comment_region, [var(source_file),
                                              region(re, none, "BEGIN: (?<name>.*)"),
                                              region(re, none, "END:")],
                             [at-var(at), label-var(name)]) ],
                     Rels, _Goals),
        memberchk(kernel_rel(comment_region(pattern_id(re, none, "BEGIN: (?<name>.*)"),
                                            pattern_id(re, none, "END:")),
                             [col(content, 'Digest'), col(open, 'PatternId'),
                              col(close, 'PatternId'), col(start, 'Int'),
                              col(end, 'Int'), col(label, 'Str')],
                             [content, open, close], world, input_digest),
                  Rels) )).

check(filespan_minted_at_the_join,
      ( desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]),
                       natom(match, [var(source_file), region(re, none, "eprintln!")],
                             [at-var(at)]) ],
                     Rels, Goals),
        % No kernel rel carries a FileSpan column ...
        forall(( member(kernel_rel(_, Columns, _, _, _), Rels),
                 member(col(_, Type), Columns) ),
               Type \== 'FileSpan'),
        % ... yet the body binds one.
        memberchk(kbind(var(at), struct('FileSpan', _)), Goals) )).

check(every_extraction_exposes_at,
      forall( member(Region, [region(re, none, "eprintln!"),
                              region(sg, rust, "$RECEIVER.unwrap()"),
                              region(jsonpath, none, "paths.*.*.operationId"),
                              region(json, none, "{ image: $image }"),
                              region(ts, rust, "(identifier) @callee")]),
              ( legal_output_names(Region, Names), memberchk(at, Names) ) )).

check(omitted_output_is_wildcard,
      ( desugar_body([ atom(file, [region(glob, none, "*.go"), var(manifest)]),
                       natom(match, [var(manifest),
                                     region(re, none, "(?<module_path>\\S+)\\s+(?<version>v\\S*)")],
                             [version-var(version)]) ],
                     _Rels, Goals),
        memberchk(katom(extract(re, _), [_, _, _, _, wild, var(version)]), Goals) )).

% ---- static laws ------------------------------------------------------------

check(unknown_output_name_is_an_error,
      \+ desugar_goal(natom(match, [var(manifest),
                                    region(re, none, "(?<module_path>\\S+)")],
                            [module_pth-var(typo)]), [], 1, _, _) ).

check(at_is_a_reserved_capture_name,
      ( reserved_output(at),
        region_captures(region(re, none, "(?<at>\\S+)"), Captures),
        memberchk(at, Captures),
        % a capture that collides with the reserved name is refused
        \+ legal_output_names_distinct(region(re, none, "(?<at>\\S+)")) )).

check(same_pattern_text_one_kernel_rel,
      ( desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(left_file)]),
                       natom(match, [var(left_file), region(re, none, "eprintln!")],
                             [at-var(left_at)]) ],
                     LeftRels, _),
        desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(right_file)]),
                       natom(match, [var(right_file), region(re, none, "eprintln!")],
                             [at-var(right_at)]) ],
                     RightRels, _),
        LeftRels == RightRels )).

check(live_tree_salt_is_input_digest,
      ( desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]),
                       natom(match, [var(source_file), region(re, none, "eprintln!")],
                             [at-var(at)]) ],
                     Rels, _),
        memberchk(kernel_rel(extract(re, _), _, _, world, input_digest), Rels),
        memberchk(kernel_rel(enumerate(live, _), _, _, world, watch), Rels) )).

check(pinned_tree_enumeration_never_refires,
      ( desugar_body([ atom(tree_file, [var(rev), region(glob, none, "go.mod"), var(manifest)]) ],
                     Rels, _),
        memberchk(kernel_rel(enumerate(tree, _), _, _, world, none), Rels) )).

% ---- measured content addressing --------------------------------------------

check(two_files_one_digest_share_one_extract_row,
      ( eprintln_hit_solutions(Solutions),
        findall(Digest-Start,
                member(solution(_, Digest, Start, _), Solutions),
                Rows),
        sort(Rows, Distinct),
        % three files, two digests, so exactly two distinct kernel rows consumed
        length(Solutions, 3),
        length(Distinct, 2) )).

check(two_files_one_digest_get_distinct_spans,
      ( eprintln_hit_solutions(Solutions),
        findall(Span, member(solution(Span, _, _, _), Solutions), Spans),
        sort(Spans, DistinctSpans),
        length(DistinctSpans, 3) )).

check(changed_digest_demands_a_new_extract_row,
      ( eprintln_hit_solutions(Solutions),
        memberchk(solution(fspan(file(sprefa, 'src/a.rs', digest_one), 12, 21), _, _, _), Solutions),
        memberchk(solution(fspan(file(sprefa, 'src/c.rs', digest_two), 40, 49), _, _, _), Solutions) )).

% ---- the arity pathology ----------------------------------------------------

check(v5_scan_arity_defaults_are_ambiguous, silent_reinterpretation(v5_scan_shape)).
check(v5_comment_arity_defaults_are_ambiguous, silent_reinterpretation(v5_comment_shape)).
check(v6_selection_rels_have_no_arity_overlap, no_arity_overlap).

% ---- the transcriptions -----------------------------------------------------

check(eprintln_rail_constructs_all_budgeted,
      ( program(eprintln_rail, Program), all_constructs_budgeted(Program) )).

check(pin_skew_constructs_all_budgeted,
      ( program(pin_skew, Program), all_constructs_budgeted(Program) )).

check(flow_services_constructs_all_budgeted,
      ( program(flow_services, Program), all_constructs_budgeted(Program) )).

check(no_rev_or_world_literal_anywhere,
      forall( member(Name, [eprintln_rail, pin_skew, flow_services]),
              ( program(Name, Program), \+ contains_banned_literal(Program) ) )).

check(every_transcription_extraction_atom_desugars,
      forall( member(Name, [eprintln_rail, pin_skew, flow_services]),
              ( program(Name, Program),
                forall(member((_ <- Body), Program),
                       desugar_body(Body, _, _)) ) )).

check(every_span_output_is_a_filespan,
      forall( member(Name, [eprintln_rail, pin_skew, flow_services]),
              ( program(Name, Program),
                forall(( member(rel_decl(_, Columns, _), Program),
                         member(col(at, Type), Columns) ),
                       Type == 'FileSpan') ) )).

check(new_constructs_cost_zero_grammar,
      forall(proposed(_, Cost, _), Cost =:= 0)).

% Supporting predicates for the checks above.

legal_output_names_distinct(Region) :-
    region_captures(Region, Captures),
    forall(member(Name, Captures), \+ reserved_output(Name)).

eprintln_hit_solutions(Solutions) :-
    desugar_body([ atom(file, [region(glob, none, "src/**/*.rs"), var(source_file)]),
                   natom(match, [var(source_file), region(re, none, "eprintln!")],
                         [at-var(at)]) ],
                 _Rels, Goals),
    findall(solution(Span, Digest, Start, End),
            ( run_goals(Goals, [], Env),
              memberchk(at-Span, Env),
              memberchk(digest_2-Digest, Env),
              memberchk(start_2-Start, Env),
              memberchk(end_2-End, Env) ),
            Solutions).

go :- run(check).
