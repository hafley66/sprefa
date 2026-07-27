% shell_stream.pl : STREAMING shell effects. What happens to the effect model
% when one demand row is answered by MANY world rows plus a terminal event.
%
% Run:  swipl -q -l v6/prolog/labs/shell_stream.pl -g go -g halt
%
% LANG.md assumes an effect's world-fill is det: exactly one envelope per
% demand row, and the envelope enum's Error arm is what makes it det rather
% than "semidet plus a throw". A `sprefa-extract` process breaks that: it
% emits one JSON object per line for as long as it runs, then exits with a
% code. That is many rows per request plus one terminal.
%
% SURFACE CHOSEN (weighed against the three candidates in shell_stream.md):
%
%   enum ExtractEvent { Line { obj: Json } }
%   enum ExtractEnd   { Done { code: Int }, Err { msg: Str } }
%   rel  extract(args: Str, salt: Digest) -> Stream(ExtractEvent, ExtractEnd);
%
% `Stream(Item, End)` is a wrapper in the arrow's result position, the same
% move `Key(Type)` makes in a column's type position. Two envelopes, not one:
% the item envelope and the terminal envelope. The wrapper is what carries the
% mode, so mode analysis reads it off the signature with no link step:
%
%   -> FetchResult          (det,   finite)   today's effects
%   -> Stream(Item, End)    (multi, finite)   this lab: terminal guaranteed
%   -> Tail(Item)           (multi, never)    no terminal type, never completes
%
% The bind still owns the transport AND the exit-code-to-constructor match,
% exactly as ghcacher's `match status { 200 => Fresh, 304 => Unchanged,
% s => Error{status:s} }` already does:
%
%   bind extract = shell {
%     `sprefa-extract {args}`
%     -> stdout_line(text) => Line { obj: json(text) }
%      , exit(code)        => match code { 0 => Done { code }, c => Err { msg } }
%   };
%
% Encoding here: surface declarations are facts, the tick engine is a fold of
% arrival events over a state term, and the downstream rules are `<+` edge
% rules (append-only). No register is used anywhere in this lab, which is the
% tier-order finding: streaming effects need {ground_terms, rule, external_rel}
% and can land before register_lowering.

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module('../src/kernel.pl').

% ═══ 1. the surface declarations, as facts ═══════════════════════════════════

% item envelope: what arrives repeatedly
enum(extract_event, [line(obj)]).

% terminal envelope: what arrives exactly once, and ends the stream
enum(extract_end,   [done(code), err(msg)]).

% the single-enum alternative (candidate (a) as posed). Kept only so a check
% can show what it costs at every match site.
enum(extract_event_flat, [line(obj), done(code), err(msg)]).

% det twins from ghcacher, for the mode table
enum(fetch_result, [fresh(tag, body), unchanged, error(status)]).
enum(log_event,    [line(text)]).

% effect_rel(Name, ProgramBoundColumns, ResultType)
effect_rel(extract,  [args, salt],     stream(extract_event, extract_end)).
effect_rel(fetch,    [endpoint, prev], fetch_result).
effect_rel(log_tail, [path],           tail(log_event)).

% the program's match arms over each envelope. These are the `<+` edge rules:
%
%   extracted(id, seq, path, kind, name) <+ extract(args, salt) -> Line { obj };
%   extraction_complete(id, count, code) <+ extract(args, salt) -> Done { code };
%   extraction_failed(id, msg)           <+ extract(args, salt) -> Err  { msg };
%
item_arms([ arm(line(Text), emit_row(Text)) ]).
end_arms( [ arm(done(Code), complete(Code))
          , arm(err(Msg),   failed(Msg)) ]).

% a program that forgot the Err arm; used by one check only
broken_end_arms([ arm(done(Code), complete(Code)) ]).

% the bind's exit-code match, the only place a transport detail lives
bind_exit(0, done(0)) :- !.
bind_exit(Code, err(Message)) :-
    format(string(Message), "sprefa-extract exited ~w", [Code]).

% ═══ 2. mode analysis: read (cardinality, lifetime) off the result type ══════

result_mode(stream(_, _), multi, finite).
result_mode(tail(_),      multi, never).
result_mode(EnumName,     det,   finite) :- atom(EnumName), enum(EnumName, _).

effect_mode(Rel, Cardinality, Lifetime) :-
    effect_rel(Rel, _, ResultType),
    result_mode(ResultType, Cardinality, Lifetime).

% ═══ 3. exhaustiveness over the SPLIT envelopes ══════════════════════════════

covers(EnumName, Arms) :-
    enum(EnumName, Constructors),
    forall(member(Constructor, Constructors),
           ( functor(Constructor, Name, Arity),
             member(arm(Pattern, _), Arms),
             functor(Pattern, Name, Arity) )).

% ═══ 4. JSON, one DCG (copied from books/v6/algos/json.pl, not imported) ═════

json_parse(Text, Value) :-
    ( string(Text) -> string_codes(Text, Codes) ; atom_codes(Text, Codes) ),
    phrase((ws, jvalue(Value), ws), Codes).

jvalue(null)  --> "null", !.
jvalue(true)  --> "true", !.
jvalue(false) --> "false", !.
jvalue(String) --> jstring(String), !.
jvalue(Number) --> jnumber(Number), !.
jvalue(List)  --> "[", !, ws, elements(List), ws, "]".
jvalue(obj(Pairs)) --> "{", ws, members(Pairs), ws, "}".

elements([Value | Rest]) --> jvalue(Value), ws, ( ",", !, ws, elements1(Rest) ; { Rest = [] } ).
elements([]) --> [].
elements1([Value | Rest]) --> jvalue(Value), ws, ( ",", !, ws, elements1(Rest) ; { Rest = [] } ).

members([Key-Value | Rest]) -->
    jstring(Key), ws, ":", ws, jvalue(Value), ws,
    ( ",", !, ws, members1(Rest) ; { Rest = [] } ).
members([]) --> [].
members1([Key-Value | Rest]) -->
    jstring(Key), ws, ":", ws, jvalue(Value), ws,
    ( ",", !, ws, members1(Rest) ; { Rest = [] } ).

jstring(String) --> "\"", jchars(Chars), "\"", { string_codes(String, Chars) }.
jchars([Char | Chars]) --> jchar(Char), !, jchars(Chars).
jchars([]) --> [].
jchar(Char) --> "\\", !, escape(Char).
jchar(Char) --> [Char], { Char =\= 0'\", Char =\= 0'\\ }.
escape(0'\") --> "\"".    escape(0'\\) --> "\\".    escape(0'/) --> "/".
escape(8)  --> "b".       escape(12) --> "f".       escape(10) --> "n".
escape(13) --> "r".       escape(9)  --> "t".
escape(Char) --> "u", [Hex1, Hex2, Hex3, Hex4],
    { foldl(hexacc, [Hex1, Hex2, Hex3, Hex4], 0, Char) }.
hexacc(Code, Acc, Out) :- code_type(Code, xdigit(Value)), Out is Acc * 16 + Value.

jnumber(Number) -->
    minus(Sign), digits1(IntCodes), frac(FracCodes0), expo(ExpCodes),
    { ( ExpCodes \== [], FracCodes0 == []       % 1e3 -> 1.0e3
      -> FracCodes = [46, 48] ; FracCodes = FracCodes0 ),
      append([Sign, IntCodes, FracCodes, ExpCodes], Codes),
      number_codes(Number, Codes) }.
minus([45]) --> "-", !.
minus([]) --> [].
digits1([Digit | Digits]) --> [Digit], { code_type(Digit, digit) }, digits0(Digits).
digits0([Digit | Digits]) --> [Digit], { code_type(Digit, digit) }, !, digits0(Digits).
digits0([]) --> [].
frac([46 | Digits]) --> ".", !, digits1(Digits).
frac([]) --> [].
expo([101 | Rest]) --> ( "e" ; "E" ), !, esign(Sign), digits1(Digits),
    { append(Sign, Digits, Rest) }.
expo([]) --> [].
esign([43]) --> "+", !.
esign([45]) --> "-", !.
esign([]) --> [].

ws --> [Char], { memberchk(Char, [32, 9, 10, 13]) }, !, ws.
ws --> [].

json_field(obj(Pairs), Key, Value) :- memberchk(Key-Value, Pairs).

% ═══ 5. the tick engine ══════════════════════════════════════════════════════
%
% state(NextId, Interned, Streams, RowsReversed, FactsReversed, NotesReversed)
%
%   Interned : Request-Id pairs. Content-addressed request identity: the same
%              ground demand term interns to the same id forever, which is what
%              makes a repeated demand row dedup instead of respawning.
%   Streams  : Id-Status, Status = open(SeqSoFar) | finished(TerminalEvent).
%              This is the per-request LIFETIME, on disk, as a row.
%   Rows     : the `<+` append target, in arrival order.
%   Facts    : once-facts derived by the terminal arms.
%   Notes    : rejections the runtime records rather than silently dropping.

empty_state(state(1, [], [], [], [], [])).

intern(Request, state(NextId, Interned, Streams, Rows, Facts, Notes), Id, StateOut, Fresh) :-
    (   memberchk(Request-Existing, Interned)
    ->  Id = Existing, Fresh = false,
        StateOut = state(NextId, Interned, Streams, Rows, Facts, Notes)
    ;   Id = NextId, Fresh = true, NextIdOut is NextId + 1,
        StateOut = state(NextIdOut, [Request-Id | Interned], Streams, Rows, Facts, Notes)
    ).

lookup_id(Request, state(_, Interned, _, _, _, _), Id) :- memberchk(Request-Id, Interned).

stream_status(Id, state(_, _, Streams, _, _, _), Status) :- memberchk(Id-Status, Streams).

open_stream(Id, state(NextId, Interned, Streams, Rows, Facts, Notes),
                state(NextId, Interned, [Id-open(0) | Streams], Rows, Facts, Notes)).

set_status(Id, Status, state(NextId, Interned, Streams0, Rows, Facts, Notes),
                       state(NextId, Interned, Streams,  Rows, Facts, Notes)) :-
    selectchk(Id-_, Streams0, Id-Status, Streams).

add_row(Id, Tick, Path, Kind, Name,
        state(NextId, Interned, Streams0, Rows,         Facts, Notes),
        state(NextId, Interned, Streams,  [Row | Rows], Facts, Notes)) :-
    selectchk(Id-open(SeqBefore), Streams0, Id-open(Seq), Streams),
    Seq is SeqBefore + 1,
    Row = row(Id, Seq, Tick, Path, Kind, Name).

add_fact(Fact, state(NextId, Interned, Streams, Rows, Facts,          Notes),
               state(NextId, Interned, Streams, Rows, [Fact | Facts], Notes)).
add_note(Note, state(NextId, Interned, Streams, Rows, Facts, Notes),
               state(NextId, Interned, Streams, Rows, Facts, [Note | Notes])).

rows_of(state(_, _, _, RowsReversed, _, _), Rows) :- reverse(RowsReversed, Rows).
facts_of(state(_, _, _, _, FactsReversed, _), Facts) :- reverse(FactsReversed, Facts).
notes_of(state(_, _, _, _, _, NotesReversed), Notes) :- reverse(NotesReversed, Notes).
streams_of(state(_, _, Streams, _, _, _), Streams).

rows_for(State, Id, Rows) :- rows_of(State, All), include(row_of_id(Id), All, Rows).
row_of_id(Id, row(Id, _, _, _, _, _)).

% ── one tick = one batch of events; the fold IS the transaction ──────────────

run_transcript(Ticks, StateOut) :-
    empty_state(State0),
    run_ticks(Ticks, 1, State0, StateOut).

run_ticks([], _, State, State).
run_ticks([Events | Rest], Tick, State0, StateOut) :-
    foldl(step(Tick), Events, State0, State1),
    NextTick is Tick + 1,
    run_ticks(Rest, NextTick, State1, StateOut).

% a demand row appears. Fresh request = spawn; seen request = dedup, no spawn.
step(Tick, demand(Request), State0, StateOut) :-
    intern(Request, State0, Id, State1, Fresh),
    (   Fresh == true
    ->  open_stream(Id, State1, StateOut)
    ;   add_note(deduped(Id, Tick, Request), State1, StateOut)
    ).

% the world fills. Only an open stream accepts events: the terminal is final.
step(Tick, arrive(Request, Event), State0, StateOut) :-
    (   lookup_id(Request, State0, Id),
        stream_status(Id, State0, Status)
    ->  (   Status = open(_)
        ->  dispatch(Tick, Id, Event, State0, StateOut)
        ;   add_note(after_terminal(Id, Tick, Event), State0, StateOut)
        )
    ;   add_note(arrival_without_demand(Tick, Request, Event), State0, StateOut)
    ).

dispatch(Tick, Id, Event, State0, StateOut) :-
    (   item_arms(ItemArms), memberchk(arm(Event, ItemAction), ItemArms)
    ->  run_item_action(ItemAction, Tick, Id, State0, StateOut)
    ;   end_arms(EndArms), memberchk(arm(Event, EndAction), EndArms)
    ->  run_end_action(EndAction, Tick, Id, State0, StateOut)
    ;   add_note(unmatched_event(Id, Tick, Event), State0, StateOut)
    ).

run_item_action(emit_row(Text), Tick, Id, State0, StateOut) :-
    (   json_parse(Text, Object),
        json_field(Object, "path", Path),
        json_field(Object, "kind", Kind),
        json_field(Object, "name", Name)
    ->  add_row(Id, Tick, Path, Kind, Name, State0, StateOut)
    ;   add_note(bad_line(Id, Tick, Text), State0, StateOut)
    ).

run_end_action(complete(Code), _Tick, Id, State0, StateOut) :-
    rows_for(State0, Id, Rows), length(Rows, Count),
    add_fact(extraction_complete(Id, Count, Code), State0, State1),
    set_status(Id, finished(done(Code)), State1, StateOut).

run_end_action(failed(Message), _Tick, Id, State0, StateOut) :-
    add_fact(extraction_failed(Id, Message), State0, State1),
    set_status(Id, finished(err(Message)), State1, StateOut).

% ═══ 6. canned transcripts ═══════════════════════════════════════════════════

jsonl(alpha, "{\"path\":\"a.ts\",\"kind\":\"fn\",\"name\":\"alpha\"}").
jsonl(beta,  "{\"path\":\"a.ts\",\"kind\":\"fn\",\"name\":\"beta\"}").
jsonl(gamma, "{\"path\":\"b.ts\",\"kind\":\"class\",\"name\":\"Gamma\"}").

% args plus a salt column. For an extractor the honest salt is the INPUT
% DIGEST, not an arrival tick: "the tree changed" is the reason to re-extract,
% "time passed" is not.
happy_request(extract("src/", "sha:aaa")).
resalt_request(extract("src/", "sha:bbb")).

transcript(happy, Ticks) :-
    happy_request(Request),
    jsonl(alpha, AlphaLine), jsonl(beta, BetaLine), jsonl(gamma, GammaLine),
    Ticks = [ [ demand(Request) ]
            , [ arrive(Request, line(AlphaLine)), arrive(Request, line(BetaLine)) ]
            , [ arrive(Request, line(GammaLine)) ]
            , [ arrive(Request, done(0)) ]
            ].

% a Line that shows up after the terminal
transcript(late_line, Ticks) :-
    transcript(happy, Happy), happy_request(Request), jsonl(alpha, AlphaLine),
    append(Happy, [[ arrive(Request, line(AlphaLine)) ]], Ticks).

% the extractor dies two lines in
transcript(err_mid_stream, Ticks) :-
    happy_request(Request), jsonl(alpha, AlphaLine), jsonl(beta, BetaLine),
    Ticks = [ [ demand(Request) ]
            , [ arrive(Request, line(AlphaLine)) ]
            , [ arrive(Request, line(BetaLine)) ]
            , [ arrive(Request, err("extractor panicked at b.ts:12")) ]
            ].

% an identical demand row after completion
transcript(dedup, Ticks) :-
    transcript(happy, Happy), happy_request(Request),
    append(Happy, [[ demand(Request) ]], Ticks).

% same args, new input digest: a genuinely different request
transcript(resalt, Ticks) :-
    transcript(happy, Happy), resalt_request(Request), jsonl(alpha, AlphaLine),
    append(Happy, [ [ demand(Request) ]
                  , [ arrive(Request, line(AlphaLine)) ]
                  , [ arrive(Request, done(0)) ] ], Ticks).

% ═══ 7. a real pipe: one process_create, the same fold ═══════════════════════

live_script(happy,
  'printf \'{"path":"a.ts","kind":"fn","name":"alpha"}\\n{"path":"a.ts","kind":"fn","name":"beta"}\\n{"path":"b.ts","kind":"class","name":"Gamma"}\\n\'').
live_script(dies,
  'printf \'{"path":"a.ts","kind":"fn","name":"alpha"}\\n{"path":"a.ts","kind":"fn","name":"beta"}\\n\'; exit 3').

live_lines(Which, Lines, ExitCode) :-
    live_script(Which, Script),
    process_create(path(sh), ['-c', Script],
                   [ stdout(pipe(Output)), process(Pid) ]),
    read_lines(Output, Lines),
    close(Output),
    process_wait(Pid, exit(ExitCode)).

read_lines(Stream, Lines) :-
    read_line_to_string(Stream, Line),
    (   Line == end_of_file
    ->  Lines = []
    ;   Line == ""
    ->  read_lines(Stream, Lines)
    ;   Lines = [Line | Rest], read_lines(Stream, Rest)
    ).

% one line per tick, so the live run also proves arrival ACROSS ticks
lines_transcript(Request, Lines, EndEvent, Ticks) :-
    maplist(one_line_tick(Request), Lines, LineTicks),
    append([[ demand(Request) ] | LineTicks], [[ arrive(Request, EndEvent) ]], Ticks).

one_line_tick(Request, Line, [ arrive(Request, line(Line)) ]).

live_state(Which, State) :-
    live_lines(Which, Lines, ExitCode),
    bind_exit(ExitCode, EndEvent),
    happy_request(Request),
    lines_transcript(Request, Lines, EndEvent, Ticks),
    run_transcript(Ticks, State).

% ═══ 8. the same fold over a fixture file on disk ════════════════════════════

fixture_path(Path) :-
    source_file(fixture_path(_), Source),
    file_directory_name(Source, Directory),
    directory_file_path(Directory, 'shell_stream_fixture.jsonl', Path).

fixture_state(State) :-
    fixture_path(Path),
    setup_call_cleanup(open(Path, read, Stream),
                       read_lines(Stream, Lines),
                       close(Stream)),
    happy_request(Request),
    lines_transcript(Request, Lines, done(0), Ticks),
    run_transcript(Ticks, State).

% ═══ 9. shape comparison (ids and ticks differ across sources) ═══════════════

shape(row(_, Seq, _, Path, Kind, Name), seq(Seq, Path, Kind, Name)).
shapes(State, Shapes) :- rows_of(State, Rows), maplist(shape, Rows, Shapes).

expected_shapes([ seq(1, "a.ts", "fn",    "alpha")
                , seq(2, "a.ts", "fn",    "beta")
                , seq(3, "b.ts", "class", "Gamma") ]).

% ═══ 10. the kernel grounding claim for this feature ═════════════════════════
% Declared locally rather than in src/kernel.pl because this lab does not own
% that file. `register` is deliberately absent from the parts list.

lab_sugar(stream_effect,     [external_rel, rule, ground_terms]).
lab_sugar(stream_terminal,   [external_rel, rule, ground_terms]).
lab_sugar(content_addressed_request, [ground_terms, rule]).

% ═══ checks ══════════════════════════════════════════════════════════════════

% --- surface form and modes ---

check(mode_read_off_result_type,
      ( effect_mode(extract,  multi, finite),
        effect_mode(fetch,    det,   finite),
        effect_mode(log_tail, multi, never) )).

check(mode_is_functional,
      ( findall(Card-Life, result_mode(stream(extract_event, extract_end), Card, Life), Modes),
        Modes == [multi-finite] )).

check(multi_finite_needs_terminal_enum,
      ( forall(( effect_rel(_, _, Result), result_mode(Result, multi, finite) ),
               ( Result = stream(_, EndEnum), enum(EndEnum, _) )),
        forall(( effect_rel(_, _, Result2), result_mode(Result2, multi, never) ),
               Result2 = tail(_)) )).

check(split_envelope_exhaustive,
      ( item_arms(ItemArms), covers(extract_event, ItemArms),
        end_arms(EndArms),   covers(extract_end,   EndArms) )).

check(flat_envelope_costs_dead_arms,
      ( item_arms(ItemArms), \+ covers(extract_event_flat, ItemArms) )).

check(missing_terminal_arm_rejected,
      ( broken_end_arms(Broken), \+ covers(extract_end, Broken) )).

check(stream_effect_grounds_in_kernel,
      ( forall(lab_sugar(_, Parts),
               forall(member(Part, Parts), kernel(Part))),
        forall(lab_sugar(_, Parts2), \+ memberchk(register, Parts2)) )).

% --- json sanity, since the DCG is copied text ---

check(jsonl_line_parses,
      ( jsonl(gamma, Text), json_parse(Text, Object),
        json_field(Object, "path", "b.ts"),
        json_field(Object, "kind", "class"),
        json_field(Object, "name", "Gamma") )).

% --- (a) all lines land as rows in arrival order ---

check(lines_land_in_arrival_order,
      ( transcript(happy, Ticks), run_transcript(Ticks, State),
        rows_of(State, Rows),
        Rows == [ row(1, 1, 2, "a.ts", "fn",    "alpha")
                , row(1, 2, 2, "a.ts", "fn",    "beta")
                , row(1, 3, 3, "b.ts", "class", "Gamma") ] )).

% --- (b) Done flips lifetime to finished; completion derives exactly once ---

check(done_finishes_lifetime,
      ( transcript(happy, Ticks), run_transcript(Ticks, State),
        streams_of(State, Streams),
        Streams == [1-finished(done(0))] )).

check(complete_derives_exactly_once,
      ( transcript(happy, Ticks), run_transcript(Ticks, State),
        facts_of(State, Facts),
        Facts == [extraction_complete(1, 3, 0)] )).

check(terminal_is_terminal,
      ( transcript(late_line, Ticks), run_transcript(Ticks, State),
        rows_of(State, Rows), length(Rows, 3),
        notes_of(State, Notes),
        Notes == [after_terminal(1, 5, line("{\"path\":\"a.ts\",\"kind\":\"fn\",\"name\":\"alpha\"}"))],
        facts_of(State, Facts), length(Facts, 1) )).

% --- (c) Err mid-stream keeps prior rows and derives a failure fact ---

check(err_keeps_prior_rows,
      ( transcript(err_mid_stream, Ticks), run_transcript(Ticks, State),
        shapes(State, Shapes),
        Shapes == [ seq(1, "a.ts", "fn", "alpha"), seq(2, "a.ts", "fn", "beta") ],
        facts_of(State, Facts),
        Facts == [extraction_failed(1, "extractor panicked at b.ts:12")],
        streams_of(State, [1-finished(err(_))]) )).

check(err_derives_no_completion,
      ( transcript(err_mid_stream, Ticks), run_transcript(Ticks, State),
        facts_of(State, Facts),
        \+ memberchk(extraction_complete(_, _, _), Facts) )).

% --- (d) identity: dedup by content, re-fire only on a new salt ---

check(identical_demand_dedups,
      ( transcript(dedup, Ticks), run_transcript(Ticks, State),
        streams_of(State, Streams), length(Streams, 1),
        rows_of(State, Rows), length(Rows, 3),
        notes_of(State, Notes),
        Notes == [deduped(1, 5, extract("src/", "sha:aaa"))] )).

check(new_salt_refires_fresh_stream,
      ( transcript(resalt, Ticks), run_transcript(Ticks, State),
        streams_of(State, Streams), msort(Streams, Sorted),
        Sorted == [1-finished(done(0)), 2-finished(done(0))],
        rows_for(State, 2, SecondRows), length(SecondRows, 1),
        facts_of(State, Facts),
        Facts == [extraction_complete(1, 3, 0), extraction_complete(2, 1, 0)] )).

% --- live process: the same fold over a real pipe ---

check(live_pipe_matches_canned,
      ( live_state(happy, State),
        shapes(State, Shapes), expected_shapes(Shapes),
        facts_of(State, [extraction_complete(1, 3, 0)]),
        streams_of(State, [1-finished(done(0))]) )).

check(live_pipe_spans_ticks,
      ( live_state(happy, State),
        rows_of(State, [ row(_, 1, 2, _, _, _)
                       , row(_, 2, 3, _, _, _)
                       , row(_, 3, 4, _, _, _) ]) )).

check(live_nonzero_exit_keeps_rows,
      ( live_state(dies, State),
        shapes(State, Shapes),
        Shapes == [ seq(1, "a.ts", "fn", "alpha"), seq(2, "a.ts", "fn", "beta") ],
        facts_of(State, [extraction_failed(1, "sprefa-extract exited 3")]),
        streams_of(State, [1-finished(err(_))]) )).

% --- fixture file on disk, same fold ---

check(fixture_file_matches_canned,
      ( fixture_state(State),
        shapes(State, Shapes), expected_shapes(Shapes),
        facts_of(State, [extraction_complete(1, 3, 0)]) )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
