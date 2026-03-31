# Plan: .sprf Parser

```
.sprf text ──parse──▶ SelectorChain ──lower──▶ Rule { select, create_matches }
                                                       ▲
                                               LinkRule { kind, predicate }
```

## .sprf Syntax

```sprf
# extraction: select > content > match slots
fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > match($NAME, package_name);

fs(**/package.json) > json({ dependencies: { $NAME: $VERSION } })
  > match($NAME, dep_name)
  > match($VERSION, dep_version);

# bare context: all 3 positional (repo > branch > fs)
my-org/* > main|release/* > **/Cargo.toml > json({ deps: { $K: $_ } })
  > match($K, dep_name);

# ast-grep, language inferred from fs extension
**/*.ts > ast(import { $$$NAMES } from '$SOURCE')
  > match($NAMES, import_name);

# ast-grep, explicit language
**/*.config > ast[typescript](import $NAME from '$PATH')
  > match($NAME, import_name);

# regex content match
helm/**/*.yaml > re(image:\s+(?P<REPO>[^:]+):(?P<TAG>.+))
  > match($REPO, image_repo)
  > match($TAG, image_tag);

# recursive descent inside destructuring
**/values.yaml > json({ **: { image: { repository: $REPO, tag: $TAG } } })
  > match($REPO, image_repo)
  > match($TAG, image_tag);

# array iteration
fs(**/Cargo.toml) > json({ workspace: { members: [...$MEMBER] } })
  > match($MEMBER, workspace_member);

# regex on keys
fs(**/Cargo.toml) > json({ re:^(dev-)?dependencies: { $NAME: $_ } })
  > match($NAME, dep_name);

# link rules
link(dep_name > package_name, norm_eq) > $dep_to_package;
link(import_name > export_name, target_file_eq, string_eq) > $import_binding;
link(env_var_ref > env_var_name, norm_eq) > $env_var_binding;
link(image_repo > package_name, norm_eq) > $image_source;
```

## Grammar

```
program    = (statement ";")*
statement  = rule | link_decl

rule       = slot (">" slot)*
slot       = tagged_slot | bare_glob | match_slot
tagged_slot = tag "[" arg "]" "(" body ")"
            | tag "(" body ")"
tag        = json | re | ast | repo | branch | fs
bare_glob  = (not > ; ( )+
match_slot = "match" "(" "$" SCREAMING "," IDENT ")"
body       = paren-counted (open increments, close decrements, done at 0)

link_decl  = "link" "(" src_kind ">" tgt_kind ("," predicate)* ")" (">" "$" IDENT)?
predicate  = norm_eq | string_eq | target_file_eq | same_repo
           | stem_eq | ext_eq | dir_eq
```

### Bare inference

N consecutive bare globs before first tagged slot:
- N=3: repo > branch > fs
- N<3 or N>3: error ("bare context requires all three, or use tags")

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

| .sprf | Target type |
|---|---|
| bare glob / `fs(...)` | `SelectStep::File { pattern }` |
| `repo(...)` | `SelectStep::Repo { pattern }` |
| `branch(...)` | `SelectStep::Branch { pattern }` |
| `json({ key: pat })` | `SelectStep::Object { entries }` |
| `json({ **: pat })` | `[SelectStep::Any, ...lowered(pat)]` |
| `json({ $K: $V })` | `ObjectEntry { key: Capture, value }` |
| `json([...$X])` | `SelectStep::Array { item: [Leaf { capture }] }` |
| `ast(pat)` / `ast[lang](pat)` | `AstSelector { pattern, language }` |
| `re(pat)` | `ValuePattern { pattern }` |
| `match($CAP, kind)` | `MatchDef { capture, kind }` |
| `link(A > B, preds) > $K` | `LinkRule { kind, predicate }` |

### Match slot partitioning

Match slots can appear anywhere in the chain. During lowering they are
partitioned out regardless of position and become `create_matches` entries
on the Rule. Same pattern as context step partitioning.

### Edge cases

- `)` in body: paren counting. Regex `(?P<...>)` is balanced. Literal unbalanced `)` needs `\)`
- `>` in body: fine, only `>` outside parens is a separator
- `>` in link body: fine, inside `link(...)` parens
- `;` in body: fine, only `;` outside parens terminates
- `**` as destructuring key: special-cased as recursive descent, not glob
- `re:` prefix in key position: regex key matching via existing `pipe_glob_matches`
- Empty `{ }`: matches any object (vacuous truth)
- `ast` as bare glob: fine, only triggers as tag when followed by `(` or `[`
- `match` as bare glob: fine, only triggers as tag when followed by `(`
- `link` as bare glob: fine, only triggers as keyword at statement start
- `#` comments: to end of line

## Crate structure

```
crates/sprf/
  Cargo.toml          (winnow, anyhow)
  src/
    lib.rs
    _0_ast.rs           (SelectorChain, Slot, Tag, LinkDecl)
    _1_parse.rs         (text -> Vec<Statement>)
    _2_pattern.rs       (json body -> Vec<SelectStep>)
    _3_lower.rs         (Statement -> Rule | LinkRule)
```

## Implementation status

- [x] _0_ast.rs: parse tree types
- [x] _1_parse.rs: selector chain parser
- [x] _2_pattern.rs: json body parser
- [x] _3_lower.rs: lower to Rule
- [x] CLI wiring (.sprf extension dispatch)
- [x] Integration tests (real repo Cargo.toml)
- [ ] match() slot: parse + lower to MatchDef
- [ ] link() declaration: parse + lower to LinkRule
- [ ] Wire link rules into RuleSet from .sprf

## Future

- Tree-sitter grammar for .sprf highlighting
- `:has()`, `:not()` pseudo-selectors
- Migrate sprefa-rules.json to .sprf as default
