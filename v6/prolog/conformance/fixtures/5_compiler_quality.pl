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

% ITEM 1b FAIL-FIRST RECEIPT: aggregate GROUP BY with bare integer literals.
% The item-1 fix wrapped literals in support_group_exprs only;
% aggregate_group_exprs (scoped-delta insert + recompute) still emitted them
% verbatim (altitude review finding 1, 2026-07-29).
%
% RED, before aggregate_group_exprs shared the literal wrap:
%   emitted recompute arm:
%     GROUP BY 0, 0
%   sqlite reads a bare integer there as a SELECT-list position.
%
% GREEN, after both group-expr sites share group_expr/3:
%   sweep: groupby_aggregate_two_bare_integer_literals oracle-identical
fixture(groupby_aggregate_two_bare_integer_literals,
  prog([ col_type(source/1, name, text),
         col_type(tallied/3, line, int),
         col_type(tallied/3, column, int),
         col_type(tallied/3, total, int) ],
       [ (tallied(0, 0, count(Name)) <- source(Name)) ]),
  [],
  [ [ +source(alpha) ],
    [ +source(beta) ] ],
  [ deltas(tallied/3,
      [ [ +tallied(0, 0, 1) ],
        [ -tallied(0, 0, 1), +tallied(0, 0, 2) ] ]),
    final(tallied/3,
      [ tallied(0, 0, 2) ]) ]).

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

% ═══ D4: a higher-order goal is a named refusal, not a phantom input rel ════
%
% `call/N` has no registry row, so nothing recognized it as a construct, and
% the edb_definition ruling (a rel no rule heads is pure input) claimed it
% first: `call/3` became a real EDB relation with synthesized columns, a real
% table, and no rows the world ever pushes. Every higher-order spelling
% therefore answered ZERO ROWS with no refusal on either door, which is the
% one shape this repo treats as worse than an error.
%
% RED RECEIPTS, taken at a4629623.
%
% COMPILER (swipl -q -l compile/compile.pl -g compile_dl6(...)):
%
%   COMPILED CLEAN
%   CREATE TABLE "call" ("src" TEXT NOT NULL, "name" TEXT NOT NULL,
%                        "value" TEXT NOT NULL,
%                        PRIMARY KEY ("src", "name", "value")) WITHOUT ROWID
%   INSERT OR IGNORE INTO "out" ("name", "value")
%   SELECT DISTINCT d0."name", d0."value" FROM "__frontier_call" d0 ...
%
% ORACLE (compile/scripts/dl6_oracle.pl over one +src arrival), for both the
% call/3 and the call/1 spellings: the tick log carries the src arrival and
% nothing else, `out` never derives, nothing is thrown.
%
%   {"tick":1,"deltas":{"src":{"add":[["a",1]],"del":[]}}}
%
% The refusal name is the one labs/generic_scan_instantiation already chose
% for this exact question: a relation name is a ground functor the program
% writes down, never an argument (dynamic_relation_name).
%
% NARROW BY CONSTRUCTION: `call` is a legal relation name here and the alpha's
% own flagship has one -- callgraph_derivation_over_extraction in
% fixtures/3_flagship_callgraph.pl declares `call/2`, copied from v5's
% examples/callgraph-ast.dl -- so the trigger is an UNDECLARED, UNHEADED
% `call/N` goal, and that flagship fixture is this refusal's negative leg. The
% first cut of the check refused the name outright and turned both flagship
% fixtures red, which is how the narrowing came to be written down here.

% The apply spelling. Identical in shape whether the first argument is a rel
% NAME (`src`) or a variable bound elsewhere: same functor, same arity, same
% refusal, so one fixture covers both.
fixture(higher_order_call_goal_rejected,
  prog([ col_type(src/2, name, text), col_type(src/2, value, int),
         col_type(out/2, name, text), col_type(out/2, value, int) ],
       [ (out(Name, Value) <- call(src, Name, Value)) ]),
  [],
  [],
  [ throws(dynamic_relation_name(call/3)) ]).

% The wrapped-atom spelling. A different arity, and the same answer: the goal
% inside is not reached by anything that resolves relation names.
fixture(higher_order_call_over_atom_rejected,
  prog([ col_type(src/2, name, text), col_type(src/2, value, int),
         col_type(out/2, name, text), col_type(out/2, value, int) ],
       [ (out(Name, Value) <- call(src(Name, Value))) ]),
  [],
  [],
  [ throws(dynamic_relation_name(call/1)) ]).

% ═══ D2: a backslash in a string constant survives both doors ═══════════════
%
% A backslash was deleted TWICE on the way from .dl6 text to a running
% program, and neither deletion said anything:
%
%   parse_dl.pl quoted_chars/4 -> escape_code/2's catch-all `escape_code(C, C)`
%   turns every unrecognized `\X` into plain `X`, so `\d` parsed as `d`.
%
%   emit_ts.pl js_template/2 wrote SQL text into a JS TEMPLATE LITERAL while
%   escaping only the backtick and `${`, so a backslash that did survive the
%   parser was then eaten by JavaScript's own template-literal escaping.
%
% `${` being escaped and `\` not is what made this a PARTIAL escape rather
% than a decision. The user-visible cost was that any regex in a .dl6 program
% had to be backslash-free.
%
% THE RULE, now stated in both places: `\n`, `\t` and `\r` are real escapes,
% `\\` is one backslash, `\'` and `\"` are the quote character, and EVERY
% OTHER `\X` is two characters, the backslash and X, unchanged. The strings
% this language carries are regexes and shell fragments, where `\d` and `\.`
% are the common case and a silently deleted backslash is a wrong pattern
% with no error.
%
% This fixture is the EMITTER half (the parser half is
% plunit_tests.pl:backslash_escapes_follow_the_stated_rule, which is where a
% hand-written `\d` can be put through parse_dl; a printed .dl6 view always
% doubles its backslashes, so round-trip alone cannot see that clause).
%
% RED RECEIPT, taken at a4629623 -- the emitted comparison SQL, in which the
% backslash is gone before sqlite ever sees it:
%
%   SELECT b0."text_value" FROM "raw" b0 WHERE b0."text_value" = 'digit d here'
%
% so the emitter matched the row WITHOUT the backslash and the oracle matched
% the row WITH it: two engines, two different rows, no error on either.
fixture(backslash_in_string_literal_survives_both_doors,
  prog([ col_type(raw/1, text_value, text),
         col_type(hit/1, text_value, text) ],
       [ (hit(Value) <- raw(Value), Value == 'digit \\d here') ]),
  [],
  [ [ +raw('digit \\d here'), +raw('digit d here') ] ],
  [ final(hit/1, [ hit('digit \\d here') ]),
    ticks(1) ]).
