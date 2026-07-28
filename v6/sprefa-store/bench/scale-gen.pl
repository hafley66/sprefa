% scale-gen.pl : synthetic tsv2 scale fixtures.
%
% The output is a normal fixture(Name, prog(...), Initial, Schedule,
% Expectations) term. v6/prolog/compile/compile.pl reads it through the same
% compile_fixture/4 entry point used by the conformance programs.

:- module(scale_gen, [write_fixture/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

write_fixture(Shape, Rows, Path) :-
    memberchk(Shape, [s1, s2, s3]),
    integer(Rows),
    Rows >= 100,
    0 is Rows mod 100,
    fixture_term(Shape, Rows, Fixture, Bindings),
    setup_call_cleanup(
        open(Path, write, Stream),
        ( format(Stream, ':- op(1150, xfx, <-).~n', []),
          format(Stream, ':- op(1150, xfx, <+).~n', []),
          format(Stream, ':- op(700, xfx, :=).~n~n', []),
          write_term(Stream, Fixture, [quoted(true), variable_names(Bindings)]),
          format(Stream, '.~n', [])
        ),
        close(Stream)),
    format('wrote ~w~n', [Path]).

fixture_term(s1, Rows, fixture(scale_bench, Prog, [], Schedule, []),
             ['Key'=Key, 'Value'=Value]) :-
    Prog = prog(
        [ kind(change/2, log), keep(change/2, all), keyed(head/2, [1]) ],
        [ (head(Key, Value) <+ change(Key, Value)) ]),
    s1_schedule(Rows, Schedule).
fixture_term(s2, Rows, fixture(scale_bench, Prog, Initial, Schedule, []),
             ['BKey'=BKey, 'BMid'=BMid, 'BPayload'=BPayload,
              'AKey'=AKey, 'AMid'=AMid, 'AValue'=AValue]) :-
    Prog = prog(
        [ kind(c/2, set), kind(b_link/2, set), kind(a_link/2, set),
          kind(b/2, set), kind(a/2, set) ],
        [ (b(BKey, BMid) <- c(BKey, BPayload), b_link(BKey, BMid)),
          (a(AKey, AValue) <- b(AKey, AMid), a_link(AMid, AValue)) ]),
    lookup_rows(Initial),
    s2_schedule(Rows, Schedule).
fixture_term(s3, Rows, fixture(scale_bench, Prog, [], Schedule, []),
             ['Left'=Left, 'Right'=Right]) :-
    Prog = prog(
        [ kind(left/1, set), kind(right/1, set), kind(combined/2, set) ],
        [ (combined(Left, Right) <- left(Left), right(Right)) ]),
    s3_schedule(Rows, Schedule).

lookup_rows(Initial) :-
    findall(Row,
            ( between(0, 99, Key), atom_value(k, Key, KeyAtom), atom_value(m, Key, Mid),
              Row = b_link(KeyAtom, Mid) ),
            BRows),
    findall(Row,
            ( between(0, 99, Key), atom_value(m, Key, Mid), atom_value(o, Key, Value),
              Row = a_link(Mid, Value) ),
            ARows),
    append(BRows, ARows, Initial).

s1_schedule(Rows, Schedule) :-
    TickCount is Rows // 100,
    LastTick is TickCount - 1,
    findall(Batch,
            ( between(0, LastTick, Tick),
              s1_batch(Tick, Batch) ),
            Schedule).

s1_batch(Tick, Batch) :-
    findall(+change(KeyAtom, ValueAtom),
            ( between(0, 99, Key),
              atom_value(k, Key, KeyAtom),
              Value is Tick * 1000 + Key,
              atom_value(v, Value, ValueAtom) ),
            Batch).

s2_schedule(Rows, Schedule) :-
    TickCount is Rows // 100,
    LastTick is TickCount - 1,
    findall(Batch,
            ( between(0, LastTick, Tick),
              s2_batch(Tick, Batch) ),
            Schedule).

s2_batch(Tick, Batch) :-
    Start is Tick * 100,
    findall(+c(KeyAtom, PayloadAtom),
            ( between(0, 99, Offset),
              atom_value(k, Offset, KeyAtom),
              Payload is Start + Offset,
              atom_value(p, Payload, PayloadAtom) ),
            Batch).

s3_schedule(Rows, Schedule) :-
    TickCount is Rows // 100,
    LastTick is TickCount - 1,
    findall(Batch,
            ( between(0, LastTick, Tick),
              s3_left_batch(Tick, Batch) ),
            LeftSchedule),
    findall(Batch,
            ( between(0, LastTick, Tick),
              s3_right_batch(Tick, Batch) ),
            RightSchedule),
    append(LeftSchedule, RightSchedule, Schedule).

s3_left_batch(Tick, Batch) :-
    Start is Tick * 100,
    findall(+left(ValueAtom),
            ( between(0, 99, Offset),
              Value is Start + Offset,
              atom_value(l, Value, ValueAtom) ),
            Batch).

s3_right_batch(Tick, Batch) :-
    Start is Tick * 100,
    findall(+right(ValueAtom),
            ( between(0, 99, Offset),
              Value is Start + Offset,
              atom_value(r, Value, ValueAtom) ),
            Batch).

atom_value(Prefix, Number, Atom) :-
    format(atom(Atom), '~w~w', [Prefix, Number]).
