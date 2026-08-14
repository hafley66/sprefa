% 16_string_affix_tests.pl : contains / starts_with / ends_with are SPELLABLE
% today, so the registry rows the string plan forked for them would buy a
% name and not a capability. Each fixture writes the predicate out of landed
% primitives, which is the receipt that keeps the fork row accurate.
%
% The one place a real row would earn its keep is an EMPTY affix: substr's
% position 0 sits before the first character, so `substr(Text, 0 - 0)` reads
% the whole text where an ends_with('') must answer true. Every affix below
% is non-empty for that reason.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(contains_is_instr_greater_than_zero,
  prog([col_type(sym/1, name, text),
        col_type(has_sep/1, name, text)],
       [ (has_sep(Name) <- sym(Name), Position := instr(Name, '::'), Position > 0) ]),
  [ sym('std::io'), sym(plain), sym('a::b::c') ],
  [],
  [ final(has_sep/1,
          [ has_sep('a::b::c'),
            has_sep('std::io') ]) ]).

fixture(starts_with_is_a_leading_substr_compare,
  prog([col_type(sym/1, name, text),
        col_type(is_emit/1, name, text)],
       [ (is_emit(Name) <-
             sym(Name), Head := substr(Name, 1, length('emit_')),
             Head == 'emit_') ]),
  [ sym(emit_types), sym(emit_), sym(lower_types), sym(emit) ],
  [],
  [ final(is_emit/1,
          [ is_emit(emit_),
            is_emit(emit_types) ]) ]).

% A negative substr start counts from the end, so the trailing |Suffix|
% characters are one substr with an arithmetic operand.
fixture(ends_with_is_a_negative_substr_compare,
  prog([col_type(path/1, name, text),
        col_type(is_prolog/1, name, text)],
       [ (is_prolog(Name) <-
             path(Name), Tail := substr(Name, 0 - length('.pl')),
             Tail == '.pl') ]),
  [ path('lower.pl'), path('lower.ts'), path('.pl'), path(pl) ],
  [],
  [ final(is_prolog/1,
          [ is_prolog('.pl'),
            is_prolog('lower.pl') ]) ]).

% The affix arrives from a COLUMN rather than a literal, which is the shape a
% renderer rule needs and the one a fixed-arity registry row would also serve.
fixture(starts_with_takes_the_prefix_from_a_column,
  prog([col_type(sym/1, name, text),
        col_type(prefix/1, text_value, text),
        col_type(prefixed/2, name, text),
        col_type(prefixed/2, prefix, text)],
       [ (prefixed(Name, Prefix) <-
             sym(Name), prefix(Prefix),
             Head := substr(Name, 1, length(Prefix)),
             Head == Prefix) ]),
  [ sym(emit_types), sym(lower_types), prefix(emit), prefix(lower) ],
  [],
  [ final(prefixed/2,
          [ prefixed(emit_types, emit),
            prefixed(lower_types, lower) ]) ]).
