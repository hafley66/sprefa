// grammar.js -- tree-sitter grammar for the .dl6 language.
// Emitted skeleton from emit.pl merged with the hand overlay.
module.exports = grammar({
  name: 'dl6',
  extras: $ => [/\s+/],

  rules: {
    program: $ => repeat($._statement),

    _statement: $ => choice(
      $._decl,
      $._rule,
      $._match
    ),

    // ---- declarations ----
    _decl: $ => choice(
      $._rel_decl,
      $._sh_decl,
      $._bind_decl,
      $._query_decl
    ),

    _rel_decl: $ => seq(
      'rel', $._name, '(',
        choice($._enum_cols, seq(optional($._cols))),
      ')',
      repeat($._modifier),
      '.'
    ),

    _enum_cols: $ => seq(
      $._variant,
      repeat(seq(';', $._variant))
    ),
    _variant: $ => seq($._name, '(', optional($._cols), ')'),

    _cols: $ => seq(
      $._col,
      repeat(seq(',', $._col))
    ),
    _col: $ => seq($._name, optional(seq(':', $._type))),

    _type: $ => seq(
      $._name,
      optional(seq('(', seq($._type, repeat(seq(',', $._type))), ')'))
    ),

    _modifier: $ => choice(
      'log',
      seq('keep', '(', choice('all', seq('count', '(', $._int, ')')), ')'),
      seq('key', '(', seq($._int, repeat(seq(',', $._int))), ')')
    ),

    _sh_decl: $ => seq(
      'sh', $._name, '(', optional($._cols), ')', '->', '(', optional($._cols), ')',
      '=', seq('`', /[^`]*/, '`'), '.'
    ),

    _bind_decl: $ => seq('bind', $._name, '(', optional($._cols), ')', '.'),

    _query_decl: $ => seq('?', $._name, '(', optional($._args), ')', '.'),

    // ---- rules ----
    _match: $ => seq(
      'match', $._atom, '(',
      optional(';'),
      seq($._arm, repeat(seq(';', $._arm))),
      ')', '.'
    ),
    _arm: $ => seq($._body, choice('|->', '|+>'), $._head),

    _rule: $ => choice(
      seq($._head, '.'),
      seq($._head, '<', choice('-', '+'), $._body, '.')
    ),

    _head: $ => $._atom,

    _body: $ => seq(
      $._item,
      repeat(seq(',', $._item))
    ),

    _item: $ => choice(
      $._atom,
      $._cmp,
      $._bind,
      $._path
    ),

    _cmp: $ => seq($._expr, $._cmpop, $._expr),
    _cmpop: $ => choice('>=', '=<', '==', '\\==', '=\\=', '>', '<', '=:='),

    _bind: $ => seq($._expr, $._bindop, $._expr),
    _bindop: $ => choice(':=', 'is'),

    // ---- atoms / calls ----
    _atom: $ => seq(
      $._name,
      '(', optional($._args), ')'
    ),

    _args: $ => seq(
      choice($._expr, $._cmp),
      repeat(seq(',', choice($._expr, $._cmp)))
    ),

    // ---- expressions ----
    _expr: $ => choice(
      $._arith,
      $._atom,
      $._bracket,
      $._braces,
      $._int,
      $._float,
      $._string,
      $._atomlit,
      $._path
    ),

    _arith: $ => prec.left(seq(
      $._expr, choice('+', '-', '*', '/', 'mod'), $._expr
    )),

    _path: $ => $._name,

    _bracket: $ => seq(
      '[',
      optional(seq(
        $._listitem,
        repeat(seq(',', $._listitem))
      )),
      ']'
    ),
    _listitem: $ => choice(
      seq('...', $._expr),
      $._expr
    ),

    _braces: $ => choice(
      seq('{', '}'),
      seq('{', seq($._pair, repeat(seq(',', $._pair))), '}')
    ),
    _pair: $ => seq($._key, ':', $._value),
    _key: $ => choice(
      '**',
      seq('$', $._name),
      $._atom,
      $._string,
      $._name
    ),
    _value: $ => choice($._expr, seq($._name, ':', $._type)),

    // ---- tokens ----
    _name: $ => /[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*/,
    _int: $ => /-?[0-9]+/,
    _float: $ => /-?[0-9]+\.[0-9]+/,
    _string: $ => /"(\\.|[^"\\\n])*"/,
    _atomlit: $ => /'(\\.|[^'\\\n])*'/
  }
});
