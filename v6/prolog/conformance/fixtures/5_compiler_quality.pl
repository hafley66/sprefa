% fixtures/5_compiler_quality.pl: fail-first receipts for the compiler-quality
% bundle. Each fixture header records the red compiler/runtime result observed
% before its corresponding compiler fix and the green oracle/sweep result
% after it.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ITEM 1 FAIL-FIRST RECEIPT: support GROUP BY with bare integer literals.
%
% RED, before lower.pl wrapped integer literal grouping expressions:
%   sqlite3 3.45.1:
%     Error: 2nd GROUP BY term out of range - should be between 1 and 4
%   emitted arm:
%     GROUP BY b0."name", 0, 0
%
% GREEN, after only integer literal GROUP BY expressions gained `+ 0`:
%   sqlite3: alpha|0|0|1
%   sweep: groupby_two_bare_integer_literals oracle-identical
%   lsp-diags.sh: LSP DIAGS HOLDS
fixture(groupby_two_bare_integer_literals,
  prog([ col_type(source/1, name, text),
         col_type(classified/3, name, text),
         col_type(classified/3, line, int),
         col_type(classified/3, column, int) ],
       [ (classified(Name, 0, 0) <- source(Name)) ]),
  [],
  [ [ +source(alpha) ],
    [ +source(beta) ] ],
  [ deltas(classified/3,
      [ [ +classified(alpha, 0, 0) ],
        [ +classified(beta, 0, 0) ] ]),
    final(classified/3,
      [ classified(alpha, 0, 0),
        classified(beta, 0, 0) ]) ]).

% ITEM 2 FAIL-FIRST RECEIPT: comparison guard over a probe output.
%
% RED, before the response output columns joined the guard bound set:
%   sweep:
%     UNSUPPORTED probe_output_comparison_guard unbound_head_var(_)
%   expanded demand rule copied `Score > 10` without a response atom.
%
% GREEN, after probe expansion kept pre-probe goals in demand and placed the
% response atom before post-probe goals:
%   sweep: probe_output_comparison_guard oracle-identical
fixture(probe_output_comparison_guard,
  program(
    [ col_type(input/1, path, text),
      col_type(accepted/2, path, text),
      col_type(accepted/2, score, int),
      sh_decl(score,
              [col(path, text)],
              [col(score, int)],
              template("score {path}"))
    ],
    [ (accepted(Path, Score) <-
         (input(Path),
          probe(score, [Path], [Score], []),
          Score > 10))
    ],
    []),
  [ input(a) ],
  [ [ +'__host_response_score'(
          'witness|score|path:text=a', 0, a, 12) ],
    [ +'__host_response_score'(
          'witness|score|path:text=a', 1, a, 8) ] ],
  [ deltas(accepted/2,
      [ [ +accepted(a, 12) ],
        [] ]),
    final(accepted/2,
      [ accepted(a, 12) ]) ]).
