% fixtures/door_split_trigger_literal.pl : one shape, two doors, two answers.
%
% fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations)

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ A DOOR SPLIT ON PROGRAM ADMISSION, pinned rather than resolved ═════════
%
% The shape is a LITERAL in an EDGE TRIGGER argument position. The two doors
% disagree on whether the program may be written at all:
%
%   oracle    RUNS it. The literal filters occurrences: only a `resp` row whose
%             status column unifies with 200 fires the rule, so the 304 that
%             follows moves nothing and the keyed latch keeps its row. This
%             fixture grades that half.
%   compiler  REFUSES it by name, unsupported_construct(trigger_arg_not_var(200)),
%             thrown by lower.pl:compile_trigger_bound/4. analyze.pl's
%             front gate passes the program first; the refusal is a lowering-time
%             one, which is why nothing earlier in the compiler reports it.
%
% MEASURED on base 1e7b6843, one program text, both doors:
%
%   oracle    {"tick":1,"deltas":{"cache_view":{"add":[["repo","tag-v1",17]],"del":[]},
%                                 "resp":{"add":[["repo",200,"tag-v1",17]],"del":[]}}}
%             {"tick":2,"deltas":{"resp":{"add":[["repo",304,"",0]],"del":[]}}}
%   compiler  lower_program THREW: unsupported_construct(trigger_arg_not_var(200))
%
% Programs therefore exist that GRADE on one door and cannot compile on the
% other. Closing the split is a ruling the user owns, and it runs either way:
% widen the compiler to lower a trigger literal as a column filter, or narrow
% the oracle to refuse the same shape (as the corpus already does elsewhere for
% shapes the two engines answer differently, 6_relation_depth.pl). Nothing here
% picks. Origin: plans/2026-08-04-ghcacher-plan.md section 1.1, where this shape
% was the first fix attempt for the 304-empties-the-cache defect and was
% abandoned for a two-rule form precisely because it would not compile.
%
% The compiler half needs no fixture of its own: the sweep manifest carries this
% same name in the `unsupported` bucket with reason trigger_arg_not_var(200),
% alongside the three shapes already there (scope_done_three_spellings and the
% two state_machine.pl compound-trigger fixtures).
%
% RED RECEIPT, taken with the fixture in place and the expectations spelled as
% though the 304 also fired the rule:
%
%   fail  edge_trigger_literal_filters_on_the_oracle_door
%         got [[+cache_view(repo,'tag-v1',17)],[]]
%         want [[+cache_view(repo,'tag-v1',17)],
%               [-cache_view(repo,'tag-v1',17),+cache_view(repo,'',0)]]
%
% The `resp` deltas below are part of the grade, not decoration: they prove the
% 304 row DID arrive and was seen at the boundary, so the quiet tick 2 is the
% literal filtering occurrences rather than an arrival that never landed.
fixture(edge_trigger_literal_filters_on_the_oracle_door,
  prog([ kind(resp/4, log), keep(resp/4, all),
         keyed(cache_view/3, [1]) ],
       [ (cache_view(Endpoint, Tag, Stars) <+ resp(Endpoint, 200, Tag, Stars)) ]),
  [],
  [ [ +resp(repo, 200, 'tag-v1', 17) ],
    [ +resp(repo, 304, '', 0) ] ],
  [ deltas(resp/4, [ [ +resp(repo, 200, 'tag-v1', 17) ],
                     [ +resp(repo, 304, '', 0) ] ]),
    deltas(cache_view/3, [ [ +cache_view(repo, 'tag-v1', 17) ], [] ]),
    final(cache_view/3, [ cache_view(repo, 'tag-v1', 17) ]),
    ticks(2) ]).
