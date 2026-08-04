:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(regexp_positive_match,
  prog([ col_type(source/1, text, text) ],
       [ (matched(Text) <- source(Text), regexp(Text, "^a.c$")) ]),
  [ source('abc'), source('axc'), source('zzz') ],
  [],
  [ final(matched/1, [ matched(abc), matched(axc) ]) ]).

fixture(regexp_non_match,
  prog([ col_type(source/1, text, text) ],
       [ (matched(Text) <- source(Text), regexp(Text, "^z+$")) ]),
  [ source('abc'), source('axc') ],
  [],
  [ final(matched/1, []) ]).

fixture(regexp_retraction_flip,
  prog([ col_type(source/1, text, text) ],
       [ (matched(Text) <- source(Text), regexp(Text, "^a.c$")) ]),
  [ source('abc') ],
  [ [ -source('abc') ], [ +source('axc') ] ],
  [ deltas(matched/1, [ [ -matched(abc) ], [ +matched(axc) ] ]),
    final(matched/1, [ matched(axc) ]) ]).

fixture(regexp_pattern_not_literal,
  prog([ col_type(source/1, text, text),
         col_type(pattern/1, pattern, text) ],
       [ (matched(Text) <- source(Text), pattern(Pattern),
                         regexp(Text, Pattern)) ]),
  [ source('abc'), pattern("^a") ],
  [],
  [ throws(regexp_pattern_not_literal) ]).

fixture(regexp_operand_not_text,
  prog([ col_type(source/1, value, int) ],
       [ (matched(Value) <- source(Value), regexp(Value, "^1$")) ]),
  [ source(1) ],
  [],
  [ throws(regexp_operand_not_text(source/1, value, int)) ]).

fixture(regexp_pattern_outside_subset,
  prog([ col_type(source/1, text, text) ],
       [ (matched(Text) <- source(Text), regexp(Text, "a(?=b)")) ]),
  [ source('ab') ],
  [],
  [ throws(regexp_pattern_outside_subset("a(?=b)")) ]).

fixture(regexp_pattern_invalid,
  prog([ col_type(source/1, text, text) ],
       [ (matched(Text) <- source(Text), regexp(Text, "[")) ]),
  [ source('abc') ],
  [],
  [ throws(regexp_pattern_invalid("[", "Syntax error: missing terminating ] for character class")) ]).
