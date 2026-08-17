:- ensure_loaded('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab363771c9c609831/v6/prolog/conformance/ticklog.pl').
:- ensure_loaded('/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/9236bad9-763d-49b4-bcdf-000c5fa97d8d/scratchpad/disc_probe.pl').

probe :-
    forall(member(Name, [mutual_recursion_no_head_expression,
                         direct_recursion_with_head_expression]),
           ( format('### ~w~n', [Name]), emit(Name) )).
