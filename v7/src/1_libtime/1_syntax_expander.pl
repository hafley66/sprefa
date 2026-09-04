:- module(dl7_syntax_expander, [expand_syntax/5]).

:- use_module(library(error), [must_be/2]).
:- use_module('0a_syntax_macro_program',
              [ macro_protocol/3,
                evaluate_macro_program/5,
                macro_results/6
              ]).
:- use_module('0b_syntax_rewriter',
              [active_nodes/2, rewrite_active_graph/8]).

%% expand_syntax(+SyntaxGraphRows, +MacroProgram,
%%               -ExpandedRows, -ProvenanceRows, -Diagnostics) is det.
%
% Evaluate an already-checked DL7 program over the active syntax graph. The
% program claims an invocation with syntax_claim/2 and emits zero or more
% ordered :/4 edges labeled expansion. Each round rewrites the active frontier
% and ordinary item edges, then evaluates again until no active node is claimed.
expand_syntax(SyntaxGraphRows, MacroProgram,
              ExpandedRows, ProvenanceRows, Diagnostics) :-
    must_be(list, SyntaxGraphRows),
    must_be(ground, SyntaxGraphRows),
    must_be(ground, MacroProgram),
    macro_protocol(MacroProgram, Protocol, ProtocolDiagnostics),
    expand_after_protocol(ProtocolDiagnostics, Protocol,
                          SyntaxGraphRows, MacroProgram,
                          ExpandedRows, ProvenanceRows, Diagnostics).

expand_after_protocol([], Protocol, Rows, Program,
                      Expanded, Provenance, Diagnostics) :-
    !,
    sort(Rows, CanonicalRows),
    expand_rounds(0, 64, [CanonicalRows], Protocol, Program, CanonicalRows,
                  [], Expanded, Provenance, Diagnostics).
expand_after_protocol(Diagnostics, _, _, _, [], [], Diagnostics).

expand_rounds(Wave, Limit, _, _, _, _, _, [], [],
              [diagnostic(macrotime, none,
                          expansion_round_limit(Limit))]) :-
    Wave >= Limit,
    !.
expand_rounds(Wave, Limit, Seen, Protocol, Program, Rows, Provenance0,
              Expanded, Provenance, Diagnostics) :-
    evaluate_macro_program(Protocol, Program, Rows,
                           Closure, EvaluationDiagnostics),
    continue_expansion(EvaluationDiagnostics, Closure,
                       Wave, Limit, Seen, Protocol, Program, Rows,
                       Provenance0, Expanded, Provenance, Diagnostics).

continue_expansion([], Closure, Wave, Limit, Seen, Protocol, Program, Rows,
                   Provenance0, Expanded, Provenance, Diagnostics) :-
    !,
    active_nodes(Rows, ActiveNodes),
    macro_results(Closure, Protocol, ActiveNodes,
                  AvailableRows, Claims, Outputs),
    (   Claims == []
    ->  Expanded = Rows,
        sort(Provenance0, Provenance),
        Diagnostics = []
    ;   rewrite_active_graph(Rows, AvailableRows, Claims, Outputs, Wave,
                             NextRows, WaveProvenance, RewriteDiagnostics),
        continue_rewritten_graph(
            RewriteDiagnostics, NextRows, WaveProvenance,
            Wave, Limit, Seen, Protocol, Program, Provenance0,
            Expanded, Provenance, Diagnostics)
    ).
continue_expansion(Diagnostics, _, _, _, _, _, _, _, _,
                   [], [], Diagnostics).

continue_rewritten_graph([], NextRows, WaveProvenance,
                         Wave, Limit, Seen, Protocol, Program, Provenance0,
                         Expanded, Provenance, Diagnostics) :-
    !,
    (   memberchk(NextRows, Seen)
    ->  Expanded = [],
        Provenance = [],
        Diagnostics = [diagnostic(
                           macrotime, none,
                           expansion_cycle(Wave))]
    ;   append(Provenance0, WaveProvenance, Provenance1),
        NextWave is Wave + 1,
        expand_rounds(NextWave, Limit, [NextRows | Seen],
                      Protocol, Program, NextRows, Provenance1,
                      Expanded, Provenance, Diagnostics)
    ).
continue_rewritten_graph(Diagnostics, _, _, _, _, _, _, _, _,
                         [], [], Diagnostics).
