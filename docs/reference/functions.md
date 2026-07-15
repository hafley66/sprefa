# Scalar functions

Callable in a rule head or a comparison side. Generated from the engine's `fn_catalog` by examples/gen-reference.dl. Do not hand-edit.

| function | arity | group | what it does |
|---|---|---|---|
| `int` | 1 | cast | text->int coercion (leading-int prefix, else 0); fills an int column or compares numerically; SQLite CAST |
| `json` | 1 | json | validate and minify a JSON string (passthrough); SQLite-native json() |
| `json_array` | 1 | json | build a JSON array from the arg values; arity >= 1; SQLite-native json_array |
| `json_object` | 2 | json | build a JSON object from (key, value, ...) pairs; even arity >= 2; values keep their type (int -> number, text -> string); SQLite-native json_object |
| `lcfirst` | 1 | string | first char lowercased, the rest unchanged |
| `lower` | 1 | string | lowercase (Unicode-aware) |
| `norm` | 1 | string | normalize for comparison: keep ASCII alphanumerics, lowercase, drop the rest — the same fold as the `string(id,text,norm)` rel's norm column, so `norm(a) = norm(b)` is a punctuation/case-blind compare and text joins against `string.norm` |
| `replace` | 3 | string | replace ALL occurrences of `from` with `to`; SQLite-native |
| `replace_re` | 3 | string | regex replace-all with $1 group refs; the pattern shares the process-wide compile cache |
| `split` | 3 | string | split text on a separator; idx 0-based, negative counts from the end (-1 = last); out-of-range drops the row (NULL filter); the sprf_split UDF |
| `strip_prefix` | 2 | string | drop a leading affix if present, else return the input unchanged (idempotent cleanup, not a filter — pair with =~ /^p/ for drop-on-miss) |
| `strip_suffix` | 2 | string | drop a trailing affix if present, else return the input unchanged |
| `sym` | 1 | string | identity compatibility builtin; text columns are interned automatically |
| `trim` | 1 | string | strip leading and trailing whitespace |
| `ucfirst` | 1 | string | first char uppercased, the rest unchanged |
| `upper` | 1 | string | uppercase (Unicode-aware) |
