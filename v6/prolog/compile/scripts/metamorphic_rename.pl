% metamorphic_rename.pl : metamorphic rename pass over the conformance corpus.
%
% Law under test: renaming every rel / variable / module-segment in a program
% (camelCase, __dunder__, trailing_underscore_, ALLCAPS, max-length shapes
% included) must produce emitted artifacts identical modulo the rename map.
%
% Two compile legs per compiled fixture (original + renamed), through the same
% single-program compile chain the sweep uses (program_plan -> lower_program ->
% boot_statements -> emit_program + the three type-artifact emitters + the
% schedule JSON), all captured IN MEMORY so nothing under compile/out/ is
% touched. The renamed artifacts are then inverse-mapped (new name -> old name)
% and compared byte-for-byte against the original; any residue not explained by
% the map is a candidate name-sensitivity finding.
%
% Run (from v6/prolog):
%   swipl -q -s compile/scripts/metamorphic_rename.pl -g run -t halt

:- module(metamorphic_rename, [ run/0, run/1 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(filesex)).
:- use_module(library(http/json)).

:- use_module('../../compile', [ program_plan/3, default_intern_mode/1,
                                 read_fixture_term/4 ]).
:- use_module('../../lower',
              [ lower_program/2, boot_statements/7, catalog_decl_rows/6 ]).
:- use_module('../../emit_ts', [ emit_program/5 ]).
:- use_module('../../3_analyze/analyze', [ declared_refs/2, program_refs/2,
                                 seeded_refs/2, snake_name/2 ]).
:- use_module('../../0_dot_expand/body', [ rel_ref/2 ]).
:- use_module('../../0_rel_record', [ relplan_column_types/3 ]).
:- use_module('../../0_dot_expand/0_type_plane',
              [ type_canonical_json/4, canonical_json_text/2,
                escape_json_codes/2 ]).
:- use_module('../4_emit_jsonschema', [ jsonschema_text/3, option_rows/3 ]).
:- use_module('../7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('../8_emit_rust_types', [ rust_types_text/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- dynamic(metamorphic_home/1).
:- prolog_load_context(directory, Here), assertz(metamorphic_home(Here)).

prolog_dir(Dir) :-
    metamorphic_home(ScriptsDir),
    atomic_list_concat([ScriptsDir, '/../..'], Dir).

manifest_file(Path) :-
    prolog_dir(Dir),
    atomic_list_concat([Dir, '/compile/out/manifest.json'], Path).

fixtures_dir(Dir) :- prolog_dir(Here), atomic_list_concat([Here, '/conformance/fixtures'], Dir).

default_seed(20260815).

% ═══════════════════════════════════════════════════════════════════════════
% top level
% ═══════════════════════════════════════════════════════════════════════════

run :- default_seed(Seed), run(Seed).

run(Seed) :-
    format("~n═══ METAMORPHIC RENAME ═══~n"),
    format("seed = ~w~n", [Seed]),
    compiled_fixtures(Fixtures),
    length(Fixtures, Total),
    format("compiled fixtures in manifest: ~w~n", [Total]),
    run_all(Fixtures, Seed, Findings, Stats),
    report_stats(Stats),
    report_findings(Findings),
    halt.

% ═══════════════════════════════════════════════════════════════════════════
% manifest -> compiled fixture (Name, File) list
% ═══════════════════════════════════════════════════════════════════════════

compiled_fixtures(Fixtures) :-
    manifest_file(Path),
    setup_call_cleanup(open(Path, read, Stream),
                       json_read_dict(Stream, Entries, []),
                       close(Stream)),
    findall(Name-StrFile,
            ( member(Entry, Entries),
              get_dict(bucket, Entry, "compiled"),
              get_dict(name, Entry, NameStr),
              atom_string(Name, NameStr),
              get_dict(file, Entry, StrFile) ),
            Fixtures).

% ═══════════════════════════════════════════════════════════════════════════
% sweep loop
% ═══════════════════════════════════════════════════════════════════════════

run_all([], _Seed, [], Stats) :- zero_stats(Stats).
run_all(Fixtures, Seed, Findings, Stats) :-
    findall(Stat, ( member(Name-File, Fixtures),
                    sweep_fixture(File, Name, Seed, Stat) ), Stats0),
    include(is_finding, Stats0, Findings),
    Stats = Stats0.

sweep_fixture(File, Name, Seed, Stat) :-
    fixtures_dir(Dir),
    atomic_list_concat([Dir, '/', File], FixturePath),
    ( catch(( read_fixture_term(FixturePath, Name, Term, Bindings),
              metamorphic_one(Name, Term, Bindings, Seed, Outcome) ),
            Error, Outcome = harness_error(Error))
    -> true
    ;  Outcome = harness_fail(Name)
    ),
    ( Outcome = ok
    -> Stat = ok(Name)
    ; Outcome = finding(_)
    -> Stat = finding(Name-Outcome)
    ; Stat = skipped(Name-Outcome)
    ).

% ═══════════════════════════════════════════════════════════════════════════
% one fixture: build map, rename, compile both, compare
% ═══════════════════════════════════════════════════════════════════════════

metamorphic_one(Name, Term, Bindings, Seed, Outcome) :-
    Term = fixture(Name, prog(Decls, Rules), Initial, _Schedule, _Expectations),
    collect_rel_names(Decls, Rules, Initial, RelNameAtoms),
    collect_segments(Decls, Rules, SegAtoms),
    union(RelNameAtoms, SegAtoms, IdAtoms),
    collect_var_names(Bindings, VarAtoms),
    build_maps(Seed, IdAtoms, VarAtoms, IdMap, VarMap),
    apply_fixture_rename(Term, Bindings, IdMap, VarMap, RenamedTerm, RenamedBindings),
    compile_capture(Name, Term, Bindings, OriginalCapture),
    compile_capture(Name, RenamedTerm, RenamedBindings, RenamedCapture),
    ( ( OriginalCapture = error(_) ; RenamedCapture = error(_) )
    -> Outcome = finding(Name-original_or_renamed_error(
                            OriginalCapture, RenamedCapture))
    ; build_inverse_map(IdMap, VarMap, InverseMap),
      compare_captures(OriginalCapture, RenamedCapture, InverseMap, Name, Outcome)
    ).

% ═══════════════════════════════════════════════════════════════════════════
% identifier collection
% ═══════════════════════════════════════════════════════════════════════════

collect_rel_names(Decls, Rules, Initial, Names) :-
    declared_refs(Decls, Declared),
    program_refs(Rules, ProgRefs),
    seeded_refs(Initial, Seeded),
    append([Declared, ProgRefs, Seeded], AllRefs),
    findall(Name, ( member(Name/_, AllRefs) ), Names0),
    sort(Names0, Names).

% module segments live in rel_path_decl(_, Segs) decls and rel_path(Segs, _)
% body atoms; walk both for the segment lists.
collect_segments(Decls, Rules, Segs) :-
    findall(Seg,
            ( member(Decl, Decls),
              Decl = rel_path_decl(_, SegList),
              member(Seg, SegList) ),
            Segs0),
    findall(Seg,
            ( member(Rule, Rules),
              rule_segments(Rule, Seg) ),
            Segs1),
    append(Segs0, Segs1, All),
    sort(All, Segs).

rule_segments(Rule, Seg) :-
    ( Rule = (_Head <- Body) -> term_segments(Body, Seg)
    ; Rule = (_Head <+ Body) -> term_segments(Body, Seg)
    ; term_segments(Rule, Seg)
    ).

term_segments(Term, Seg) :-
    ( var(Term) -> fail
    ; Term = rel_path(SegList, _) -> member(Seg, SegList)
    ; compound(Term) -> Term =.. [_ | Args], member(Arg, Args), term_segments(Arg, Seg)
    ; fail
    ).

collect_var_names(Bindings, Names) :-
    findall(Name,
            ( member(Name = Var, Bindings),
              var(Var),
              proper_var_name(Name) ),
            Names0),
    sort(Names0, Names).

proper_var_name(Name) :-
    atom(Name),
    Name \== '_',
    atom_chars(Name, [First | _]),
    ( First == '_' ; char_type(First, upper) ).

% ═══════════════════════════════════════════════════════════════════════════
% name maps (deterministic, seeded)
% ═══════════════════════════════════════════════════════════════════════════

build_maps(Seed, IdAtoms, VarAtoms, IdMap, VarMap) :-
    assign_names(rel, Seed, IdAtoms, IdMap),
    assign_names(var, Seed, VarAtoms, VarMap).

assign_names(_Class, _Seed, [], []).
assign_names(Class, Seed, Atoms, Map) :-
    assign_names(Class, Seed, Atoms, 0, Map).

assign_names(_Class, _Seed, [], _Idx, []).
assign_names(Class, Seed, [Atom | Atoms], Idx, [Atom-New | Map]) :-
    new_name(Class, Seed, Idx, New),
    Idx1 is Idx + 1,
    assign_names(Class, Seed, Atoms, Idx1, Map).

new_name(rel, Seed, Idx, New) :-
    ShapeCount = 5,
    ShapeIdx is (Idx + Seed) mod ShapeCount,
    rel_shape_name(ShapeIdx, Idx, New).

new_name(var, Seed, Idx, New) :-
    ShapeCount = 6,
    ShapeIdx is (Idx + Seed) mod ShapeCount,
    var_shape_name(ShapeIdx, Idx, New).

rel_shape_name(0, Idx, New) :- format(atom(New), 'rel_~w', [Idx]).                    % snake
rel_shape_name(1, Idx, New) :- format(atom(New), 'relCase~w', [Idx]).                 % camelCase
rel_shape_name(2, Idx, New) :- format(atom(New), 'rel_tail_~w_', [Idx]).             % trailing_
rel_shape_name(3, Idx, New) :- format(atom(New), 'REL_CAPS_~w', [Idx]).              % ALLCAPS
rel_shape_name(4, Idx, New) :- pad(50, Pad), format(atom(New), 'rel_maxlen_~w_~w', [Idx, Pad]).

var_shape_name(0, Idx, New) :- format(atom(New), 'Var_~w', [Idx]).
var_shape_name(1, Idx, New) :- format(atom(New), 'VarCase~w', [Idx]).
var_shape_name(2, Idx, New) :- format(atom(New), 'Var_tail_~w_', [Idx]).
var_shape_name(3, Idx, New) :- format(atom(New), 'VAR_CAPS_~w', [Idx]).
var_shape_name(4, Idx, New) :- pad(50, Pad), format(atom(New), 'Var_maxlen_~w_~w', [Idx, Pad]).
var_shape_name(5, Idx, New) :- format(atom(New), '_Var_dunder_~w_', [Idx]).

pad(0, '').
pad(N, S) :- N > 0, N1 is N - 1, pad(N1, Rest), atom_concat('x', Rest, S).

% ═══════════════════════════════════════════════════════════════════════════
% rename application
% ═══════════════════════════════════════════════════════════════════════════

apply_fixture_rename(fixture(Name, Prog, Initial, Schedule, Expectations),
                     Bindings, IdMap, VarMap,
                     fixture(Name, RenamedProg, RenamedInitial, RenamedSchedule,
                             RenamedExpectations),
                     RenamedBindings) :-
    rename_prog(Prog, IdMap, RenamedProg),
    rename_initial(IdMap, Initial, RenamedInitial),
    rename_schedule(IdMap, Schedule, RenamedSchedule),
    rename_expectations(Expectations, IdMap, RenamedExpectations),
    rename_bindings(Bindings, VarMap, RenamedBindings).

rename_prog(prog(Decls, Rules), IdMap, prog(RenamedDecls, RenamedRules)) :-
    maplist(rename_decl(IdMap), Decls, RenamedDecls),
    maplist(rename_rule(IdMap), Rules, RenamedRules).

rename_decl(IdMap, Decl, Renamed) :-
    rename_rel_atom(IdMap, Decl, Renamed).

rename_ref(IdMap, Name/Arity, NewName/Arity) :-
    map_atom(IdMap, Name, NewName).

map_atom(IdMap, Atom, New) :-
    ( memberchk(Atom-New, IdMap) -> true ; New = Atom ).

rename_rule(IdMap, Rule, Renamed) :-
    ( Rule = (Head <- Body)
    -> rename_head(IdMap, Head, RenamedHead),
       rename_body(IdMap, Body, RenamedBody),
       Renamed = (RenamedHead <- RenamedBody)
    ; Rule = (Head <+ Body)
    -> rename_head(IdMap, Head, RenamedHead),
       rename_body(IdMap, Body, RenamedBody),
       Renamed = (RenamedHead <+ RenamedBody)
    ; rename_generic(IdMap, Rule, Renamed)
    ).

rename_head(IdMap, Head, Renamed) :-
    rename_rel_atom(IdMap, Head, Renamed).

rename_rel_atom(IdMap, Term, Renamed) :-
    ( var(Term) -> Renamed = Term
    ; atomic(Term) -> map_atom(IdMap, Term, Renamed)
    ; Term =.. [F | Args],
      map_atom(IdMap, F, NewF),
      maplist(rename_rel_atom(IdMap), Args, NewArgs),
      Renamed =.. [NewF | NewArgs]
    ).

rename_body(IdMap, Body, Renamed) :-
    ( var(Body) -> Renamed = Body
    ; Body = rel_path(Segs, Args)
    -> maplist(map_atom(IdMap), Segs, RenamedSegs),
       maplist(rename_rel_atom(IdMap), Args, RenamedArgs),
       Renamed = rel_path(RenamedSegs, RenamedArgs)
    ; rename_rel_atom(IdMap, Body, Renamed)
    ).

rename_initial(IdMap, Initial, Renamed) :-
    maplist(rename_arrival(IdMap), Initial, Renamed).

rename_arrival(IdMap, Term, Renamed) :-
    ( Term = +Atom
    -> rename_rel_atom(IdMap, Atom, RenamedAtom), Renamed = +RenamedAtom
    ; Term = -Atom
    -> rename_rel_atom(IdMap, Atom, RenamedAtom), Renamed = -RenamedAtom
    ; rename_rel_atom(IdMap, Term, Renamed)
    ).

rename_schedule(IdMap, Schedule, Renamed) :-
    maplist(rename_initial(IdMap), Schedule, Renamed).

rename_expectations(Expectations, IdMap, Renamed) :-
    maplist(rename_expectation(IdMap), Expectations, Renamed).

rename_expectation(IdMap, Expectation, Renamed) :-
    ( Expectation = final(Ref, Rows)
    -> rename_ref(IdMap, Ref, RenamedRef),
       maplist(rename_rel_atom(IdMap), Rows, RenamedRows),
       Renamed = final(RenamedRef, RenamedRows)
    ; Expectation = deltas(Ref, Batches)
    -> rename_ref(IdMap, Ref, RenamedRef),
       maplist(rename_initial(IdMap), Batches, RenamedBatches),
       Renamed = deltas(RenamedRef, RenamedBatches)
    ; rename_generic(IdMap, Expectation, Renamed)
    ).

% fallback: recurse into any compound, renaming map atoms it meets in functor
% or rel-path position (used for decls whose shape we did not special-case).
rename_generic(IdMap, Term, Renamed) :-
    ( var(Term) -> Renamed = Term
    ; Term = rel_path(Segs, Args)
    -> maplist(map_atom(IdMap), Segs, RenamedSegs),
       maplist(rename_generic(IdMap), Args, RenamedArgs),
       Renamed = rel_path(RenamedSegs, RenamedArgs)
    ; atomic(Term) -> Renamed = Term
    ; Term =.. [F | Args],
      ( memberchk(F-NewF, IdMap) -> F1 = NewF ; F1 = F ),
      maplist(rename_generic(IdMap), Args, NewArgs),
      Renamed =.. [F1 | NewArgs]
    ).

rename_bindings([], _VarMap, []).
rename_bindings([Name = Var | Rest], VarMap, [NewName = Var | More]) :-
    ( memberchk(Name-NewName, VarMap) -> true ; NewName = Name ),
    rename_bindings(Rest, VarMap, More).

% ═══════════════════════════════════════════════════════════════════════════
% compile-and-capture (in memory; mirrors sweep.pl sweep_one + arm_census)
% ═══════════════════════════════════════════════════════════════════════════

compile_capture(Name, Term, Bindings, Capture) :-
    default_intern_mode(Mode),
    catch(
        ( program_plan(Term-Bindings, [intern(Mode)], Plan),
          lower_program(Plan, Lowered),
          Term = fixture(Name, _Prog, Initial, Schedule, _Expectations),
          Plan = plan(_, prog(Decls, Rules), Types, RelPlans, _, _, _, _, Mode),
          Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
          boot_statements(Mode, Decls, Types, RelPlans, Initial,
                          LevelStatements, BootStatements),
          emit_program(Name, Plan, Lowered, BootStatements, TsText),
          schedule_json(Types, RelPlans, Schedule, ScheduleJson),
          catch(( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                     SchemaRows, _),
                  option_rows(Decls, SchemaRows, SchemaRowsOpt),
                  jsonschema_text(Name, SchemaRowsOpt, SchemaText) ),
                _SchemaError, SchemaText = '<schema-error>'),
          catch(( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                     TypeRows, _),
                  option_rows(Decls, TypeRows, TypeRowsOpt),
                  ts_types_text(Name, TypeRowsOpt, TsTypesText) ),
                _TsError, TsTypesText = '<tstypes-error>'),
          catch(( catalog_decl_rows(Name, Rules, RelPlans, Decls,
                                     TypeRows2, _),
                  option_rows(Decls, TypeRows2, TypeRowsOpt2),
                  rust_types_text(Name, TypeRowsOpt2, RustTypesText) ),
                _RustError, RustTypesText = '<rusttypes-error>'),
          to_string(TsText, TsStr),
          to_string(ScheduleJson, SchedStr),
          to_string(SchemaText, SchemaStr),
          to_string(TsTypesText, TsTypesStr),
          to_string(RustTypesText, RustTypesStr),
          Capture = capture(TsStr, SchedStr, SchemaStr, TsTypesStr, RustTypesStr)
        ),
        Error,
        Capture = error(Error)
    ).

to_string(X, S) :- atom(X), !, atom_string(X, S).
to_string(X, S) :- string(X), !, S = X.
to_string(X, S) :- term_string(X, S).

% ═══════════════════════════════════════════════════════════════════════════
% schedule -> JSON (copied verbatim from sweep.pl so the byte shape matches)
% ═══════════════════════════════════════════════════════════════════════════

schedule_json(Types, RelPlans, Schedule, Json) :-
    maplist(tick_json(Types, RelPlans), Schedule, TickJsons),
    atomic_list_concat(TickJsons, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).

tick_json(Types, RelPlans, Batch, Json) :-
    maplist(arrival_json(Types, RelPlans), Batch, ArrivalJsons),
    atomic_list_concat(ArrivalJsons, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).

arrival_json(Types, RelPlans, +Atom, Json) :- !, arrival_json_signed(Types, RelPlans, Atom, add, Json).
arrival_json(Types, RelPlans, -Atom, Json) :- !, arrival_json_signed(Types, RelPlans, Atom, del, Json).

arrival_json_signed(Types, RelPlans, Atom, Sign, Json) :-
    rel_ref(Atom, Ref), Ref = Name/_Arity,
    Atom =.. [_ | Args],
    ( relplan_column_types(RelPlans, Ref, ColumnTypes) -> true ; ColumnTypes = [] ),
    maplist(arrival_value_json(Types), ColumnTypes, Args, ArgJsons),
    atomic_list_concat(ArgJsons, ',', RowInner),
    json_string(Name, NameJson),
    format(atom(Json), '{"rel":~w,"sign":"~w","row":[~w]}', [NameJson, Sign, RowInner]).

arrival_value_json(Types, ref(TypeName), Value, Json) :- !,
    type_canonical_json(Types, TypeName, Value, Json).
arrival_value_json(_, json, Value, Json) :- !,
    canonical_json_text(Value, Text), json_string(Text, Json).
arrival_value_json(_, json_list(_), Value, Json) :- !,
    canonical_json_text(Value, Text), json_string(Text, Json).
arrival_value_json(_, _, Value, Json) :- row_value_json(Value, Json).

row_value_json(Value, Json) :- integer(Value), !, format(atom(Json), '~w', [Value]).
row_value_json(bool_lit(Boolean), Json) :- !, format(atom(Json), '~w', [Boolean]).
row_value_json(Value, Json) :- float(Value), !, canonical_json_text(Value, Json).
row_value_json(Value, Json) :- compound(Value), !, term_text(Value, Text), json_string(Text, Json).
row_value_json(Value, Json) :- json_string(Value, Json).

term_text(Value, Text) :- atomic(Value), !, format(atom(Text), '~w', [Value]).
term_text(Value, Text) :- compound(Value), !,
    Value =.. [Name | Args], maplist(term_text, Args, ArgTexts),
    atomic_list_concat(ArgTexts, ',', Inner), format(atom(Text), '~w(~w)', [Name, Inner]).

json_string(Value, Json) :-
    format(atom(Raw), '~w', [Value]),
    atom_codes(Raw, Codes),
    escape_json_codes(Codes, EscapedCodes),
    atom_codes(Escaped, EscapedCodes),
    format(atom(Json), '"~w"', [Escaped]).

% ═══════════════════════════════════════════════════════════════════════════
% inverse map + comparison
% ═══════════════════════════════════════════════════════════════════════════

% Inverse map = new -> old tokens. Rel table-name prefixes are part of the
% compiler's known identifier shape, so they are mapped too. snake_name and
% type_name transforms are DELIBERATELY NOT pre-mapped: a residual diff there is
% the name-sensitivity signal this pass exists to find.
build_inverse_map(IdMap, VarMap, InverseMap) :-
    findall(New-Old, member(Old-New, IdMap), BasePairs),
    findall(NewT-OldT,
            ( member(Old-New, IdMap),
              ( rel_prefix_pair('__delta_', New, Old, NewT, OldT)
              ; rel_prefix_pair('__frontier_', New, Old, NewT, OldT)
              ; rel_prefix_pair('__next_frontier_', New, Old, NewT, OldT)
              ; rel_prefix_pair('__pre_', New, Old, NewT, OldT)
              ; rel_prefix_pair('__support_next_', New, Old, NewT, OldT)
              ; rel_prefix_pair('__departure_frontier_', New, Old, NewT, OldT)
              )
            ), PrefixPairs),
    append(BasePairs, PrefixPairs, RelPairs),
    VarPairs = VarMap,
    append(RelPairs, VarPairs, InverseMap).

rel_prefix_pair(Prefix, New, Old, NewT, OldT) :-
    atom_concat(Prefix, New, NewT),
    atom_concat(Prefix, Old, OldT).

compare_captures(capture(A,B,C,D,E), capture(RA,RB,RC,RD,RE), Map, Name, Outcome) :-
    inv(A, RA, Map, IA), inv(B, RB, Map, IB), inv(C, RC, Map, IC),
    inv(D, RD, Map, ID), inv(E, RE, Map, IE),
    ( ( A == IA, B == IB, C == IC, D == ID, E == IE )
    -> Outcome = ok
    ; diff_kind(A, IA, ts, D1),
      diff_kind(B, IB, schedule, D2),
      diff_kind(C, IC, schema, D3),
      diff_kind(D, ID, tstypes, D4),
      diff_kind(E, IE, rusttypes, D5),
      Outcome = finding(Name-artifact_diff([D1,D2,D3,D4,D5]))
    ).

inv(_Orig, Renamed, Map, Out) :-
    apply_token_map(Renamed, Map, Out).

diff_kind(A, B, Kind, Result) :-
    ( A == B -> Result = same(Kind)
    ; string_length(A, LA), string_length(B, LB),
      window(A, 0, 200, AEx), window(B, 0, 200, BEx),
      Result = diff(Kind, LA, LB, AEx, BEx)
    ).

window(S, Start, Len, Ex) :-
    string_length(S, L),
    ( Start < L
    -> Len1 is min(Len, L - Start),
       sub_string(S, Start, Len1, _, Ex)
    ; Ex = "" ).

% ordered string substitution, longest new-name first
apply_token_map(Str, Map, Out) :-
    sort_pairs_by_len(Map, Ordered),
    foldl(replace_pair, Ordered, Str, Out).

sort_pairs_by_len(Map, Ordered) :-
    map_list_to_pairs(key_len, Map, Keyed),
    keysort(Keyed, SortedAsc),
    reverse(SortedAsc, SortedDesc),
    pairs_values(SortedDesc, Ordered).

key_len(New-_, K-New) :- atom_string(New, S), string_length(S, L), K is -L.

replace_pair(New-Old, Str, Out) :-
    atom_string(New, NewS),
    atom_string(Old, OldS),
    ( sub_string(Str, _, _, _, NewS)
    -> replace_all(Str, NewS, OldS, Out)
    ; Out = Str
    ).

replace_all(Str, Find, Replace, Out) :-
    atomic_list_concat(Parts, Find, Str),
    atomic_list_concat(Parts, Replace, Out).

% ═══════════════════════════════════════════════════════════════════════════
% reporting
% ═══════════════════════════════════════════════════════════════════════════

zero_stats([]).

report_stats(Stats) :-
    include(is_ok, Stats, Oks),
    include(is_finding, Stats, Findings),
    include(is_skipped, Stats, Skipped),
    length(Stats, Total), length(Oks, OkCount),
    length(Findings, FindingCount), length(Skipped, SkipCount),
    format("~n═══ COUNTS ═══~n"),
    format("swept=~w  identical-modulo-map=~w  findings=~w  skipped=~w~n",
           [Total, OkCount, FindingCount, SkipCount]).

is_ok(ok(_)).
is_finding(finding(_)).
is_skipped(skipped(_)).

report_findings([]) :-
    format("~nno findings.~n").
report_findings(Findings) :-
    length(Findings, N),
    format("~n═══ FINDINGS (~w) ═══~n", [N]),
    forall(member(F, Findings), report_one_finding(F)).

report_one_finding(finding(Name-Outcome)) :-
    format("~n--- ~w ---~n", [Name]),
    report_outcome(Outcome).

report_outcome(artifact_diff(Diffs)) :-
    forall(member(D, Diffs), report_diff(D)).
report_outcome(original_or_renamed_error(O, R)) :-
    format("  original error: ~q~n", [O]),
    format("  renamed error:  ~q~n", [R]).
report_outcome(Other) :-
    format("  ~q~n", [Other]).

report_diff(same(_)).
report_diff(diff(Kind, LA, LB, OEx, REx)) :-
    format("  DIFF ~w (len orig=~w renamed=~w)~n", [Kind, LA, LB]),
    format("    ORIG: ~s~n", [OEx]),
    format("    RENM: ~s~n", [REx]).
