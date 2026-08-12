// grammar.js -- tree-sitter grammar for .dl6
//
// Hand-written overlay over the emitter's skeleton. Tokens and the value
// expression spine follow the DCG's recognizer; see emit.pl + REPORT.md for
// which rules the emitter produced versus which this file adds.

module.exports = grammar({
  name: 'dl6',

  extras: ($) => [/\s/],

  word: ($) => $.identifier,

  rules: {
    program: ($) => repeat($._statement),

    _statement: ($) =>
      choice(
        $.rel_declaration,
        $.sh_declaration,
        $.bind_declaration,
        $.query,
        $.level_rule,
        $.edge_rule,
        $.fact,
        $.match_statement
      ),

    // ----------------------------------------------------------------- declarations
    rel_declaration: ($) =>
      seq(
        'rel',
        $.name,
        '(',
        optional($._decl_arg_list),
        ')',
        repeat($._decl_mod),
        '.'
      ),

    sh_declaration: ($) =>
      seq(
        'sh',
        $.name,
        '(',
        optional($._decl_arg_list),
        ')',
        '->',
        '(',
        optional($._decl_arg_list),
        ')',
        '=',
        '`',
        /([^`])*/,
        '`',
        '.'
      ),

    bind_declaration: ($) =>
      seq('bind', $.name, '(', optional($._decl_arg_list), ')', '.'),

    _decl_mod: ($) =>
      choice(
        $._decl_mod_log,
        $._decl_mod_keep,
        $._decl_mod_key
      ),
    _decl_mod_log: ($) => 'log',
    _decl_mod_keep: ($) =>
      seq(
        'keep',
        '(',
        choice('all', seq($.identifier, '(', $.number, ')')),
        ')'
      ),
    _decl_mod_key: ($) => seq('key', '(', repeat(seq($.number, ',')), $.number, ')'),

    _decl_arg_list: ($) =>
      seq(
        $.decl_arg,
        repeat(seq(choice(',', ';'), $.decl_arg))
      ),

    decl_arg: ($) =>
      choice(
        seq($.identifier, ':', $.decl_arg),
        seq($.name, '(', optional($._decl_arg_list), ')'),
        $.name
      ),

    // ----------------------------------------------------------------- rules
    level_rule: ($) =>
      seq(field('head', $.call), '<-', field('body', $._goal_list), '.'),
    edge_rule: ($) =>
      seq(field('head', $.call), '<+', field('body', $._goal_list), '.'),
    fact: ($) => seq(field('head', $.call), '.'),
    query: ($) => seq('?', field('head', $.call), '.'),

    match_statement: ($) =>
      seq(
        'match',
        field('source', $.call),
        '(',
        optional($._match_arms),
        ')',
        '.'
      ),
    _match_arms: ($) =>
      seq(optional(';'), $.match_arm, repeat(seq(';', $.match_arm))),
    match_arm: ($) =>
      seq(
        field('guards', $._goal_list),
        choice('|->', '|+>'),
        field('head', $.call)
      ),

    _goal_list: ($) => seq($.expr, repeat(seq(',', $.expr))),

    // ----------------------------------------------------------------- expressions
    expr: ($) =>
      choice(
        $.call,
        $.primary,
        $.unary_expr,
        prec.left(0, seq(field('left', $.expr), $._bind_op, field('right', $.expr))),
        prec.left(1, seq(field('left', $.expr), $._cmp_op, field('right', $.expr))),
        prec.left(2, seq(field('left', $.expr), choice('+', '-'), field('right', $.expr))),
        prec.left(3, seq(field('left', $.expr), choice('*', '/', 'mod'), field('right', $.expr)))
      ),

    _bind_op: ($) => choice(':=', 'is'),
    _cmp_op: ($) =>
      choice('==', '\\==', '=:=', '=\\=', '=<', '<=', '>=', '>', '<', '!=', '='),

    call: ($) =>
      seq(field('name', $.name), '(', optional($._arg_list), ')'),

    _arg_list: ($) => seq($.expr, repeat(seq(',', $.expr))),

    primary: ($) =>
      choice(
        $.identifier,
        $.number,
        $.atom_literal,
        $.string_literal,
        $.list_literal,
        $.brace_literal
      ),

    unary_expr: ($) => prec(4, seq('-', $.expr)),

    list_literal: ($) =>
      seq(
        '[',
        optional($._list_items),
        ']'
      ),
    _list_items: ($) =>
      seq($._list_item, repeat(seq(',', $._list_item))),
    _list_item: ($) => choice(seq('...', $.expr), $.expr),

    brace_literal: ($) =>
      seq('{', optional($._brace_entries), '}'),
    _brace_entries: ($) =>
      seq($.brace_entry, repeat(seq(',', $.brace_entry))),
    brace_entry: ($) =>
      choice(
        seq($.brace_key, ':', $.expr, ':', $.decl_arg),
        seq($.brace_key, ':', $.expr)
      ),
    brace_key: ($) =>
      choice(
        seq('$', $.identifier),
        '**',
        $.atom_literal,
        $.string_literal,
        $.identifier,
        '_'
      ),

    // ----------------------------------------------------------------- tokens
    name: ($) => $.identifier,

    identifier: ($) => /[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*/,

    number: ($) => /-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?/,

    atom_literal: ($) => /'([^'\\]|\\.)*'/,

    string_literal: ($) => /"([^"\\]|\\.)*"/,
  },
})
