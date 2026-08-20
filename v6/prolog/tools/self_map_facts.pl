% Self-map fact source. One path in, JSON lines of architecture facts out.
%
% Run:  swipl -q -l v6/prolog/tools/self_map_facts.pl -g 'emit_for(Path)' -g halt
%       (v6/tsv2/scripts/self-map.sh drives it through an `sh` host instead)
%
% ── why this file exists at all ──────────────────────────────────────────────
%
% The self-map program (v6/dl/fixtures/self-map.dl6) describes THIS compiler
% and THIS runtime, and the standing requirement is that it read machine-
% readable sources the repo already maintains rather than re-parsing prolog by
% hand. Four sources, each already the single authority for its own subject:
%
%   compile/registry.pl   surface/5, the construct inventory the parser,
%                         printer, analyzer and gate all project from
%   1_expansion.pl        expansion_phase/3, the declared sugar order
%   ARCH.pl               task/3, the build DAG with its dependency lists
%   any *.dl6             the compiler's OWN analysis of a program: rel kinds,
%                         rule-head origins, and body-read edges, straight out
%                         of analyze.pl -- the same predicates lowering uses
%
% So the facts are not a description of the compiler written beside it. They
% are the compiler's own tables, read by the compiler's own predicates.
%
% ── relation to tools/arch_map.pl ────────────────────────────────────────────
%
% arch_map.pl is prior art on the same instinct and stays as it is: it emits a
% d2 atlas module into ~/projects/anim, an EXTERNAL renderer, and reads the
% wider ARCH tables (graphs, algorithms, rulings, prior art). This file emits
% JSON lines for consumption BY A DL6 PROGRAM, in-repo, and reads four sources
% only. Neither replaces the other and neither shares code, because the shape
% of what they emit has nothing in common.
%
% ── output contract ──────────────────────────────────────────────────────────
%
% One JSON object per line. Every object carries `record`, and the key SETS are
% pairwise non-containing so a projection host declaring one set never picks up
% another record's line (v6/tsv2/serve/1_hosts.ts:decodeObjectItems drops any
% object missing a declared output column). The six shapes:
%
%   {"record":"surface",   "functor","arity","axis","status"}
%   {"record":"phase",     "phase_order","phase_name","expander"}
%   {"record":"task",      "task_name","state"}
%   {"record":"task_dep",  "task_name","dep_name"}
%   {"record":"rel",       "program","rel_name","rel_kind","origin"}
%   {"record":"rel_edge",  "program","from_rel","to_rel","sign","arrow"}
%
% Rows are SORTED before printing. The rail's byte-stability receipt (two runs
% of `just self-map` diff clean) rests on that, and on nothing else -- prolog's
% clause order is stable but the dl6 engine's row order is not a guarantee this
% file may lean on.
%
% ── loading is per-dispatch, deliberately ────────────────────────────────────
%
% ARCH.pl consults src/kernel.pl and conformance/rulings.pl; the dl6 door pulls
% the whole compiler front. Loading all of them in one process to answer one
% path would multiply the failure surface for no gain, so each dispatch loads
% only its own source. `emit_for/1` is therefore the only entry point and it
% dispatches on the path's base name.

:- module(self_map_facts, [ emit_for/1, main/0 ]).

:- use_module(library(lists)).

:- dynamic(tools_dir_fact/1).
:- prolog_load_context(directory, Here), assertz(tools_dir_fact(Here)).

source_path(Relative, Absolute) :-
    tools_dir_fact(ToolsDir),
    atomic_list_concat([ToolsDir, '/', Relative], Absolute).

% ── entry ────────────────────────────────────────────────────────────────────
%
% `main/0` is the entry the `sh` host uses, and the path arrives in argv rather
% than spliced into a `-g` goal on purpose: a goal string would put the path
% through the shell's quoting AND prolog's reader, two escaping layers for a
% value the host template already quotes once. argv crosses both untouched.
%
% prolog-lint reports `unused_export_candidate main/0` and that report is
% CORRECT AND EXPECTED -- step 8 is advisory precisely because a `-g` entry is
% indistinguishable from a dead export at the source level (prolog_lint.pl says
% so in its own header). The caller is the `sh` template in
% v6/dl/fixtures/self-map.dl6. Do not delete this predicate to quiet the
% advisory; deleting it breaks the rail and nothing gated would notice.

main :-
    current_prolog_flag(argv, Argv),
    (   Argv = [Path]
    ->  emit_for(Path)
    ;   throw(error(self_map_expects_one_path(Argv), _))
    ).

emit_for(Path) :-
    file_base_name(Path, BaseName),
    file_name_extension(_, Extension, BaseName),
    (   Extension == dl6
    ->  emit_program_facts(Path)
    ;   emit_source_facts(BaseName)
    ).

emit_source_facts('registry.pl')    :- !, emit_surface_facts.
emit_source_facts('1_expansion.pl') :- !, emit_phase_facts.
emit_source_facts('ARCH.pl')        :- !, emit_task_facts.
% A path the rail asked for and this file has no reading of is a NAMED silence,
% not an empty answer: the host would otherwise report zero rows and look like
% a source that legitimately has no facts.
emit_source_facts(Other) :-
    throw(error(self_map_unknown_source(Other), _)).

% ── registry.pl : the construct inventory ────────────────────────────────────

emit_surface_facts :-
    source_path('../compile/registry.pl', RegistryPath),
    use_module(RegistryPath, [surface/5]),
    findall(row(FunctorText, ArityText, Axis, Status),
            ( surface(Functor/Arity, Axis, _AnalyzeRole, _LowerRole, Status),
              term_text(Functor, FunctorText),
              term_text(Arity, ArityText)
            ),
            Rows0),
    sort(Rows0, Rows),
    forall(member(row(FunctorText, ArityText, Axis, Status), Rows),
           print_json([ 'record'-'surface',
                        'functor'-FunctorText,
                        'arity'-ArityText,
                        'axis'-Axis,
                        'status'-Status ])).

% ── 1_expansion.pl : the declared sugar order ────────────────────────────────

emit_phase_facts :-
    source_path('../1_expansion.pl', ExpansionPath),
    use_module(ExpansionPath, [expansion_phase/3]),
    findall(row(Order, Name, ExpanderText),
            ( expansion_phase(Order, Name, Expander),
              term_text(Expander, ExpanderText)
            ),
            Rows0),
    sort(Rows0, Rows),
    forall(member(row(Order, Name, ExpanderText), Rows),
           print_json([ 'record'-'phase',
                        'phase_order'-Order,
                        'phase_name'-Name,
                        'expander'-ExpanderText ])).

% ── ARCH.pl : the build DAG ──────────────────────────────────────────────────
%
% task/3's third argument is the dependency LIST. It is flattened here into one
% task_dep row per edge, because a dl6 rel column holds one value and the join
% the map wants is edge-shaped anyway.

emit_task_facts :-
    source_path('../ARCH.pl', ArchPath),
    ensure_loaded(ArchPath),
    findall(row(Name, State), task(Name, State, _), TaskRows0),
    sort(TaskRows0, TaskRows),
    forall(member(row(Name, State), TaskRows),
           print_json([ 'record'-'task',
                        'task_name'-Name,
                        'state'-State ])),
    findall(edge(Name, Dep),
            ( task(Name, _, Dependencies), member(Dep, Dependencies) ),
            EdgeRows0),
    sort(EdgeRows0, EdgeRows),
    forall(member(edge(Name, Dep), EdgeRows),
           print_json([ 'record'-'task_dep',
                        'task_name'-Name,
                        'dep_name'-Dep ])).

% ── any *.dl6 : the compiler's own reading of a program ──────────────────────
%
% This is the deep one. `parse_dl_file/4` is the text door; `expand_program/3`
% runs the declared sugar phases so the facts describe what the compiler
% actually lowers rather than what the author typed; `analyze.pl`'s own
% predicates then answer rel kind, which refs a rule head writes, and which
% refs each body reads and how. No new analysis is written here -- if this
% disagrees with the emitted SQL, one of the two is a compiler bug, and that is
% the point of deriving it from the same predicates.
%
% ORIGIN, three values, straight out of the analyzer's own partition:
%   edge     headed by at least one edge rule (<+)
%   level    headed by at least one level rule (<-)
%   world    headed by no rule: an arrival target (a host answer, a bind, or a
%            posted EDB row)
% A rel headed by both kinds is reported `edge`, which matches the lowering's
% own precedence and is a shape the keyed_level_head unsupported construct already narrows.

emit_program_facts(Path) :-
    source_path('../compile/parse_dl_dcg.pl', ParsePath),
    source_path('../analyze.pl', AnalyzePath),
    source_path('../1_expansion.pl', ExpansionPath),
    source_path('../1_host_expand.pl', HostExpandPath),
    use_module(ParsePath, [parse_dl_file/4]),
    use_module(AnalyzePath,
               [ rel_kind/3, body_ref_uses/2, rule_head_ref/2,
                 rule_is_edge/1, edge_headed_refs/2, level_headed_refs/2,
                 program_refs/2, declared_refs/2 ]),
    use_module(ExpansionPath, [expand_program/3]),
    use_module(HostExpandPath, [prepare_program/5]),
    file_base_name(Path, BaseName),
    file_name_extension(Program, _, BaseName),
    parse_dl_file(Path, SurfaceProgram, _Bindings, Findings),
    (   Findings == []
    ->  true
    ;   throw(error(self_map_program_findings(Program, Findings), _))
    ),
    % The exact two-step compile.pl:program_plan/2 runs, and in its order: the
    % host pre-pass turns `sh`/`bind`/`?` declarations into ordinary demand and
    % response rels, THEN the declared sugar phases run. Skipping the pre-pass
    % is not an option here -- parse_dl_file/4 answers program/3, prog/2 is
    % what every analyze.pl predicate reads, and the host rels are exactly the
    % world-plane nodes the dataflow diagram exists to show.
    prepare_program(SurfaceProgram, HostProgram, _HostPlans, _BindPlans, _QueryPlans),
    expand_program(HostProgram, prog(Decls, Rules), _Context),
    program_refs(Rules, RuleRefs),
    declared_refs(Decls, DeclaredRefs),
    append(RuleRefs, DeclaredRefs, AllRefs0),
    sort(AllRefs0, AllRefs),
    edge_headed_refs(Rules, EdgeRefs),
    level_headed_refs(Rules, LevelRefs),
    findall(row(Name, Kind, Origin),
            ( member(Name/Arity, AllRefs),
              rel_kind(Decls, Name/Arity, Kind),
              ref_origin(Name/Arity, EdgeRefs, LevelRefs, Origin)
            ),
            RelRows0),
    sort(RelRows0, RelRows),
    forall(member(row(Name, Kind, Origin), RelRows),
           print_json([ 'record'-'rel',
                        'program'-Program,
                        'rel_name'-Name,
                        'rel_kind'-Kind,
                        'origin'-Origin ])),
    findall(edge(FromName, ToName, Sign, Arrow),
            ( member(Rule, Rules),
              rule_head_ref(Rule, ToName/_),
              ( rule_is_edge(Rule) -> Arrow = edge ; Arrow = level ),
              rule_body(Rule, Body),
              body_ref_uses(Body, Uses),
              member(use(FromName/_, _Args, Sign, _Marking), Uses)
            ),
            EdgeRows0),
    sort(EdgeRows0, EdgeRows),
    forall(member(edge(FromName, ToName, Sign, Arrow), EdgeRows),
           print_json([ 'record'-'rel_edge',
                        'program'-Program,
                        'from_rel'-FromName,
                        'to_rel'-ToName,
                        'sign'-Sign,
                        'arrow'-Arrow ])).

ref_origin(Ref, EdgeRefs, _, edge)  :- memberchk(Ref, EdgeRefs), !.
ref_origin(Ref, _, LevelRefs, level) :- memberchk(Ref, LevelRefs), !.
ref_origin(_, _, _, world).

% Matched by FUNCTOR NAME rather than by writing `(_ <- Body)`: `<-` and `<+`
% are operators the fixture files and parse_dl_dcg.pl declare for themselves, and
% this module deliberately declares none -- an `:- op/3` here would be a second
% place the arrow spellings live.
rule_body(Rule, Body) :-
    functor(Rule, Functor, 2),
    memberchk(Functor, ['<-', '<+']),
    !,
    arg(2, Rule, Body).
rule_body(_, true).

% ── printing ─────────────────────────────────────────────────────────────────
%
% Hand-rolled rather than library(http/json): the output is a flat object of
% atoms and integers, the escape set is four characters wide, and pulling the
% http bundle into a one-shot that a subprocess host spawns per file is cost
% with no benefit. `print_json/1` refuses anything but atom/integer values, so
% a future record shape carrying a compound cannot silently print as `foo(bar)`
% inside a JSON string.

print_json(Pairs) :-
    maplist(json_pair_text, Pairs, PairTexts),
    atomic_list_concat(PairTexts, ',', Body),
    format("{~w}~n", [Body]).

json_pair_text(Key-Value, Text) :-
    json_string(Key, KeyText),
    (   integer(Value)
    ->  format(atom(Text), '~w:~w', [KeyText, Value])
    ;   atom(Value)
    ->  json_string(Value, ValueText),
        format(atom(Text), '~w:~w', [KeyText, ValueText])
    ;   throw(error(self_map_unprintable_value(Key, Value), _))
    ).

json_string(Value, Text) :-
    atom_codes(Value, Codes),
    json_escape(Codes, Escaped),
    atom_codes(EscapedAtom, Escaped),
    format(atom(Text), '"~w"', [EscapedAtom]).

json_escape([], []).
json_escape([Code | Rest], Out) :-
    (   Code =:= 0'\\ -> Escaped = [0'\\, 0'\\]
    ;   Code =:= 0'"  -> Escaped = [0'\\, 0'"]
    ;   Code =:= 10   -> Escaped = [0'\\, 0'n]
    ;   Code =:= 13   -> Escaped = [0'\\, 0'r]
    ;   Code =:= 9    -> Escaped = [0'\\, 0't]
    ;   Escaped = [Code]
    ),
    json_escape(Rest, RestEscaped),
    append(Escaped, RestEscaped, Out).

% `~q` would quote an atom that needs it (`'2026-07-29'`), and the quotes would
% then land INSIDE the JSON string. Values here are identifiers and module-
% qualified predicate names, so plain `~w` is the reading that round-trips.
term_text(Term, Text) :- format(atom(Text), '~w', [Term]).
