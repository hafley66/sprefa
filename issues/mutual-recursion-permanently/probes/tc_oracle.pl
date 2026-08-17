:- ensure_loaded('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab363771c9c609831/v6/prolog/conformance/ticklog.pl').
:- ensure_loaded('/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/9236bad9-763d-49b4-bcdf-000c5fa97d8d/scratchpad/tc_probe.pl').

probe :-
    forall(member(Name, [tc_chain_batched_one_tick, tc_chain_one_edge_per_tick]),
           ( format('### ~w~n', [Name]), emit(Name) )).
