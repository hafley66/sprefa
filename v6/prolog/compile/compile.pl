% compile.pl : Phase B entry point. Reads a fixture(Name, prog(Decls, Rules),
% Initial, Schedule, Expectations) fact DIRECTLY off the fixture source file
% via read_term/3 with variable_names/1, never through consult, so the
% surface Prolog variable names (SessionId, RouteId, ...) survive as real
% term identity (shared across the WHOLE clause, since one fixture is one
% Prolog fact and same-spelled variables inside one clause are the same
% variable object). analyze.pl:rel_columns/4 mines column names from that
% identity. Consulting the fixture the normal way loses the names the moment
% the reader returns; that is the reason this file exists instead of just
% calling `user:fixture/5`.
%
% Emit:  swipl -q -l v6/prolog/compile/compile.pl \
%          -g "compile_fixture(switch_as_keyed_replace, 'v6/prolog/conformance/fixtures/scopes.pl', 'v6/prolog/compile/out/switch_as_keyed_replace.ts')" \
%          -g halt
%
% BACKEND-PLUGGABLE (user directive, mid-arc: the compile technique will
% later also target Rust, so nothing above the emitter may become 1:1 with
% TypeScript). analyze.pl, strat.pl and lower.pl never mention a host
% language; program_plan/2 and lower_program/2 produce plan/6 and lowered/8,
% both plain SQL text + Prolog structure. `compile_fixture/3` below defaults
% to the emit_ts.pl backend; `compile_fixture/4` takes an explicit
% Module:Predicate emitter (called as `call(Emitter, Name, Plan, Lowered,
% BootStatements, Text)`) so a future emit_rust.pl plugs in without
% touching anything upstream of it -- same plan term, different renderer.

:- module(compile,
          [ read_fixture_term/4,
            compile_fixture/3,
            compile_fixture/4,
            compile_dl6/2,
            compile_program/6,
            program_plan/2
          ]).

:- use_module(library(lists)).
:- use_module('../0_enum_expand', [expand_enum_program/2]).
:- use_module(analyze).
:- use_module(strat).
:- use_module(lower).
:- use_module(emit_ts).
:- use_module(parse_dl, [parse_dl_file/4]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ reading ═════════════════════════════════════════════════════════════════

read_fixture_term(File, Name, Term, Bindings) :-
    open(File, read, Stream),
    call_cleanup(find_fixture(Stream, Name, Term, Bindings), close(Stream)).

% Operators declared with op/3 inside a module are local to THAT module's own
% clause parsing; they do not carry over to a raw read_term/3 stream read.
% Every fixture file opens with its own `:- op(...)` directives (FIXTURES.md:
% "copy them from fixtures/merge_family.pl"), which normal consult would
% execute in sequence, making <-/<+/:= visible for the REST of that same
% file. This reader replays that: a directive term is CALLED, exactly what
% consult does, so the file's own operator declarations take effect for
% later terms in the same scan. compile.pl's own top-of-file op/3 lines are
% therefore redundant with this replay but kept for readability/IDE parsing.
find_fixture(Stream, Name, Term, Bindings) :-
    read_term(Stream, Candidate, [variable_names(CandidateBindings)]),
    ( Candidate == end_of_file
    -> throw(fixture_not_found(Name))
    ; Candidate = (:- Directive)
    -> call(Directive), find_fixture(Stream, Name, Term, Bindings)
    ; Candidate = fixture(Name, _, _, _, _)
    -> Term = Candidate, Bindings = CandidateBindings
    ; find_fixture(Stream, Name, Term, Bindings)
    ).

% ═══ the compile plan : everything lower.pl and emit_ts.pl need, computed
% once so both stay pure functions of it rather than re-deriving it ═════════
%
% plan(Name, Prog, RelPlans, ArrivalTargets, RuleOrder, EdgeRules)
%   RelPlans: list of relplan(Ref, Kind, Columns, KeyPositionsOrNone,
%             ColumnTypes) covering every ref program_refs/2 or typed
%             declaration finds (arrival
%             targets and derived rels alike -- the tick log envelope
%             reports both). ColumnTypes (PHASE C2 RULING 1) is int|text per
%             Columns position, inferred by analyze.pl:rel_column_types/5
%             from declaration types when present, otherwise from the
%             fixture's own literal values -- lower.pl:column_def/3 is the
%             only SQL storage reader.
%   RuleOrder: level rules in strat.pl:sql_rule_order/2 order.
%   EdgeRules: edge rules, program order (engine.pl tries edge rules in
%              program order for each occurrence; with at most one edge rule
%              per target fixture this is a formality kept for generality).

program_plan(fixture(Name, SugaredProg, Initial, Schedule, _Expectations)-Bindings, Plan) :-
    expand_enum_program(SugaredProg, Prog),
    Prog = prog(Decls, Rules),
    check_supported_subset(Prog),
    % Union rule-derived refs with EVERY declared ref (analyze.pl:
    % declared_refs/2's header comment) -- a kind(Ref, _) decl that no rule
    % ever mentions is still a real rel a schedule can write, and must still
    % get a table + arrival handling in the emitted program.
    program_refs(Rules, RuleRefs),
    declared_refs(Decls, DeclaredRefs),
    append(RuleRefs, DeclaredRefs, AllRefs0), sort(AllRefs0, AllRefs),
    derived_refs(Rules, DerivedRefs),
    subtract(AllRefs, DerivedRefs, ArrivalTargets),
    findall(relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes),
            ( member(Ref, AllRefs),
              rel_kind(Decls, Ref, Kind),
              rel_columns(Decls, Rules, Bindings, Ref, Columns),
              rel_column_types(Decls, Rules, Initial, Schedule, Bindings, Ref, ColumnTypes),
              ( decl_key(Decls, Ref, Positions) -> KeyOrNone = key(Positions) ; KeyOrNone = none )
            ), RelPlans),
    % PHASE C2 RULING 1 x RULING 2: this needs RelPlans (ColumnTypes), so it
    % runs here rather than inside check_supported_subset/1 above (which
    % runs before RelPlans exists).
    check_edge_head_column_types(RelPlans, Rules),
    sql_rule_order(Rules, RuleOrder),
    include(rule_is_edge, Rules, EdgeRules),
    Plan = plan(Name, Prog, RelPlans, ArrivalTargets, RuleOrder, EdgeRules).

% ═══ top level ═══════════════════════════════════════════════════════════════

compile_fixture(Name, FixtureFile, OutFile) :-
    compile_fixture(Name, FixtureFile, OutFile, emit_ts:emit_program).

compile_fixture(Name, FixtureFile, OutFile, Emitter) :-
    read_fixture_term(FixtureFile, Name, Term, Bindings),
    Term = fixture(Name, _Prog, Initial, _Schedule, _Expectations),
    compile_program(Name, Term, Bindings, Initial, OutFile, Emitter).

compile_dl6(File, OutFile) :-
    parse_dl_file(File, Prog, Bindings, Findings),
    ( Findings == []
    -> true
    ; throw(unsupported_construct(surface_findings(Findings)))
    ),
    file_base_name(File, BaseName),
    file_name_extension(Name, _Extension, BaseName),
    compile_program(Name, fixture(Name, Prog, [], [], []), Bindings,
                    [], OutFile, emit_ts:emit_program).

compile_program(Name, Term, Bindings, Initial, OutFile, Emitter) :-
    program_plan(Term-Bindings, Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, _, RelPlans, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(RelPlans, Initial, LevelStatements, BootStatements),
    call(Emitter, Name, Plan, Lowered, BootStatements, Text),
    setup_call_cleanup(
        open(OutFile, write, Stream),
        format(Stream, "~s", [Text]),
        close(Stream)),
    format("wrote ~w~n", [OutFile]).
