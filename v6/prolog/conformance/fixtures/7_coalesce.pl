% fixtures/7_coalesce.pl : `coalesce/2`, the use-site total read.
%
% RULING null_design = get_else_use_site_never_storage (conformance/rulings.pl;
% ARCH task get_else_wiring; design record plans/2026-07-30-option-versus-null-
% lab.md section 5, candidate D). Null never enters storage or the type system.
% Absence stays row absence. A query clause over a missing row drops the whole
% tuple -- an inner-join effect nobody wants -- so the consumer that wants a
% TOTAL answer spells the default itself, at the use site:
%
%   repo_latest(Name, Commit) <-
%     repo(Name),
%     coalesce(latest_commit(Name, Commit), 'absent').
%
% Datomic's `get-else` is the prior art. The WORD is `coalesce` because the
% language vocabulary law admits only rxjs, prolog or SQL words and COALESCE is
% SQL's name for exactly these semantics.
%
% WHAT THESE FIXTURES ARE FOR. The construct is a COMPILE-TIME DESUGAR
% (0_coalesce_expand.pl, expansion phase 45, consumed by BOTH doors): one rule
% becomes two ordinary clauses, the read and `not(...)` plus a `:=` of the
% default. The claim that it therefore inherits every shipped gate -- the
% incremental delta path, the negation path's retraction flip, the naive
% referee -- is a claim, so it is GRADED here rather than asserted:
%
%   coalesce_defaults_the_absent_row   present and absent rows in ONE answer.
%   coalesce_default_returns_when_source_retracts
%                                      THE RECEIPT THAT MATTERS: the source row
%                                      arrives and then leaves, and the
%                                      defaulted row comes BACK. A desugar that
%                                      did not ride the incremental negation
%                                      path would strand the real value.
%   coalesce_over_derived_source       the source is a level view, not an EDB
%                                      rel, and the default is an INT.
%   coalesce_in_edge_body_samples      the edge arm is latest(...), not a bare
%                                      atom. A bare atom in an edge body is a
%                                      TRIGGER, so the naive desugar would turn
%                                      an optional lookup into a second firing
%                                      source; graded by the tick count.
%
% and four named refusals, each thrown by the shared expander so the oracle and
% the compiler cannot disagree about which programs are legal (the single-door
% refusal is this repo's recurring defect class).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ (a) present and absent in one answer ═══════════════════════════════════
% `alpha` has a commit; `beta` does not. Both rows survive, which is the whole
% point: without coalesce the `beta` tuple would drop out of the join.

fixture(coalesce_defaults_the_absent_row,
  prog([ col_type(repo/1, name, text),
         col_type(latest_commit/2, name, text),
         col_type(latest_commit/2, commit, text),
         col_type(repo_latest/2, name, text),
         col_type(repo_latest/2, commit, text) ],
       [ (repo_latest(Name, Commit) <-
              repo(Name),
              coalesce(latest_commit(Name, Commit), 'absent')) ]),
  [ repo('alpha'),
    repo('beta'),
    latest_commit('alpha', 'sha-1') ],
  [],
  [ final(repo_latest/2, [ repo_latest('alpha', 'sha-1'),
                           repo_latest('beta',  'absent') ]) ]).

% ═══ (b) the retraction flip ════════════════════════════════════════════════
% Tick 1: no commit, so the default. Tick 2: the commit arrives and REPLACES
% the default. Tick 3: the commit is retracted and the default COMES BACK.
%
% This is the receipt for the desugar riding the shipped negation path in both
% directions. The absent arm's `not(latest_commit(Name, _))` has to go false on
% tick 2 and true again on tick 3, and the delta list is where a desugar that
% only ever ADDED rows would show up.

fixture(coalesce_default_returns_when_source_retracts,
  prog([ col_type(repo/1, name, text),
         col_type(latest_commit/2, name, text),
         col_type(latest_commit/2, commit, text),
         col_type(repo_latest/2, name, text),
         col_type(repo_latest/2, commit, text) ],
       [ (repo_latest(Name, Commit) <-
              repo(Name),
              coalesce(latest_commit(Name, Commit), 'absent')) ]),
  [ repo('alpha') ],
  [ [ +latest_commit('alpha', 'sha-1') ],
    [ -latest_commit('alpha', 'sha-1') ] ],
  [ deltas(repo_latest/2,
      [ [ -repo_latest('alpha', 'absent'), +repo_latest('alpha', 'sha-1') ],
        [ -repo_latest('alpha', 'sha-1'),  +repo_latest('alpha', 'absent') ] ]),
    final(repo_latest/2, [ repo_latest('alpha', 'absent') ]) ]).

% ═══ (c) a DERIVED source, and an int default ═══════════════════════════════
% `heavy` is a level view over `pick`, so the coalesce reads a rel no arrival
% ever writes; stratification has to place `heavy` strictly below `report`
% because the absent arm negates it. The default is an INT, and it stays a
% variable bound by `:=` rather than a literal spliced into the head -- a bare
% integer literal in a head column is read positionally by the emitter's
% refCount GROUP BY (ARCH emitter_groupby_literal), and this spelling never
% goes near it.

fixture(coalesce_over_derived_source,
  prog([ col_type(tree/1, tree_id, int),
         col_type(pick/2, tree_id, int),
         col_type(pick/2, kilos, int),
         col_type(heavy/2, tree_id, int),
         col_type(heavy/2, kilos, int),
         col_type(report/2, tree_id, int),
         col_type(report/2, kilos, int) ],
       [ (heavy(TreeId, Kilos) <- pick(TreeId, Kilos), Kilos > 10),
         (report(TreeId, Kilos) <-
              tree(TreeId),
              coalesce(heavy(TreeId, Kilos), 0)) ]),
  [ tree(1), tree(2), tree(3),
    pick(1, 40),
    pick(2, 4) ],
  [],
  [ final(heavy/2,  [ heavy(1, 40) ]),
    final(report/2, [ report(1, 40),
                      report(2, 0),
                      report(3, 0) ]) ]).

% ═══ (d) an EDGE body: the arm is latest(...), never a bare atom ════════════
% A bare relation atom in an edge body is a TRIGGER, an occurrence. If the
% present arm desugared to the bare atom, an arrival on `name` would fire this
% rule on its own and label a tree nothing pinged. The expander emits
% latest(...) instead (the same split 0_relation_edge_expand.pl makes, for the
% same reason), and tick 2 below is what grades it: a `name` arrival lands with
% NO ping, and `labelled` stays silent.

fixture(coalesce_in_edge_body_samples,
  % Decl ORDER is per-rel, col_type before kind before keep, which is the order
  % parse_dl.pl reconstructs from `rel r(cols) log keep(all).` -- the roundtrip
  % grades =@= over the whole prog term, so a list carrying the same decls in a
  % different order reads as not_variant.
  prog([ col_type(ping/1, tree_id, int),
         kind(ping/1, log), keep(ping/1, all),
         col_type(labelled/2, tree_id, int),
         col_type(labelled/2, label, text),
         kind(labelled/2, log), keep(labelled/2, all),
         col_type(name/2, tree_id, int),
         col_type(name/2, label, text) ],
       [ (labelled(TreeId, Label) <+
              ping(TreeId),
              coalesce(name(TreeId, Label), 'unnamed')) ]),
  [ name(1, 'oak') ],
  [ [ +ping(1), +ping(2) ],
    [ +name(2, 'elm') ],
    [ +ping(2) ] ],
  [ deltas(labelled/2,
      [ [ +labelled(1, 'oak'), +labelled(2, 'unnamed') ],
        [],
        [ +labelled(2, 'elm') ],
        [] ]),
    final(labelled/2, [ labelled(1, 'oak'),          % msort order, not tick order
                        labelled(2, 'elm'),
                        labelled(2, 'unnamed') ]) ]).

% ═══ named refusals ═════════════════════════════════════════════════════════
% All four are thrown by 0_coalesce_expand.pl, the ONE expansion both the
% oracle and the compiler consult, so neither door can accept what the other
% refuses.

% Every variable in the source atom is already bound by the rest of the body,
% so there is no column for the default to land in and the goal is a membership
% test wearing a default.
fixture(coalesce_without_an_output_is_refused,
  prog([ col_type(repo/1, name, text) ],
       [ (known(Name) <- repo(Name), coalesce(archived(Name), 'absent')) ]),
  [],
  [],
  [ throws(unsupported_construct(coalesce_no_output(archived/1))) ]).

% Two unbound columns. A default is ONE value; which column it lands in would
% be a guess, so it is refused rather than guessed.
fixture(coalesce_with_two_outputs_is_refused,
  prog([ col_type(repo/1, name, text) ],
       [ (pair(Name, Commit, Author) <-
              repo(Name),
              coalesce(commit_by(Name, Commit, Author), 'absent')) ]),
  [],
  [],
  [ throws(unsupported_construct(coalesce_multiple_outputs(commit_by/3, 2))) ]).

% A VARIABLE default would reintroduce exactly the unbound hole the ruling
% exists to prevent, so the default must be a literal value.
fixture(coalesce_with_a_variable_default_is_refused,
  prog([ col_type(repo/1, name, text) ],
       [ (repo_latest(Name, Commit) <-
              repo(Name),
              fallback(Fallback),
              coalesce(latest_commit(Name, Commit), Fallback)) ]),
  [],
  [],
  [ throws(unsupported_construct(
             coalesce_default_not_literal(latest_commit/2, variable))) ]).

% A coalesce that is not on the conjunction spine. This refusal is what keeps
% the construct from ever reaching analyze.pl, where the live registry row's
% refs_of_arg role would read the source atom as an ordinary join and drop the
% default in SILENCE.
fixture(coalesce_under_negation_is_refused,
  prog([ col_type(repo/1, name, text) ],
       [ (odd(Name) <-
              repo(Name),
              not(coalesce(latest_commit(Name, _Commit), 'absent'))) ]),
  [],
  [],
  [ throws(unsupported_construct(coalesce_not_top_level(latest_commit/2))) ]).
