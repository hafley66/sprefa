module.exports = grammar({
  name: "dl6",

  extras: $ => [/\s/, $.comment],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($.statement),

    statement: $ => choice($.relation_declaration, $.rule, $.fact),

    relation_declaration: $ => seq(
      "rel",
      field("name", $.path),
      "(",
      optional(choice($.enum_variants, commaSep1($.column))),
      ")",
      repeat($.relation_modifier),
      ".",
    ),

    column: $ => seq(field("name", $.identifier), ":", field("type", $.type)),

    type: $ => seq(
      field("name", $.identifier),
      optional(seq("(", field("element", $.type), ")")),
      optional("?"),
    ),

    enum_variants: $ => seq($.enum_variant, repeat1(seq(";", $.enum_variant))),
    enum_variant: $ => seq($.identifier, "(", optional(commaSep1($.column)), ")"),

    relation_modifier: $ => choice(
      "log",
      seq("keep", "(", choice("all", seq("count", "(", $.integer, ")")), ")"),
      seq("key", "(", commaSep1($.integer), ")"),
    ),

    rule: $ => seq(
      field("head", $.atom),
      field("arrow", choice("<-", "<+")),
      field("body", $.goal_list),
      ".",
    ),

    fact: $ => seq($.atom, "."),
    goal_list: $ => commaSep1($.expression),

    expression: $ => choice($.atom, $.object_pattern, $.literal, $.variable),

    atom: $ => seq(field("name", $.path), "(", optional(commaSep1($.argument)), ")"),
    argument: $ => choice(
      seq(field("name", $.identifier), ":", field("value", $.expression)),
      $.expression,
    ),

    object_pattern: $ => seq("{", optional(commaSep1($.object_pair)), "}"),
    object_pair: $ => seq(
      field("key", choice($.identifier, $.quoted_atom, $.capture_key)),
      ":",
      field("value", $.expression),
      optional(seq(":", field("type", $.identifier))),
    ),
    capture_key: $ => /\$[A-Z_][A-Za-z0-9_]*/,

    path: $ => sep1($.identifier, "."),
    literal: $ => choice($.integer, $.float, $.string, $.quoted_atom, $.boolean),
    integer: $ => /-?[0-9]+/,
    float: $ => /-?[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?/,
    string: $ => /"([^"\\]|\\.)*"/,
    quoted_atom: $ => /'([^'\\]|\\.)*'/,
    boolean: $ => choice("true", "false"),
    variable: $ => /[A-Z_][A-Za-z0-9_]*/,
    identifier: $ => /[a-z_][A-Za-z0-9_]*/,
    comment: $ => token(seq("#", /.*/)),
  },
});

function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}

function sep1(rule, separator) {
  return seq(rule, repeat(seq(separator, rule)));
}
