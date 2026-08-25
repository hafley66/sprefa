const PREC = {
  bind: 1,
  compare: 2,
  add: 3,
  multiply: 4,
  unary: 5,
  call: 6,
};

module.exports = grammar({
  name: "dl6",

  extras: $ => [/\s/, $.comment],
  word: $ => $.identifier,

  conflicts: $ => [
    [$.object_pattern, $.json_object],
    [$.list, $.json_array],
    [$.json_value, $.literal],
    [$.json_literal, $.json_value],
  ],

  rules: {
    source_file: $ => repeat($.statement),

    statement: $ => choice($.use_declaration, $.bind_declaration, $.relation_declaration, $.shell_declaration, seq("import", $.string, "."), $.query, $.match_statement, $.rule, $.fact),

    use_declaration: $ => seq(field("visibility", optional("pub")), "use", field("path", $.string), optional(seq("as", field("alias", $.identifier))), "."),
    relation_declaration: $ => seq("rel", field("name", $.path), "(", field("columns", optional(choice($.enum_variants, seq($.declaration_parameter, repeat(seq(",", $.declaration_parameter)))))), ")", field("modifiers", repeat($.relation_modifier)), "."),

    shell_declaration: $ => seq("sh", field("name", $.path), "(", field("inputs", optional(seq($.column, repeat(seq(",", $.column))))), ")", "->", "(", field("outputs", optional(seq($.column, repeat(seq(",", $.column))))), ")", "=", field("template", $.template), "."),

    bind_declaration: $ => seq("bind", field("name", $.identifier), "(", optional(seq($.column, repeat(seq(",", $.column)))), ")", "."),
    declaration_parameter: $ => seq(field("name", $.identifier), optional(seq(":", field("type", $.type)))),
    column: $ => seq(field("name", $.identifier), ":", field("type", $.type)),

    type: $ => choice(
      $.arrow_type,
      $.product_type,
      $.sum_type,
      seq(field("name", $.identifier), optional(seq("(", field("arguments", optional(seq($.type_argument, repeat(seq(",", $.type_argument))))), ")")), field("optional", optional("?"))),
    ),

    arrow_type: $ => seq("(", "(", field("inputs", optional(commaSep1($.field))), ")", "->", field("output", $.type), ")"),

    type_argument: $ => choice($.type, $.type_named_argument),
    type_named_argument: $ => seq(field("name", choice($.identifier, $.variable)), ":", field("value", $.expression)),

    product_type: $ => seq("(", field("fields", commaSep1($.field)), ")"),
    sum_type: $ => seq("(", $.sum_variant, repeat(seq(";", $.sum_variant)), ")"),
    field: $ => seq(field("name", $.identifier), ":", field("type", $.type)),
    sum_variant: $ => seq(field("name", $.identifier), "(", optional(commaSep1($.field)), ")"),

    enum_variants: $ => seq($.enum_variant, repeat(seq(";", $.enum_variant))),
    enum_variant: $ => seq($.identifier, "(", optional(commaSep1($.column)), ")"),

    relation_modifier: $ => choice("log", seq("keep", "(", choice("all", seq("count", "(", $.integer, ")")), ")"), seq("key", "(", $.integer, repeat(seq(",", $.integer)), ")"), "set"),
    rule: $ => seq(field("head", $.atom), field("arrow", choice("<-", "<+")), field("body", $.goal_list), "."),
    fact: $ => seq($.atom, "."),
    query: $ => seq("?", $.atom, "."),

    match_statement: $ => seq("match", field("scrutinee", $.atom), "(", optional(";"), $.match_arm, repeat(seq(";", $.match_arm)), ")", "."),
    match_arm: $ => seq(field("guard", $.goal_list), field("arrow", choice("|->", "|+>")), field("head", $.atom)),
    goal_list: $ => commaSep1($.expression),

    expression: $ => choice(
      $.binding_expression,
      $.comparison_expression,
      $.binary_expression,
      $.unary_expression,
      $.member_expression,
      $.atom,
      $.identifier,
      $.json_literal,
      $.object_pattern,
      $.list,
      $.literal,
      $.variable,
      $.parenthesized_expression,
    ),

    binding_expression: $ => prec.right(PREC.bind, seq(
      field("left", choice($.variable, $.atom)),
      field("operator", choice(":=", "is")),
      field("right", $.expression),
    )),

    comparison_expression: $ => prec.left(PREC.compare, seq(
      field("left", $.expression),
      field("operator", choice("==", "\\==", ">", "<", ">=", "=<", "=:=", "=\\=")),
      field("right", $.expression),
    )),

    binary_expression: $ => choice(
      prec.left(PREC.add, seq($.expression, choice("+", "-"), $.expression)),
      prec.left(PREC.multiply, seq($.expression, choice("*", "/", "mod"), $.expression)),
    ),

    unary_expression: $ => prec(PREC.unary, seq(choice("-", "+"), $.expression)),
    member_expression: $ => prec(PREC.call, seq(choice($.identifier, $.variable), repeat1($.member_access))),
    member_access: $ => token.immediate(/\.[A-Za-z_][A-Za-z0-9_]*/),
    parenthesized_expression: $ => seq("(", $.expression, ")"),

    atom: $ => seq(field("name", $.path), "(", optional(seq(choice($.named_argument, $.expression), repeat(seq(",", choice($.named_argument, $.expression))))), ")"),
    named_argument: $ => seq(field("name", $.identifier), ":", field("value", $.expression)),
    object_pattern: $ => seq("{", optional(seq($.object_pair, repeat(seq(",", $.object_pair)))), "}"),
    object_pair: $ => seq(field("key", choice("**", $.capture_key, $.quoted_atom, $.string, $.identifier)), ":", field("value", $.expression), optional(seq(":", field("type", $.identifier)))),
    json_literal: $ => choice($.json_object, $.json_array),
    json_object: $ => seq("{", optional(seq($.json_pair, repeat(seq(",", $.json_pair)))), "}"),
    json_pair: $ => seq(field("key", $.string), ":", field("value", $.json_value)),
    json_array: $ => seq("[", optional(seq($.json_value, repeat(seq(",", $.json_value)))), "]"),
    json_value: $ => choice($.json_object, $.json_array, $.float, $.integer, $.string, $.boolean, "null"),
    capture_key: $ => token(/\$[A-Za-z_][A-Za-z0-9_]*/),
    list: $ => seq("[", optional(choice($.spread_element, seq($.expression, repeat(seq(",", $.expression))))), "]"),
    spread_element: $ => seq("...", $.expression),

    // The DCG accepts a path dot only when an identifier follows it with no
    // gap (parse_dl_dcg.pl dot_then_ident//0), which is what keeps a
    // clause-terminating "." out of the path.
    path: $ => seq($.identifier, repeat($.member_access)),
    literal: $ => choice($.float, $.integer, $.string, $.quoted_atom, $.boolean),
    integer: $ => /[0-9]+/,
    float: $ => /[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?/,
    string: $ => /"([^"\\]|\\.)*"/,
    quoted_atom: $ => /'([^'\\]|\\.)*'/,
    template: $ => /`([^`\\]|\\.)*`/,
    boolean: $ => choice("true", "false"),
    variable: $ => choice(/[A-Z][A-Za-z0-9_]*/, /_+[A-Z][A-Za-z0-9_]*/, "_"),
    identifier: $ => /_*[a-z][A-Za-z0-9_]*/,
    comment: $ => token(/#.*/),
  },
});

function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}
