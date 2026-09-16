/**
 * tree-sitter-dl — grammar for sprefa v5 `.dl` source.
 *
 * The language is a datalog over files in repo/rev/time space. Top-level
 * items (each terminated by `.`):
 *   - rel declarations:      `rel NAME(COL: TYPE, ...).`
 *   - brand declarations:    `type NAME <: PARENT.`
 *   - anchor declarations:   `anchor NAME = fs:BODY.`
 *   - rules:                 `NAME(TERMS) <- BODY.`
 *   - facts (body-less rules): `NAME(TERMS).`
 *   - queries:               `? ATOM.`
 *   - codegen rules:         `gen PATH <- BODY.`
 *   - module imports:        `use "PATH".`
 *   - rule templates:        `def NAME(PARAMS) <- BODY.`
 *
 * Body items (comma-separated):
 *   - relation atoms:        `rel(TERMS)`
 *   - negated atoms:         `!rel(TERMS)`
 *   - builtins:              `scan(...)`, `sg(...)`, `match(...)`, `ast(...)`,
 *                            `json(...)`, `cmd(...)`
 *   - comparisons:           `TERM =~ "re"`, `TERM == TERM`, `TERM != TERM`,
 *                            `TERM < TERM`, ...
 *
 * Terms:
 *   - identifiers (vars or rel refs), integers, strings, interpolated strings,
 *     regexes `/.../`, typed path literals `scheme:BODY`, `:TYPE` annotations
 *     (e.g. `:rust`), arithmetic expressions.
 *
 * Regen: `cd tree-sitter-dl && tree-sitter generate`.
 */

const BSPAREN = token.immediate(seq('\\', /./));

const PREC = {
  expr: 1,
  unary: 2,
};

module.exports = grammar({
  name: 'dl',

  extras: $ => [/\s/, $.comment],


  conflicts: $ => [
    [$.atom, $.scan_call],
    [$.atom, $.sg_call],
    [$.atom, $.match_call],
    [$.atom, $.ast_call],
    [$.atom, $.json_call],
    [$.atom, $.cmd_call],
    [$.term, $.type_tag],
    [$.expr, $.comparison],
    [$.def_template, $.rule],
    [$.use_decl, $.rule],
  ],

  rules: {
    program: $ => repeat($._item),

    _item: $ => choice(
      $.rel_decl,
      $.brand_decl,
      $.anchor_decl,
      $.gen_rule,
      $.query,
      $.rule,
    ),

    // ---------- top-level items ----------

    rel_decl: $ => seq(
      'rel',
      field('name', $.identifier),
      $.param_list,
      '.',
    ),

    brand_decl: $ => seq(
      'type',
      field('name', $.identifier),
      '<:',
      field('parent', $.identifier),
      '.',
    ),

    anchor_decl: $ => seq(
      'anchor',
      field('name', $.identifier),
      '=',
      field('body', $.scheme_literal),
      '.',
    ),

    use_decl: $ => prec(-1, seq(
      'use',
      field('path', $.string),
      '.',
    )),

    // A rule head may use a name that is also a keyword (`def(...)` as a rel
    // named "def"). def_template needs a separate name identifier after the
    // `def` keyword; if `(` follows `def` directly, this is a rule, not a
    // template. We express that by requiring the name to be a full identifier
    // token followed by `(`, not allowing tree-sitter's missing-token recovery
    // to insert a phantom name. The `prec(1, ...)` shifts resolution in favor
    // of `rule` when the input is `def(` — the lower-precedence def_template
    // loses to a clean rule match.
    def_template: $ => prec(-1, seq(
      'def',
      field('name', $.identifier),
      $.def_param_list,
      '<-',
      field('body', $.body),
      '.',
    )),

    // `def` template params are bare identifiers (no `:type`), unlike rel
    // decls. The engine binds them positionally at each call site.
    def_param_list: $ => seq(
      '(',
      optional(seq(commaSep($.identifier), optional(','))),
      ')',
    ),

    query: $ => seq('?', $.atom, '.'),

    // A rule is `head <- body.` or `head.` (a fact). The head is an atom that
    // may carry aggregate calls in term positions.
    rule: $ => seq(
      field('head', $.head_atom),
      field('body', optional(seq('<-', $.body))),
      '.',
    ),

    // `gen (head_args) <- body.` — same head shape as a rule, with a
    // `gen` keyword prefix. Codegen rules' head has no separate rel
    // identifier; the keyword `gen` itself is the operator and the args
    // are the path + template.
    gen_rule: $ => prec(-1, seq(
      'gen',
      $.arg_list,
      '<-',
      field('body', $.body),
      '.',
    )),

    // ---------- atoms and heads ----------

    // A head atom allows aggregate calls (`count(T)`, `sum(X)`, ...) in term
    // positions; regular relation names everywhere else.
    head_atom: $ => seq(
      field('rel', $.identifier),
      $.head_arg_list,
    ),

    head_arg_list: $ => seq(
      '(',
      optional(seq(commaSep($.head_arg), optional(','))),
      ')',
    ),

    head_arg: $ => choice($.agg_call, $.expr),

    agg_call: $ => seq(
      field('fn', $.agg_fn),
      '(',
      field('arg', $.expr),
      ')',
    ),

    agg_fn: $ => choice('count', 'sum', 'min', 'max', 'mean'),

    // A plain relation atom (body and query positions). Same shape as a
    // function call: `rel(term, term, ...)`.
    atom: $ => seq(
      field('rel', $.identifier),
      $.arg_list,
    ),

    arg_list: $ => seq(
      '(',
      optional(seq(commaSep(choice($.term, $.named_arg)), optional(','))),
      ')',
    ),

    param_list: $ => seq(
      '(',
      optional(seq(commaSep($.param), optional(','))),
      ')',
    ),

    param: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $.type_ref),
    ),

    type_ref: $ => $.identifier,

    // ---------- body ----------

    body: $ => commaSep1($.body_item),

    body_item: $ => choice(
      $.neg_atom,
      $.scan_call,
      $.sg_call,
      $.match_call,
      $.ast_call,
      $.json_call,
      $.cmd_call,
      $.comparison,
      $.scheme_atom,
      $.atom,
    ),

    neg_atom: $ => seq('!', $.atom),

    // Builtins. Each has its own node so semantic highlighting / queries can
    // pick them out by name. Argument structure is a flat term list — the
    // engine's lower-time matches positional args to the builtin's signature.
    scan_call: $ => seq('scan', $.arg_list),
    sg_call:   $ => seq(choice('sg', 'ast-grep'), $.arg_list),
    match_call: $ => seq('match', $.arg_list),
    ast_call:  $ => seq('ast', $.arg_list),
    json_call: $ => seq('json', $.arg_list),
    cmd_call:  $ => seq('cmd', $.arg_list),

    // A bare scheme literal in body position (`fs:src/x` as a body atom).
    scheme_atom: $ => $.scheme_literal,

    comparison: $ => seq(
      field('left', $.term),
      field('op', $.cmp_op),
      field('right', $.term),
    ),

    cmp_op: $ => choice(
      '~~',
      '=~', '!~',
      '==', '!=', '<=', '>=', '<', '>',
      '=~*', '!~*',
      '=',
    ),

    // ---------- terms ----------

    // Body terms include `:type` tags (e.g. `:rust` in `sg(p, v, :rust, ...)`).
    // Head terms are arithmetic exprs (no type tags).
    term: $ => choice(
      $.type_tag,
      $.expr,
    ),

    type_tag: $ => seq(':', $.identifier),

    named_arg: $ => seq(
      field('label', $.identifier),
      token.immediate(':'),
      field('value', $.expr),
    ),

    expr: $ => choice(
      $.unary_expr,
      $.binary_expr,
      $.call_expr,
      $.primary,
    ),

    // Unary minus: `-1` for split's negative index. Lowered to `0 - x` by
    // the engine.
    unary_expr: $ => prec(PREC.unary, seq('-', field('arg', $.expr))),

    // Function call in term position: `split(file, "/", -1)`. Same shape as
    // a relation atom; context (head arg vs body) decides which it is at
    // lower-time. The grammar treats both as the same surface syntax.
    call_expr: $ => seq(
      field('fn', $.identifier),
      $.arg_list,
    ),

    binary_expr: $ => prec.left(PREC.expr, seq(
      field('left', $.primary),
      field('op', $.arith_op),
      field('right', $.expr),
    )),

    arith_op: $ => choice('+', '-', '*', '/', '%'),

    primary: $ => choice(
      $.identifier,
      $.integer,
      $.string,
      $.interp_string,
      $.regex,
      $.scheme_literal,
      $.paren_expr,
    ),

    paren_expr: $ => seq('(', $.expr, ')'),

    // ---------- literals ----------

    // Typed path literal: `scheme:body`. The body has two forms (fenced and
    // bare) — see `scheme_body`. The scheme is an identifier; the colon and
    // body are immediate (no whitespace allowed between scheme and `:`).
    scheme_literal: $ => seq(
      field('scheme', $.identifier),
      token.immediate(':'),
      field('body', $.scheme_body),
    ),

    // Fenced: `` `...` `` — only backtick terminates. Bare: runs to whitespace
    // / `,` / `)` at depth 0, allowing `()[]{}` nesting.
    scheme_body: $ => choice(
      token.immediate(seq('`', /[^`]*/, '`')),
      token.immediate(/[^ \t\r\n,)`]+/),
    ),

    string: $ => token(seq(
      '"',
      repeat(choice(/[^"\\\n]/, /\\./)),
      '"',
    )),

    interp_string: $ => seq(
      '"',
      repeat(choice(
        /[^"\\\n$]+/,
        /\\./,
        seq('$', /[A-Za-z_][A-Za-z0-9_]*/),
        seq('${', /[A-Za-z_][A-Za-z0-9_]*/, '}'),
      )),
      '"',
    ),

    regex: $ => token(seq(
      '/',
      repeat(choice(/[^\/\\\n]/, /\\./)),
      '/',
    )),

    integer: $ => token(/-?\d+/),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    comment: $ => token(seq('#', /.*/)),
  },
});

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

function commaSep(rule) {
  return optional(commaSep1(rule));
}

function noneOf(chars) {
  const escaped = chars.split('').map(c => {
    if (/[.*+?^${}()|[\]\\]/.test(c)) return '\\' + c;
    return c;
  }).join('');
  return new RegExp(`[^${escaped}]`);
}
