:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% An enum variant field declared float stays float. The removed catch-all
% used to flatten every non-int non-text field to int; a 2.5 arrival at an
% int column is the field_not_int unsupported construct, so this fixture would refuse under
% the old ruleset and passes only when float survives into the variant.
fixture(variant_field_float_stays_float,
    prog(
        [enum_decl(reading, (peak(value: float) ; trough(value: float)))],
        []),
    [],
    [
        [+reading_peak(1, 2.5)]
    ],
    [
        final(reading_peak/2, [reading_peak(1, 2.5)]),
        deltas(reading_peak/2, [
            [+reading_peak(1, 2.5)]
        ]),
        ticks(1)
    ]).

% An enum variant field declared bool keeps its CHECK: the storage kind bool
% exists and is not collapsed to int, so a bool literal survives.
fixture(variant_field_bool_stays_bool,
    prog(
        [enum_decl(switch, (on(is_on: bool) ; off(is_on: bool)))],
        []),
    [],
    [
        [+switch_on(1, bool_lit(true))]
    ],
    [
        final(switch_on/2, [switch_on(1, bool_lit(true))]),
        deltas(switch_on/2, [
            [+switch_on(1, bool_lit(true))]
        ]),
        ticks(1)
    ]).

% A variant field typed as a declared struct is a ref: the type name passes
% through verbatim and the type plane resolves it to ref(span), the pointer
% survival the removed coercion used to erase.
fixture(variant_field_typed_as_struct_is_a_ref,
    prog(
        [type_decl(span, [col(lo, int), col(hi, int)]),
         col_type(span/2, lo, int),
         col_type(span/2, hi, int),
         col_type(holder/1, item, span),
         enum_decl(loc, (here(at: span) ; elsewhere(note: text)))],
        []),
    [],
    [
        [+loc_here(2, obj([lo-40, hi-42]))]
    ],
    [
        final(loc_here/2, [loc_here(2, obj([hi-42, lo-40]))]),
        deltas(loc_here/2, [
            [+loc_here(2, obj([hi-42, lo-40]))]
        ]),
        ticks(1)
    ]).

% A variant field typed json stays json: the document is not flattened into
% an int and round-trips as a canonical object.
fixture(variant_field_typed_as_json_stays_json,
    prog(
        [enum_decl(payload, (blob(data: json) ; none))],
        []),
    [],
    [
        [+payload_blob(1, {k: 'v'})]
    ],
    [
        final(payload_blob/2, [payload_blob(1, {k:'v'})]),
        deltas(payload_blob/2, [
            [+payload_blob(1, {k:'v'})]
        ]),
        ticks(1)
    ]).

% The two storage clauses that were already correct (int, text) do not
% regress: an int field and a text field both survive verbatim.
fixture(variant_field_int_and_text_unchanged,
    prog(
        [enum_decl(record, (num(n: int) ; word(w: text)))],
        []),
    [],
    [
        [+record_num(1, 7)],
        [+record_word(2, "hi")]
    ],
    [
        final(record_num/2, [record_num(1, 7)]),
        final(record_word/2, [record_word(2, "hi")]),
        deltas(record_num/2, [
            [+record_num(1, 7)],
            []
        ]),
        deltas(record_word/2, [
            [],
            [+record_word(2, "hi")]
        ]),
        ticks(2)
    ]).
