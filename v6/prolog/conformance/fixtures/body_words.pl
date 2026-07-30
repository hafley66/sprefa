% fixtures/body_words.pl : the registry's BODY-WORD surface, graded.
%
% Every fixture here was written against a proven silence. The defect class is
% one shape: a word registry.pl carries a row for reaches the reference
% engine's solve/2, which has no clause for it, so solve/2's LAST clause reads
% it as an ordinary relation atom. Nothing ever pushes a row named `combine/2`
% or `zip/2`, so the rule derives nothing, reports nothing, and looks like a
% program whose inputs simply have not arrived yet.
%
% FAIL-FIRST RECEIPT, oracle door, before this file existed (scratch harness
% over engine:run_program/5, same programs as the fixtures below):
%
%   F1 combine:     rows=[]          F1 next: rows=[]
%   control conj:   rows=[out(1,2)]
%   F2 zip:         rows=[]          F2 subscribe:   rows=[]
%   F2 complete:    rows=[]          F2 unsubscribe: rows=[]
%   F2 error:       rows=[]
%
% and the COMPILER on the identical programs:
%
%   F1 combine:   COMPILED CLEAN     F1 next: COMPILED CLEAN
%   F2 zip:       THREW unsupported_construct(zip)
%   F2 subscribe: THREW unsupported_construct(lifecycle_arm(subscribe))
%
% So the two families were silent for two different reasons and needed two
% different answers:
%
%   LIVE rows (combine/variadic, next/1) are a capability the compiler already
%   shipped. Its lowering for `combine(a(X), b(Y))` is BYTE-IDENTICAL to the
%   lowering of `a(X), b(Y)`, and for `next(a(X))` byte-identical to the bare
%   atom, on the level plane and the edge plane both. The oracle was the only
%   reader that could not execute them, so it gained the clause rather than a
%   refusal, and these fixtures are the byte-diffed proof the two doors now
%   answer the same thing.
%
%   RESERVED rows (zip/2 and the four lifecycle wrappers) are words the
%   language has claimed and given no meaning. The compiler refused them by
%   name already; the oracle now refuses them too, so the answer is the same
%   at both doors instead of "error" at one and "no rows" at the other.
%
% Owner: coordinator.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ live splice rows: combine/variadic and next/1 ══════════════════════════

% combine on the LEVEL plane. The pair of fixtures below is the whole point:
% the expectation lists are character for character identical, because
% `combine(A, B)` and `A, B` are the same program. The oracle derived nothing
% for the first and pair(1, 2) for the second until body.pl:solve/2 learned the
% splice role.
fixture(combine_level_is_the_conjunction_spelling,
  prog([],
       [ (pair(Left, Right) <- combine(source_a(Left), source_b(Right))) ]),
  [],
  [ [ +source_a(1), +source_b(2) ] ],
  [ deltas(pair/2, [ [ +pair(1, 2) ] ]),
    final(pair/2, [ pair(1, 2) ]) ]).

fixture(conjunction_level_control_for_combine,
  prog([],
       [ (pair(Left, Right) <- (source_a(Left), source_b(Right))) ]),
  [],
  [ [ +source_a(1), +source_b(2) ] ],
  [ deltas(pair/2, [ [ +pair(1, 2) ] ]),
    final(pair/2, [ pair(1, 2) ]) ]).

% combine on the EDGE plane, which was a SECOND silence with its own cause:
% even with a solving clause, engine.pl:trigger_items/2 walked with
% splice_bare(false), so the spliced atoms never became trigger occurrences
% and the rule was statically dead. The compiler classified the same body as
% `unmarked_conjunction([source_a(_), source_b(_)])`, exactly as it classifies
% the conjunction control below.
%
% Both arrivals land before any occurrence fires, so each of the two
% occurrences derives pair(1, 2); the keyed head makes the second an
% equal-row no-op, which is why one delta is reported and not two. The empty
% second tick is the engine-owned drain the edge write's carry-out mints (q5),
% and the conjunction control mints exactly the same one.
fixture(combine_edge_is_the_conjunction_spelling,
  prog([ keyed(pair/2, [1]) ],
       [ (pair(Left, Right) <+ combine(source_a(Left), source_b(Right))) ]),
  [],
  [ [ +source_a(1), +source_b(2) ] ],
  [ deltas(pair/2, [ [ +pair(1, 2) ], [] ]),
    final(pair/2, [ pair(1, 2) ]) ]).

fixture(conjunction_edge_control_for_combine,
  prog([ keyed(pair/2, [1]) ],
       [ (pair(Left, Right) <+ (source_a(Left), source_b(Right))) ]),
  [],
  [ [ +source_a(1), +source_b(2) ] ],
  [ deltas(pair/2, [ [ +pair(1, 2) ], [] ]),
    final(pair/2, [ pair(1, 2) ]) ]).

% next/1 wraps ONE atom, so its splice is the identity and the fixture pins
% exactly that: the wrapper reads the same as the bare atom on both planes.
fixture(next_level_is_the_bare_atom_spelling,
  prog([], [ (seen(Value) <- next(source(Value))) ]),
  [],
  [ [ +source(1) ] ],
  [ deltas(seen/1, [ [ +seen(1) ] ]),
    final(seen/1, [ seen(1) ]) ]).

fixture(next_edge_is_the_bare_atom_spelling,
  prog([ keyed(seen/1, [1]) ],
       [ (seen(Value) <+ next(source(Value))) ]),
  [],
  [ [ +source(1) ] ],
  [ deltas(seen/1, [ [ +seen(1) ], [] ]),
    final(seen/1, [ seen(1) ]) ]).

% ═══ reserved rows: claimed words with no meaning ═══════════════════════════
%
% One fixture per reserved body word, because each was independently silent
% and a single representative would let the other four regress unnoticed. The
% refusal names the word and its arity, which is the only thing the author
% needs to know: the language has taken this spelling and has not defined it.

fixture(zip_is_a_named_refusal,
  prog([], [ (pair(Left, Right) <- zip(source_a(Left), source_b(Right))) ]),
  [],
  [ [ +source_a(1), +source_b(2) ] ],
  [ throws(reserved_body_word(zip/2)) ]).

fixture(subscribe_is_a_named_refusal,
  prog([], [ (seen(Value) <- subscribe(source(Value))) ]),
  [],
  [ [ +source(1) ] ],
  [ throws(reserved_body_word(subscribe/1)) ]).

fixture(unsubscribe_is_a_named_refusal,
  prog([], [ (seen(Value) <- unsubscribe(source(Value))) ]),
  [],
  [ [ +source(1) ] ],
  [ throws(reserved_body_word(unsubscribe/1)) ]).

fixture(complete_is_a_named_refusal,
  prog([], [ (seen(Value) <- complete(source(Value))) ]),
  [],
  [ [ +source(1) ] ],
  [ throws(reserved_body_word(complete/1)) ]).

fixture(error_is_a_named_refusal,
  prog([], [ (seen(Value) <- error(source(Value))) ]),
  [],
  [ [ +source(1) ] ],
  [ throws(reserved_body_word(error/1)) ]).
