:- module mercury_semi_naive.
:- interface.

:- import_module io.

:- pred main(io::di, io::uo) is det.

:- implementation.

:- import_module array.
:- import_module bool.
:- import_module char.
:- import_module int.
:- import_module list.
:- import_module maybe.
:- import_module string.
:- import_module uint.
:- import_module uint64.

:- type edge
    --->    edge(int, int).

:- type dense_bits
    --->    dense_bits(
                base_word :: int,
                words     :: array(uint64)
            ).

:- type graph
    --->    graph(
                node_count :: int,
                edge_count :: int,
                offsets    :: array(int),
                targets    :: array(int),
                seeds      :: list(edge)
            ).

:- pred monotonic_ms(int::out, io::di, io::uo) is det.
:- pred peak_rss_kb(int::out, io::di, io::uo) is det.

:- pragma foreign_decl("C", "
#include <stdint.h>
#include <sys/resource.h>
#include <time.h>
").

:- pragma foreign_proc("C",
    monotonic_ms(Milliseconds::out, IO0::di, IO::uo),
    [will_not_call_mercury, promise_pure, tabled_for_io], "
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        Milliseconds = (MR_Integer) now.tv_sec * 1000
            + (MR_Integer) (now.tv_nsec / 1000000);
        IO = IO0;
    ").

:- pragma foreign_proc("C",
    peak_rss_kb(PeakRssKb::out, IO0::di, IO::uo),
    [will_not_call_mercury, promise_pure, tabled_for_io], "
        struct rusage usage;
        if (getrusage(RUSAGE_SELF, &usage) != 0) {
            PeakRssKb = -1;
        } else {
#ifdef __APPLE__
            PeakRssKb = (MR_Integer) (usage.ru_maxrss / 1024);
#else
            PeakRssKb = (MR_Integer) usage.ru_maxrss;
#endif
        }
        IO = IO0;
    ").

main(!IO) :-
    io.command_line_arguments(Arguments, !IO),
    ( if Arguments = ["--input", InputPath] then
        monotonic_ms(LoadStart, !IO),
        read_graph(InputPath, ReadResult, !IO),
        (
            ReadResult = yes(Graph),
            seed_seen(Graph ^ node_count, Graph ^ seeds, Seen0, Delta,
                0, Derived0),
            monotonic_ms(LoadedAt, !IO),
            LoadMs = LoadedAt - LoadStart,
            io.format("{\"event\":\"loaded\",\"edges\":%d,\"ms\":%d}\n",
                [i(Graph ^ edge_count), i(LoadMs)], !IO),
            monotonic_ms(FixpointStart, !IO),
            derive(Graph ^ offsets, Graph ^ targets, Delta, Seen0, Seen,
                Derived0, Derived),
            monotonic_ms(FixpointEnd, !IO),
            FixpointMs = FixpointEnd - FixpointStart,
            io.format("{\"event\":\"fixpoint\",\"derived\":%d,\"ms\":%d}\n",
                [i(Derived), i(FixpointMs)], !IO),
            checksum_seen(Seen, Checksum),
            ChecksumHex = string.format("%016x",
                [i(uint64.cast_to_int(Checksum))]),
            peak_rss_kb(PeakRssKb, !IO),
            io.format("{\"event\":\"done\",\"checksum\":\"%s\",\"peak_rss_kb\":%d}\n",
                [s(ChecksumHex), i(PeakRssKb)], !IO)
        ;
            ReadResult = no
        )
    else
        fail_with("usage: mercury-semi-naive --input <path>", !IO)
    ).

:- pred fail_with(string::in, io::di, io::uo) is det.

fail_with(Message, !IO) :-
    io.stderr_stream(Stderr, !IO),
    io.write_string(Stderr, "mercury-semi-naive: " ++ Message ++ "\n", !IO),
    io.set_exit_status(1, !IO).

:- pred read_graph(string::in, maybe(graph)::out, io::di, io::uo) is det.

read_graph(Path, Result, !IO) :-
    io.open_input(Path, OpenResult, !IO),
    (
        OpenResult = io.ok(Stream),
        io.read_file_as_string(Stream, FileResult, !IO),
        io.close_input(Stream, !IO),
        (
            FileResult = io.ok(Content),
            ( if parse_input(Content, Nodes, DeclaredEdges, Edges) then
                list.length(Edges, ActualEdges),
                ( if ActualEdges = DeclaredEdges then
                    build_graph(Nodes, ActualEdges, Edges, Graph),
                    Result = yes(Graph)
                else
                    fail_with(string.format(
                        "header declares %d edges but parsed %d",
                        [i(DeclaredEdges), i(ActualEdges)]), !IO),
                    Result = no
                )
            else
                fail_with("invalid input", !IO),
                Result = no
            )
        ;
            FileResult = io.error(_, ReadError),
            fail_with("read " ++ Path ++ ": " ++ io.error_message(ReadError),
                !IO),
            Result = no
        )
    ;
        OpenResult = io.error(OpenError),
        fail_with("open " ++ Path ++ ": " ++ io.error_message(OpenError), !IO),
        Result = no
    ).

:- pred parse_input(string::in, int::out, int::out, list(edge)::out) is semidet.

parse_input(Content, Nodes, DeclaredEdges, Edges) :-
    Length = string.count_code_units(Content),
    skip_space(Content, Length, 0, HeaderAt),
    HeaderAt < Length,
    string.unsafe_index_code_unit(Content, HeaderAt, HeaderCode),
    HeaderCode = char.to_int('p'),
    scan_uint(Content, Length, HeaderAt + 1, Nodes, AfterNodes),
    scan_uint(Content, Length, AfterNodes, DeclaredEdges, AfterHeader),
    Nodes >= 0,
    DeclaredEdges >= 0,
    scan_edges(Content, Length, AfterHeader, Nodes, [], ReverseEdges),
    list.reverse(ReverseEdges, Edges).

:- pred skip_space(string::in, int::in, int::in, int::out) is det.

skip_space(Content, Length, Position0, Position) :-
    ( if Position0 < Length then
        string.unsafe_index_code_unit(Content, Position0, Code),
        ( if is_space_code(Code) then
            skip_space(Content, Length, Position0 + 1, Position)
        else
            Position = Position0
        )
    else
        Position = Position0
    ).

:- pred is_space_code(int::in) is semidet.

is_space_code(Code) :-
    ( Code = 32 ; Code = 9 ; Code = 10 ; Code = 13 ).

:- pred scan_uint(string::in, int::in, int::in, int::out, int::out) is semidet.

scan_uint(Content, Length, Position0, Value, Position) :-
    skip_space(Content, Length, Position0, DigitAt),
    DigitAt < Length,
    string.unsafe_index_code_unit(Content, DigitAt, FirstCode),
    FirstCode >= 48,
    FirstCode =< 57,
    scan_digits(Content, Length, DigitAt, 0, Value, Position).

:- pred scan_digits(string::in, int::in, int::in, int::in, int::out, int::out)
    is det.

scan_digits(Content, Length, Position0, Value0, Value, Position) :-
    ( if Position0 < Length then
        string.unsafe_index_code_unit(Content, Position0, Code),
        ( if Code >= 48, Code =< 57 then
            scan_digits(Content, Length, Position0 + 1,
                Value0 * 10 + Code - 48, Value, Position)
        else
            Value = Value0,
            Position = Position0
        )
    else
        Value = Value0,
        Position = Position0
    ).

:- pred scan_edges(string::in, int::in, int::in, int::in,
    list(edge)::in, list(edge)::out) is semidet.

scan_edges(Content, Length, Position0, Nodes, !Edges) :-
    skip_space(Content, Length, Position0, Position),
    ( if Position >= Length then
        true
    else
        scan_uint(Content, Length, Position, Source, AfterSource),
        scan_uint(Content, Length, AfterSource, Target, AfterTarget),
        Source >= 0,
        Source < Nodes,
        Target >= 0,
        Target < Nodes,
        !:Edges = [edge(Source, Target) | !.Edges],
        scan_edges(Content, Length, AfterTarget, Nodes, !Edges)
    ).

:- pred build_graph(int::in, int::in, list(edge)::in, graph::out) is det.

build_graph(Nodes, EdgeCount, Edges, Graph) :-
    array.init(Nodes + 1, 0, Counts0),
    count_edges(Edges, Counts0, Counts),
    prefix_offsets(0, Nodes, Counts, Offsets),
    array.copy(Offsets, Cursors0),
    array.init(EdgeCount, 0, Targets0),
    fill_targets(Edges, Cursors0, _Cursors, Targets0, Targets),
    Graph = graph(Nodes, EdgeCount, Offsets, Targets, Edges).

:- pred count_edges(list(edge)::in, array(int)::array_di,
    array(int)::array_uo) is det.

count_edges([], !Counts).
count_edges([edge(Source, _) | Rest], !Counts) :-
    CountIndex = Source + 1,
    array.unsafe_lookup(!.Counts, CountIndex, Count0),
    array.unsafe_set(CountIndex, Count0 + 1, !Counts),
    count_edges(Rest, !Counts).

:- pred prefix_offsets(int::in, int::in, array(int)::array_di,
    array(int)::array_uo) is det.

prefix_offsets(Index, Nodes, !Offsets) :-
    ( if Index < Nodes then
        array.unsafe_lookup(!.Offsets, Index, Before),
        array.unsafe_lookup(!.Offsets, Index + 1, Count),
        array.unsafe_set(Index + 1, Before + Count, !Offsets),
        prefix_offsets(Index + 1, Nodes, !Offsets)
    else
        true
    ).

:- pred fill_targets(list(edge)::in, array(int)::array_di,
    array(int)::array_uo, array(int)::array_di, array(int)::array_uo) is det.

fill_targets([], !Cursors, !Targets).
fill_targets([edge(Source, Target) | Rest], !Cursors, !Targets) :-
    array.unsafe_lookup(!.Cursors, Source, TargetIndex),
    array.unsafe_set(TargetIndex, Target, !Targets),
    array.unsafe_set(Source, TargetIndex + 1, !Cursors),
    fill_targets(Rest, !Cursors, !Targets).

:- func empty_dense_bits(int) = dense_bits.

empty_dense_bits(_) = dense_bits(0, array.make_empty_array).

:- pred seed_seen(int::in, list(edge)::in, array(dense_bits)::array_uo,
    list(edge)::out, int::in, int::out) is det.

seed_seen(Nodes, Seeds, Seen, Delta, !Derived) :-
    Seen0 = array.generate(Nodes, empty_dense_bits),
    seed_edges(Seeds, Seen0, Seen, [], ReverseDelta,
        !Derived),
    list.reverse(ReverseDelta, Delta).

:- pred seed_edges(list(edge)::in, array(dense_bits)::array_di,
    array(dense_bits)::array_uo, list(edge)::in, list(edge)::out,
    int::in, int::out) is det.

seed_edges([], !Seen, !Delta, !Derived).
seed_edges([Pair @ edge(Source, Target) | Rest], !Seen, !Delta,
        !Derived) :-
    insert_seen(Source, Target, IsNew, !Seen),
    ( if IsNew = yes then
        !:Delta = [Pair | !.Delta],
        !:Derived = !.Derived + 1
    else
        true
    ),
    seed_edges(Rest, !Seen, !Delta, !Derived).

:- pred insert_seen(int::in, int::in, bool::out,
    array(dense_bits)::array_di, array(dense_bits)::array_uo) is det.

insert_seen(Source, Target, IsNew, !Seen) :-
    array.unsafe_lookup(!.Seen, Source, Bits0),
    insert_dense(Target, IsNew, Bits0, Bits),
    array.unsafe_set(Source, Bits, !Seen).

:- pred insert_dense(int::in, bool::out,
    dense_bits::in, dense_bits::out) is det.

insert_dense(Target, IsNew, dense_bits(Base0, Words0), dense_bits(Base, Words)) :-
    TargetWord = Target // 64,
    BitIndex = uint.cast_from_int(Target mod 64),
    ensure_word(TargetWord, Base0, Words0, Base, Words1),
    WordIndex = TargetWord - Base,
    array.unsafe_lookup(Words1, WordIndex, Word0),
    ( if uint64.bit_is_set(Word0, BitIndex) then
        IsNew = no,
        Words = Words1
    else
        IsNew = yes,
        Word = uint64.set_bit(Word0, BitIndex),
        array.unsafe_set(WordIndex, Word, Words1, Words)
    ).

:- pred ensure_word(int::in, int::in, array(uint64)::array_di,
    int::out, array(uint64)::array_uo) is det.

ensure_word(TargetWord, Base0, Words0, Base, Words) :-
    Length0 = array.size(Words0),
    ( if Length0 = 0 then
        Base = TargetWord,
        array.init(1, uint64.cast_from_int(0), Words)
    else if TargetWord >= Base0, TargetWord < Base0 + Length0 then
        Base = Base0,
        Words = Words0
    else
        Lower = int.min(TargetWord, Base0),
        Upper = int.max(TargetWord + 1, Base0 + Length0),
        Required = Upper - Lower,
        grow_capacity(Length0 * 2, Required, NewLength),
        ( if TargetWord < Base0 then
            Base = Upper - NewLength
        else
            Base = Base0
        ),
        array.init(NewLength, uint64.cast_from_int(0), NewWords0),
        copy_words(0, Base0 - Base, Words0, NewWords0, Words)
    ).

:- pred grow_capacity(int::in, int::in, int::out) is det.

grow_capacity(Candidate, Required, Capacity) :-
    ( if Candidate >= Required then
        Capacity = Candidate
    else
        grow_capacity(Candidate * 2, Required, Capacity)
    ).

:- pred copy_words(int::in, int::in, array(uint64)::in,
    array(uint64)::array_di, array(uint64)::array_uo) is det.

copy_words(Index, DestinationOffset, Source, !Destination) :-
    ( if Index < array.size(Source) then
        array.unsafe_lookup(Source, Index, Word),
        array.unsafe_set(DestinationOffset + Index, Word, !Destination),
        copy_words(Index + 1, DestinationOffset, Source, !Destination)
    else
        true
    ).

:- pred derive(array(int)::in, array(int)::in, list(edge)::in,
    array(dense_bits)::array_di, array(dense_bits)::array_uo,
    int::in, int::out) is det.

derive(Offsets, Targets, Delta, !Seen, !Derived) :-
    (
        Delta = []
    ;
        Delta = [_ | _],
        extend_delta(Offsets, Targets, Delta, [], ReverseNext,
            !Seen, !Derived),
        list.reverse(ReverseNext, Next),
        derive(Offsets, Targets, Next, !Seen, !Derived)
    ).

:- pred extend_delta(array(int)::in, array(int)::in, list(edge)::in,
    list(edge)::in, list(edge)::out,
    array(dense_bits)::array_di, array(dense_bits)::array_uo,
    int::in, int::out) is det.

extend_delta(_, _, [], !Next, !Seen, !Derived).
extend_delta(Offsets, Targets, [edge(Source, Mid) | Rest], !Next,
        !Seen, !Derived) :-
    array.unsafe_lookup(Offsets, Mid, TargetAt),
    array.unsafe_lookup(Offsets, Mid + 1, TargetEnd),
    extend_targets(TargetAt, TargetEnd, Source, Targets, !Next,
        !Seen, !Derived),
    extend_delta(Offsets, Targets, Rest, !Next,
        !Seen, !Derived).

:- pred extend_targets(int::in, int::in, int::in, array(int)::in,
    list(edge)::in, list(edge)::out,
    array(dense_bits)::array_di, array(dense_bits)::array_uo,
    int::in, int::out) is det.

extend_targets(TargetAt, TargetEnd, Source, Targets, !Next,
        !Seen, !Derived) :-
    ( if TargetAt < TargetEnd then
        array.unsafe_lookup(Targets, TargetAt, Target),
        insert_seen(Source, Target, IsNew, !Seen),
        ( if IsNew = yes then
            !:Next = [edge(Source, Target) | !.Next],
            !:Derived = !.Derived + 1
        else
            true
        ),
        extend_targets(TargetAt + 1, TargetEnd, Source, Targets, !Next,
            !Seen, !Derived)
    else
        true
    ).

:- pred checksum_seen(array(dense_bits)::in, uint64::out) is det.

checksum_seen(Seen, Checksum) :-
    checksum_sources(0, Seen, uint64.cast_from_int(0), Checksum).

:- pred checksum_sources(int::in, array(dense_bits)::in,
    uint64::in, uint64::out) is det.

checksum_sources(Source, Seen, !Checksum) :-
    ( if Source < array.size(Seen) then
        array.unsafe_lookup(Seen, Source, Bits),
        checksum_dense(Source, Bits, !Checksum),
        checksum_sources(Source + 1, Seen, !Checksum)
    else
        true
    ).

:- pred checksum_dense(int::in, dense_bits::in,
    uint64::in, uint64::out) is det.

checksum_dense(Source, dense_bits(Base, Words), !Checksum) :-
    checksum_words(Source, Base, 0, Words, !Checksum).

:- pred checksum_words(int::in, int::in, int::in, array(uint64)::in,
    uint64::in, uint64::out) is det.

checksum_words(Source, Base, WordIndex, Words, !Checksum) :-
    ( if WordIndex < array.size(Words) then
        array.unsafe_lookup(Words, WordIndex, Word),
        checksum_word_bits(Source, (Base + WordIndex) * 64, 0, Word,
            !Checksum),
        checksum_words(Source, Base, WordIndex + 1, Words, !Checksum)
    else
        true
    ).

:- pred checksum_word_bits(int::in, int::in, int::in, uint64::in,
    uint64::in, uint64::out) is det.

checksum_word_bits(Source, TargetBase, BitIndex, Word, !Checksum) :-
    ( if BitIndex < 64 then
        ( if uint64.bit_is_set(Word, uint.cast_from_int(BitIndex)) then
            !:Checksum = uint64.xor(!.Checksum,
                fnv1a64(Source, TargetBase + BitIndex))
        else
            true
        ),
        checksum_word_bits(Source, TargetBase, BitIndex + 1, Word, !Checksum)
    else
        true
    ).

:- func fnv1a64(int, int) = uint64.

fnv1a64(Source, Target) = Hash :-
    Offset = uint64.from_bytes_be(203u8, 242u8, 156u8, 228u8,
        132u8, 34u8, 35u8, 37u8),
    Hash4 = fnv_int_le(Source, Offset),
    Hash = fnv_int_le(Target, Hash4).

:- func fnv_int_le(int, uint64) = uint64.

fnv_int_le(Value, Hash0) = Hash :-
    Hash1 = fnv_byte(Value mod 256, Hash0),
    Hash2 = fnv_byte((Value // 256) mod 256, Hash1),
    Hash3 = fnv_byte((Value // 65536) mod 256, Hash2),
    Hash = fnv_byte((Value // 16777216) mod 256, Hash3).

:- func fnv_byte(int, uint64) = uint64.

fnv_byte(Byte, Hash0) = Hash :-
    Prime = uint64.from_bytes_be(0u8, 0u8, 1u8, 0u8,
        0u8, 0u8, 1u8, 179u8),
    Hash = uint64.times(
        uint64.xor(Hash0, uint64.cast_from_int(Byte)), Prime).
