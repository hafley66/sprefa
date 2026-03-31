# Plan: .sprf Parser

```
.sprf text ──parse──▶ SelectorChain ──lower──▶ Rule { select: Vec<SelectStep> }
```

## .sprf Syntax

```sprf
# tagged slots: function notation
fs(**/Cargo.toml) > json({ package: { name: $NAME } });

# bare context: all 3 positional (repo > branch > fs)
my-org/* > main|release/* > **/Cargo.toml > json({ deps: { $K: $_ } });

# ast-grep, language inferred from fs extension
**/*.ts > ast(import { $$$NAMES } from '$SOURCE');

# ast-grep, explicit language
**/*.config > ast[typescript](import $NAME from '$PATH');

# regex content match
helm/**/*.yaml > re(image:\s+(?P<REPO>[^:]+):(?P<TAG>.+));

# recursive descent inside destructuring
**/values.yaml > json({ **: { image: { repository: $REPO, tag: $TAG } } });

# array iteration
**/Cargo.toml > json({ workspace: { members: [...$MEMBER] } });

# regex on keys
**/Cargo.toml > json({ re:^(dev-)?dependencies: { $NAME: $_ } });
```

## Grammar

```
program    = (rule ";")*
rule       = slot (">" slot)*
slot       = tag "[" arg "]" "(" body ")"
           | tag "(" body ")"
           | bare_glob
tag        = json | re | ast | repo | branch | fs
bare_glob  = (not > ; ( )+
body       = paren-counted (open increments, close decrements, done at 0)
```

### Bare inference

N consecutive bare globs before first tagged slot:
- N=3: repo > branch > fs
- N<3: error ("bare context requires all three, or use tags")

### Destructuring (inside `json(...)`)

```
pattern    = object | array | capture | wildcard | value_glob
object     = "{" (entry ("," entry)*)? "}"
entry      = key ":" pattern
key        = ** | $CAP | $_ | re:REGEX | glob_str
array      = "[" "..." pattern "]"
capture    = "$" SCREAMING
wildcard   = "$_"
value_glob = (not , } ] )+
```

### Lowering

| .sprf | SelectStep |
|---|---|
| bare glob / `fs(...)` | `File { pattern }` |
| `repo(...)` | `Repo { pattern }` |
| `branch(...)` | `Branch { pattern }` |
| `json({ key: pat })` | `Object { entries }` |
| `json({ **: pat })` | `[Any, ...lowered(pat)]` |
| `json({ $K: $V })` | `ObjectEntry { key: Capture, value }` |
| `json([...$X])` | `Array { item: [Leaf { capture }] }` |
| `ast(pat)` / `ast[lang](pat)` | `AstSelector { pattern, language }` |
| `re(pat)` | `ValuePattern { pattern }` |

### Edge cases

- `)` in body: paren counting. Regex `(?P<...>)` is balanced. Literal unbalanced `)` needs `\)`
- `>` in body: fine, only `>` outside parens is a separator
- `;` in body: fine, only `;` outside parens terminates
- `**` as destructuring key: special-cased as recursive descent, not glob
- `re:` prefix in key position: regex key matching via existing `pipe_glob_matches`
- Empty `{ }`: matches any object (vacuous truth)
- `ast` as bare glob: fine, only triggers as tag when followed by `(` or `[`
- `#` comments: to end of line

## Crate structure

```
crates/sprf/
  Cargo.toml          (winnow, anyhow)
  src/
    lib.rs
    _0_ast.rs           (SelectorChain, Slot, Tag)
    _1_parse.rs         (winnow: .sprf text -> Vec<SelectorChain>)
    _2_pattern.rs       (winnow: json body -> Vec<ObjectEntry> / SelectStep tree)
    _3_lower.rs         (SelectorChain -> Rule)
```

## Future

- Output slot syntax for capture-to-match mapping
- Link rules
- Tree-sitter grammar for .sprf highlighting
- `:has()`, `:not()` pseudo-selectors
