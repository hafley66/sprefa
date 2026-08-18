% fixtures/7_module_path_wrapper.pl : a module path in FUNCTOR position inside
% a surface wrapper.
%
% A dotted rel path is already a compiled body atom
% (compile/out/manifest.json, module_path_in_body_reads_the_flat_rel,
% bucket compiled). The wrapper wraps that same atom, so it takes the same
% dotted_path//1 plus path_atom/4 the bare position takes; the resolution
% these fixtures grade is 0_dot_expand.pl:rewrite_rel_paths/3, which already
% walks into any compound.
%
% FAIL-FIRST RECEIPT, text door, before parse_dl_dcg.pl:rel_atom_term//1 took
% dotted_path//1 (probes/2026-08-17-stress/run.sh, audit F2):
%
%   p02_modulepath_x_pre        crash  parse error at line 10, column 28
%   p06_modulepath_x_latest     crash  parse error at line 8, column 24
%   p07_modulepath_x_coalesce   crash  parse error at line 10, column 25
%   n2_modulepath_x_next        crash  parse error at line 5, column 34
%   n3_modulepath_x_combine     crash  parse error at line 6, column 44
%   n4_modulepath_x_finalize    crash  parse error at line 5, column 46
%
% The term door compiled all six: SWI reads `a.b(X)` as '.'(a, b(X)) and
% rel_path_parts/3 catches that shape, so the gap was the TEXT spelling alone
% and every fixture here is also a text-door round-trip receipt.
%
% One fixture per parse_surface_wrapper/4 shape, which is the whole grammar
% surface the gap covered:
%
%   rel_atom          latest/1, finalize/1, next/1, pre/1
%   rel_atom_default  coalesce/2, pre/2
%   atom_list         combine/variadic

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ rel_atom_default : coalesce/2 over a dotted source ═════════════════════
% `alpha` has a commit at the path, `beta` does not. coalesce desugars to a
% read plus a negated arm, so this also crosses path resolution with the
% expander's own not(...) rewrite.
% rx: source$.pipe(defaultIfEmpty('absent')) per repo key, the path being a
% compile-time name over the same stream.

fixture(module_path_in_coalesce_wrapper,
  prog([ col_type(repo/1, name, text),
         col_type(git__latest_commit/2, name, text),
         col_type(git__latest_commit/2, commit, text),
         rel_path_decl(git__latest_commit/2, [git, latest_commit]),
         col_type(repo_latest/2, name, text),
         col_type(repo_latest/2, commit, text) ],
       [ (repo_latest(Name, Commit) <-
              repo(Name),
              coalesce(rel_path([git, latest_commit], [Name, Commit]),
                       'absent')) ]),
  [ repo('alpha'),
    repo('beta'),
    git__latest_commit('alpha', 'sha-1') ],
  [],
  [ final(repo_latest/2, [ repo_latest('alpha', 'sha-1'),
                           repo_latest('beta',  'absent') ]) ]).

% ═══ rel_atom : latest/1 over a dotted source in an EDGE body ═══════════════
% A bare atom in an edge body is a TRIGGER, so the dotted roster is sampled
% rather than fired on: only `bell` occurrences produce a row.
% rx: bell$.pipe(withLatestFrom(roster$)), the path naming the same roster$.

fixture(module_path_in_latest_wrapper,
  prog([ col_type(orchard__roster/2, tree_id, int),
         col_type(orchard__roster/2, owner, text),
         rel_path_decl(orchard__roster/2, [orchard, roster]),
         col_type(bell/1, tree_id, int),
         kind(bell/1, log), keep(bell/1, all),
         col_type(called/2, tree_id, int),
         col_type(called/2, owner, text),
         kind(called/2, log), keep(called/2, all) ],
       [ (called(TreeId, Owner) <+
              bell(TreeId),
              latest(rel_path([orchard, roster], [TreeId, Owner]))) ]),
  [ orchard__roster(1, 'ada'),
    orchard__roster(2, 'bo') ],
  [ [ +bell(1) ] ],
  [ final(called/2, [ called(1, 'ada') ]) ]).

% ═══ atom_list : combine/variadic over two dotted sources ═══════════════════
% `combine(A, B)` is the conjunction spelling, so a dotted atom in either slot
% has to resolve exactly as it does in a bare conjunction.
% rx: combineLatest([tree$, plot$]) joined on the tree key.

fixture(module_path_in_combine_wrapper,
  prog([ col_type(orchard__tree/1, tree_id, int),
         rel_path_decl(orchard__tree/1, [orchard, tree]),
         col_type(orchard__plot/2, tree_id, int),
         col_type(orchard__plot/2, plot, text),
         rel_path_decl(orchard__plot/2, [orchard, plot]),
         col_type(sited/2, tree_id, int),
         col_type(sited/2, plot, text) ],
       [ (sited(TreeId, Plot) <-
              combine(rel_path([orchard, tree], [TreeId]),
                      rel_path([orchard, plot], [TreeId, Plot]))) ]),
  [ orchard__tree(1),
    orchard__plot(1, 'north'),
    orchard__plot(2, 'south') ],
  [],
  [ final(sited/2, [ sited(1, 'north') ]) ]).
