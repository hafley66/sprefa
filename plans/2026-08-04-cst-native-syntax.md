# cst native syntax — the surface the user actually asked for (2026-08-04)

User words: "i wanted cst to be native in the syntax... woulda been lispy".
The string-literal `ast()` in plans/2026-08-04-ast-op-contract.md is the WIRE
layer only. This arc puts the s-expression pattern language INSIDE the dl6
grammar. Lane C, dispatch after lanes A+B land.

## Surface

```
def(function_name, line) <-
  file(path, digest),
  cst(path, digest, rust) {
    [ (function_item name: (identifier) @function_name)
      (macro_definition name: (identifier) @function_name) ]
    (#match? @function_name "^handle_")
  }.
```

- `cst(path_var, digest_var, lang_atom) { <patterns> }` is a body item.
- The `{ ... }` block is parsed by parse_dl.pl as s-expressions: node kinds,
  field names (`name:`), `[ ]` alternation, `@capture` names, `#match?` /
  `#not-match?` / `#eq?` predicates. NOT a string; a parse error is a dl6
  parse error with line/column.
- Compile-time checks the string layer cannot do:
  - every `@capture` must name a variable used in the rule (else refusal
    `cst_capture_unused`), and every rule variable fed by the block must
    correspond to a capture (`cst_variable_uncaptured`);
  - `#match?` patterns run through the regexp/2 subset check (shipped);
  - `line` / `end_line` bind per the v5 law when named in the rule.
- Desugar: serialize the block to the canonical tree-sitter query string ->
  hand it to the ast-op expansion from lane B (minted `sh` host, wire
  contract from lane A). One new phase, zero new runtime, both doors shared.
- Later (separate, not this lane): validate node kinds against the grammar's
  node-types.json so `(function_itme)` refuses at compile time.

## Why layered, stated once
regexp/coalesce both landed as surface -> shared expansion -> shipped
machinery. Same shape here: lane A/B are that machinery; this lane is only
a parser + serializer + two refusals.
