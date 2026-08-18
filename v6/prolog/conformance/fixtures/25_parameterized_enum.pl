:- op(1150, xfx, '<-').
:- op(1150, xfx, '<+').
:- op(700,  xfx, ':=').

% A parameterized sum declaration mints two DISTINCT concrete enums when the
% template is instantiated at two different argument pairs. Each concrete enum
% keeps its own payload types: err(error: L) and ok(value: R) substitute the
% concrete L/R per instantiation.
fixture(parameterized_enum_two_instantiations,
    prog(
        [ rel_template_enum([result],
                            [type_parameter('L', []), type_parameter('R', [])],
                            (err(error:'L') ; ok(value:'R'))),
          col_type(host_error/1, code, int),
          col_type(boop_response/1, body, text),
          col_type(parse_error/1, message, text),
          col_type(syntax_tree/1, root, text),
          col_type(fetch/2, id, int),
          col_type(fetch/2, outcome, result(host_error, boop_response)),
          keyed(fetch/2, [1]),
          col_type(compile/2, id, int),
          col_type(compile/2, outcome, result(parse_error, syntax_tree)),
          keyed(compile/2, [1]) ],
        []),
    [],
    [],
    []).
