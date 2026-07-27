% dl_to_ts.pl : a datalog with yield points, lowered to DIRECT TypeScript text.
%
% Run:     swipl -q -l books/v6/dl_to_ts.pl
% Score:   ?- go.          (generates books/v6/gen/*.ts, runs them under node,
%                           compares stdout against the expected transcript)
% Read:    books/v6/gen/counter.ts, books/v6/gen/reach.ts
%
% The three arrows are the Bloom parking spots, one per rule kind:
%   Head <= Body      deduce: same tick, joins fused into the fixpoint loop
%   Head <+ Body      @next:  park a delta for MY next tick (the mailbox)
%   Head <~ Body      @async: hand a delta to the host (effects)
% <+ and <~ compile IDENTICALLY except for which array receives the push:
% one boundary kind, delivery annotation only.
%
% No TS AST anywhere. The compiler walks rule terms; shared holes become
% `if (x !== y) continue;` guards exactly where rule_select (dl_in_prolog.pl)
% made them SQL join conditions. Same analyzer, third backend.

:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module(library(apply)).
:- use_module(library(filesex)).

:- op(1150, xfx, <=).       % prolog's less-equal is =<, so <= is free
:- op(1150, xfx, <+).
:- op(1150, xfx, <~).

% ── example programs ────────────────────────────────────────────────────────

% counter: seed arrives on tick 0, carry re-enters through the mailbox,
% doubled is derived each tick, log leaves through the host boundary.
program(counter,
  [ ( total(N)   <= seed(N) )
  , ( total(N)   <= carry(N) )
  , ( doubled(D) <= total(N), D is N * 2 )
  , ( carry(N1)  <+ total(N), tick_event(_), N1 is N + 1 )
  , ( log(D)     <~ doubled(D) )
  ]).

% reach: recursion inside one tick's fixpoint, result out through <~.
program(reach,
  [ ( reach(Node)  <= root(Node) )
  , ( reach(Child) <= edge(Parent, Child), reach(Parent) )
  , ( found(Node)  <~ reach(Node) )
  ]).

% ── drivers + expected transcripts (the oracle for `go`) ────────────────────

driver(counter,
  [ 'let mailbox: Fact[] = [["seed", 0], ["tick_event", 0]];'
  , 'for (let t = 0; t < 3; t++) {'
  , '  const { next, effects } = tick(mailbox);'
  , '  console.log(JSON.stringify(effects));'
  , '  mailbox = [...next, ["tick_event", t + 1]];'
  , '}'
  ]).
driver(reach,
  [ 'const { effects } = tick([["root", 1], ["edge", 1, 2], ["edge", 2, 3], ["edge", 7, 8]]);'
  , 'console.log(JSON.stringify(effects));'
  ]).

expected(counter,
  [ "[[\"log\",0]]"
  , "[[\"log\",2]]"
  , "[[\"log\",4]]"
  ]).
expected(reach,
  [ "[[\"found\",1],[\"found\",2],[\"found\",3]]"
  ]).

% ── rule kinds ──────────────────────────────────────────────────────────────

rule_kind((Head <= Body), deduce, Head, Body).
rule_kind((Head <+ Body), next,   Head, Body).
rule_kind((Head <~ Body), async,  Head, Body).

body_list((Item, Rest), [Item | Items]) :- !, body_list(Rest, Items).
body_list(Item, [Item]).

% ── the compiler state: hole -> TS name bindings, plus a name counter ───────

alloc(Var, st(Binds, I), st([Var-Name | Binds], I1), Name) :-
    ts_name(I, Name), I1 is I + 1.

alloc_aux(st(Binds, I), st(Binds, I1), Name) :-        % named, not bound
    ts_name(I, Name), I1 is I + 1.

ts_name(I, Name) :-
    Letters = [a,b,c,d,e,f,g,h,i,j,k,l,m],
    ( nth0(I, Letters, Name) -> true ; format(atom(Name), 'v~w', [I]) ).

vfind(Var, [Key-Name | _], Name) :- Var == Key, !.
vfind(Var, [_ | Rest], Name) :- vfind(Var, Rest, Name).

% ── expressions and comparisons ─────────────────────────────────────────────

expr_ts(Var, st(Binds, _), Name) :- var(Var), !, vfind(Var, Binds, Name).
expr_ts(N, _, N) :- number(N), !.
expr_ts(A + B, S, T) :- !, expr2(A, B, S, '+', T).
expr_ts(A - B, S, T) :- !, expr2(A, B, S, '-', T).
expr_ts(A * B, S, T) :- !, expr2(A, B, S, '*', T).
expr_ts(A mod B, S, T) :- !, expr2(A, B, S, '%', T).
expr_ts(Atom, _, T) :- atom(Atom), format(atom(T), '"~w"', [Atom]).

expr2(A, B, S, Op, T) :-
    expr_ts(A, S, TA), expr_ts(B, S, TB),
    format(atom(T), '(~w ~w ~w)', [TA, Op, TB]).

cmp_op(A > B,   A, B, '>').
cmp_op(A < B,   A, B, '<').
cmp_op(A >= B,  A, B, '>=').
cmp_op(A =< B,  A, B, '<=').
cmp_op(A =:= B, A, B, '===').
cmp_op(A =\= B, A, B, '!==').

% ── body compilation: rel atoms open loops, everything else is a line ───────

compile_items([], S, S, Ind, Ind, [], []).
compile_items([Item | Rest], S0, S, Ind, FinalInd, Lines, Closes) :-
    (   Item = (Var is Expr)
    ->  expr_ts(Expr, S0, ExprTs),
        alloc(Var, S0, S1, Name),
        iline(Ind, 'const ~w = ~w;', [Name, ExprTs], Line),
        compile_items(Rest, S1, S, Ind, FinalInd, RestLines, Closes),
        Lines = [Line | RestLines]
    ;   cmp_op(Item, A, B, JsOp)
    ->  expr_ts(A, S0, TA), expr_ts(B, S0, TB),
        iline(Ind, 'if (!(~w ~w ~w)) continue;', [TA, JsOp, TB], Line),
        compile_items(Rest, S0, S, Ind, FinalInd, RestLines, Closes),
        Lines = [Line | RestLines]
    ;   Item =.. [Rel | Args],
        dargs(Args, S0, S1, Pats, Guards),
        atomic_list_concat(Pats, ', ', PatList),
        iline(Ind, 'for (const [~w] of facts.get("~w") ?? []) {', [PatList, Rel], LoopLine),
        Ind1 is Ind + 1,
        maplist(guard_line(Ind1), Guards, GuardLines),
        compile_items(Rest, S1, S, Ind1, FinalInd, RestLines, RestCloses),
        iline(Ind, '}', [], CloseLine),
        append([LoopLine | GuardLines], RestLines, Lines),
        append(RestCloses, [CloseLine], Closes)
    ).

guard_line(Ind, Guard, Line) :- iline(Ind, '~w', [Guard], Line).

% Shared hole: second occurrence destructures to a fresh name and guards
% equality. This is rule_select's join condition, retargeted.
dargs([], S, S, [], []).
dargs([Arg | Rest], S0, S, [Pat | Pats], Guards) :-
    (   var(Arg), S0 = st(Binds, _), vfind(Arg, Binds, Existing)
    ->  alloc_aux(S0, S1, Pat),
        format(atom(Guard), 'if (~w !== ~w) continue;', [Pat, Existing]),
        Guards = [Guard | MoreGuards]
    ;   var(Arg)
    ->  alloc(Arg, S0, S1, Pat), Guards = MoreGuards
    ;   number(Arg)
    ->  alloc_aux(S0, S1, Pat),
        format(atom(Guard), 'if (~w !== ~w) continue;', [Pat, Arg]),
        Guards = [Guard | MoreGuards]
    ;   alloc_aux(S0, S1, Pat),
        format(atom(Guard), 'if (~w !== "~w") continue;', [Pat, Arg]),
        Guards = [Guard | MoreGuards]
    ),
    dargs(Rest, S1, S, Pats, MoreGuards).

% ── heads: the only place the three arrows differ ───────────────────────────

compile_head(Kind, Head, S, Ind, Line) :-
    Head =.. [Rel | Args],
    maplist(head_arg(S), Args, ArgTs),
    atomic_list_concat(ArgTs, ', ', ArgList),
    (   Kind == deduce
    ->  iline(Ind, 'if (add("~w", [~w])) changed = true;', [Rel, ArgList], Line)
    ;   Kind == next
    ->  iline(Ind, 'next.push(["~w", ~w]);', [Rel, ArgList], Line)
    ;   iline(Ind, 'effects.push(["~w", ~w]);', [Rel, ArgList], Line)
    ).

head_arg(S, Arg, Ts) :- expr_ts(Arg, S, Ts).    % fails on an unbound head
                                                % hole: safety check for free

compile_rule(Rule, BaseInd, [Comment | Lines]) :-
    rule_kind(Rule, Kind, Head0, Body0),
    comment_line(Rule, BaseInd, Comment),
    copy_term(Head0-Body0, Head-Body),
    body_list(Body, Items),
    compile_items(Items, st([], 0), S, BaseInd, FinalInd, BodyLines, Closes),
    compile_head(Kind, Head, S, FinalInd, HeadLine),
    append(BodyLines, [HeadLine | Closes], Lines).

comment_line(Rule, Ind, Line) :-
    copy_term(Rule, Copy), numbervars(Copy, 0, _),
    with_output_to(atom(Text), write_term(Copy, [numbervars(true)])),
    iline(Ind, '// ~w', [Text], Line).

rules_lines(Rules, Kind, Ind, Lines) :-
    findall(RuleLines,
            ( member(Rule, Rules), rule_kind(Rule, Kind, _, _),
              compile_rule(Rule, Ind, RuleLines) ),
            PerRule),
    append(PerRule, Lines).

% ── whole-file assembly ─────────────────────────────────────────────────────

gen(Name) :-
    program(Name, Rules),
    rules_lines(Rules, deduce, 2, DeduceLines),
    rules_lines(Rules, next,   1, NextLines),
    rules_lines(Rules, async,  1, AsyncLines),
    driver(Name, DriverLines),
    format(atom(Header), '// GENERATED by books/v6/dl_to_ts.pl : program ~w. Do not edit.', [Name]),
    Prelude =
      [ Header
      , 'type Fact = [string, ...any[]];'
      , ''
      , 'function tick(mailbox: Fact[]): { next: Fact[]; effects: Fact[] } {'
      , '  const facts = new Map<string, any[][]>();'
      , '  const seen = new Set<string>();'
      , '  const add = (rel: string, row: any[]): boolean => {'
      , '    const key = rel + JSON.stringify(row);'
      , '    if (seen.has(key)) return false;'
      , '    if (!facts.has(rel)) facts.set(rel, []);'
      , '    facts.get(rel)!.push(row);'
      , '    seen.add(key);'
      , '    return true;'
      , '  };'
      , '  for (const [rel, ...args] of mailbox) add(rel, args);'
      , ''
      , '  let changed = true;'
      , '  while (changed) {'
      , '    changed = false;'
      ],
    Mid =
      [ '  }'
      , ''
      , '  const next: Fact[] = [];'
      ],
    PreAsync =
      [ ''
      , '  const effects: Fact[] = [];'
      ],
    Post =
      [ '  return { next, effects };'
      , '}'
      , ''
      ],
    append([Prelude, DeduceLines, Mid, NextLines, PreAsync, AsyncLines, Post,
            DriverLines], AllLines),
    atomic_list_concat(AllLines, '\n', Text),
    ts_path(Name, Path),
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, '~w~n', [Text]),
                       close(Stream)).

ts_path(Name, Path) :-
    make_directory_path('books/v6/gen'),
    format(atom(Path), 'books/v6/gen/~w.ts', [Name]).

iline(Ind, Fmt, Args, Line) :-
    Spaces is Ind * 2,
    format(atom(Body), Fmt, Args),
    ( Spaces =:= 0 -> Line = Body
    ; format(atom(Line), '~*c~w', [Spaces, 32, Body]) ).

% ── run under node (24+ strips types natively), compare transcripts ─────────

run_ts(Name, Lines) :-
    ts_path(Name, Path),
    process_create(path(node), [Path],
                   [stdout(pipe(Out)), stderr(null)]),
    read_lines_from(Out, Lines),
    close(Out).

read_lines_from(Stream, Lines) :-
    read_line_to_string(Stream, Line),
    (   Line == end_of_file -> Lines = []
    ;   Lines = [Line | Rest], read_lines_from(Stream, Rest)
    ).

check(counter_ts, ( gen(counter), run_ts(counter, Lines),
                    expected(counter, Lines) )).
check(reach_ts,   ( gen(reach), run_ts(reach, Lines),
                    expected(reach, Lines) )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
