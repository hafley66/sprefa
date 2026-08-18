% resident_coroutine.pl : the resident-coroutine program's SEMANTICS, the same
% rules v6/dl/fixtures/resident-coroutine.dl6 spells.
%
% One source session's turns fold into same-role runs, consecutive runs pair
% into bundles, each bundle asks the resident once, and the resident's reply
% arrives as an ordinary arrival into a base rel that no rule heads.
%
% Two spelling adjustments from the plan's dl6 text, both forced:
%   * `run_said` materializes the three-way join the concat groups over. A head
%     aggregate groups over ONE body atom (ARCH.pl:32-35); taking `role` from
%     `run_start` while the delta arm stands on `turn` raises
%     aggregate_group_not_delta_local (lower.pl:4441).
%   * the resident's reply columns are spelled out rather than written behind a
%     result arrow. `->` in a rel declaration takes one type expression and
%     appends one `return` column (parse_dl_dcg.pl:509-521), so a two-column
%     output has no arrow spelling; the explicit columns are what the sugar
%     would have produced anyway.
%
% Turns 1..6 carry roles user, assistant, assistant, user, user, assistant, so
% the runs are {1}, {2,3}, {4,5}, {6} and the only assistant-then-user pair is
% run 2 into run 4.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

% ═══ the standing demand ════════════════════════════════════════════════════
% Nothing writes `resident` here, so `handled` stays empty: the ask is standing
% and unanswered.

fixture(resident_coroutine_runs_bundle_into_one_ask,
  prog([],
       [ ( prev_same_role(Session, TurnNumber) <-
             turn(Session, TurnNumber, _, Role, _),
             turn(Session, PrevNumber, _, Role, _),
             PrevNumber == TurnNumber - 1 ),
         ( run_start(Session, TurnNumber, Role) <-
             turn(Session, TurnNumber, _, Role, _),
             not(prev_same_role(Session, TurnNumber)) ),
         ( later_start_between(Session, RunTurn, TurnNumber) <-
             run_start(Session, RunTurn, _),
             run_start(Session, LaterTurn, _),
             turn(Session, TurnNumber, _, _, _),
             RunTurn < LaterTurn,
             LaterTurn =< TurnNumber ),
         ( run_member(Session, RunTurn, TurnNumber) <-
             run_start(Session, RunTurn, _),
             turn(Session, TurnNumber, _, _, _),
             RunTurn =< TurnNumber,
             not(later_start_between(Session, RunTurn, TurnNumber)) ),
         ( run_said(Session, RunTurn, Role, TurnNumber, Said) <-
             run_start(Session, RunTurn, Role),
             run_member(Session, RunTurn, TurnNumber),
             turn(Session, TurnNumber, _, _, Said) ),
         ( run(Session, RunTurn, Role, group_concat(Said, "\n", TurnNumber)) <-
             run_said(Session, RunTurn, Role, TurnNumber, Said) ),
         ( run_between(Session, AiRun, UserRun) <-
             run_start(Session, AiRun, _),
             run_start(Session, UserRun, _),
             run_start(Session, MidRun, _),
             AiRun < MidRun,
             MidRun < UserRun ),
         ( bundle(Session, AiRun, UserRun, AiText, UserText) <-
             run(Session, AiRun, assistant, AiText),
             run(Session, UserRun, user, UserText),
             AiRun < UserRun,
             not(run_between(Session, AiRun, UserRun)) ),
         ( resident_ask(Session, UserRun, Prompt) <-
             bundle(Session, _, UserRun, AiText, UserText),
             Prompt := concat(['<ai>\n', AiText, '\n</ai>\n<user>\n',
                               UserText, '\n</user>']) ),
         ( handled(Session, UserRun) <-
             resident(Session, UserRun, _, _) ) ]),
  [],
  [ [ +turn(s, 1, 101, user,      'hi'),
      +turn(s, 2, 102, assistant, 'one'),
      +turn(s, 3, 103, assistant, 'two'),
      +turn(s, 4, 104, user,      'more'),
      +turn(s, 5, 105, user,      'please'),
      +turn(s, 6, 106, assistant, 'done') ] ],
  [ final(run/4, [ run(s, 1, user,      'hi'),
                   run(s, 2, assistant, 'one\ntwo'),
                   run(s, 4, user,      'more\nplease'),
                   run(s, 6, assistant, 'done') ]),
    final(bundle/5, [ bundle(s, 2, 4, 'one\ntwo', 'more\nplease') ]),
    final(resident_ask/3,
          [ resident_ask(s, 4,
              '<ai>\none\ntwo\n</ai>\n<user>\nmore\nplease\n</user>') ]),
    final(handled/2, []) ]).

% ═══ the reply ══════════════════════════════════════════════════════════════
% Same program, one more tick: the reply arrives as a plain arrival into
% `resident`, the door a host executor's rows already come through, and
% `handled` answers.

fixture(resident_reply_arrives_as_a_plain_arrival,
  prog([],
       [ ( prev_same_role(Session, TurnNumber) <-
             turn(Session, TurnNumber, _, Role, _),
             turn(Session, PrevNumber, _, Role, _),
             PrevNumber == TurnNumber - 1 ),
         ( run_start(Session, TurnNumber, Role) <-
             turn(Session, TurnNumber, _, Role, _),
             not(prev_same_role(Session, TurnNumber)) ),
         ( later_start_between(Session, RunTurn, TurnNumber) <-
             run_start(Session, RunTurn, _),
             run_start(Session, LaterTurn, _),
             turn(Session, TurnNumber, _, _, _),
             RunTurn < LaterTurn,
             LaterTurn =< TurnNumber ),
         ( run_member(Session, RunTurn, TurnNumber) <-
             run_start(Session, RunTurn, _),
             turn(Session, TurnNumber, _, _, _),
             RunTurn =< TurnNumber,
             not(later_start_between(Session, RunTurn, TurnNumber)) ),
         ( run_said(Session, RunTurn, Role, TurnNumber, Said) <-
             run_start(Session, RunTurn, Role),
             run_member(Session, RunTurn, TurnNumber),
             turn(Session, TurnNumber, _, _, Said) ),
         ( run(Session, RunTurn, Role, group_concat(Said, "\n", TurnNumber)) <-
             run_said(Session, RunTurn, Role, TurnNumber, Said) ),
         ( run_between(Session, AiRun, UserRun) <-
             run_start(Session, AiRun, _),
             run_start(Session, UserRun, _),
             run_start(Session, MidRun, _),
             AiRun < MidRun,
             MidRun < UserRun ),
         ( bundle(Session, AiRun, UserRun, AiText, UserText) <-
             run(Session, AiRun, assistant, AiText),
             run(Session, UserRun, user, UserText),
             AiRun < UserRun,
             not(run_between(Session, AiRun, UserRun)) ),
         ( resident_ask(Session, UserRun, Prompt) <-
             bundle(Session, _, UserRun, AiText, UserText),
             Prompt := concat(['<ai>\n', AiText, '\n</ai>\n<user>\n',
                               UserText, '\n</user>']) ),
         ( handled(Session, UserRun) <-
             resident(Session, UserRun, _, _) ) ]),
  [],
  [ [ +turn(s, 1, 101, user,      'hi'),
      +turn(s, 2, 102, assistant, 'one'),
      +turn(s, 3, 103, assistant, 'two'),
      +turn(s, 4, 104, user,      'more'),
      +turn(s, 5, 105, user,      'please'),
      +turn(s, 6, 106, assistant, 'done') ],
    [ +resident(s, 4, 9, 'reply') ] ],
  [ final(resident_ask/3,
          [ resident_ask(s, 4,
              '<ai>\none\ntwo\n</ai>\n<user>\nmore\nplease\n</user>') ]),
    final(handled/2, [ handled(s, 4) ]) ]).
