# JS/TS Analysis with oxc: Algorithms and Test Structure

## What oxc provides

oxc is a Rust-native JS/TS parser and toolchain. Three components used here:

- `oxc_parser`   -- parses source to AST + ModuleRecord in one pass
- `oxc_resolver` -- node/TS module resolution (tsconfig paths, extensions, conditions)
- `oxc_ast`      -- AST node types for JSX traversal

All three share a single arena allocator (`Allocator`). Parse once, walk multiple times.


## Core algorithms

### 1. Import extraction

`oxc_parser` builds a `ModuleRecord` during parsing. No second pass needed.
`ModuleRecord.requested_modules` maps specifier strings to statement spans.

Algorithm:
```
parse(source) -> ModuleRecord
for each (specifier, spans) in requested_modules:
  for each span (covers full import statement):
    rfind(specifier) within span source range  -- specifier always trails
    emit ImportInfo { specifier, span_start, span_end, is_type }
```

The rfind trick: the specifier string always appears as the trailing string
literal in an import/export statement. Searching backwards within the statement
span gives the exact byte range of the specifier without quotes.

Output: `Vec<ImportInfo>`, sorted ascending by `span_start` (for safe bottom-to-top rewriting).


### 2. Export extraction

`ModuleRecord.local_export_entries` contains all named exports.
Each entry has: `local_name`, `export_name`, `span`.

Algorithm:
```
for each entry in local_export_entries:
  match entry.local_name:
    Name(s)  -> emit ExportInfo { local_name: s, span, is_default: false }
    Default  -> emit ExportInfo { local_name: "default", is_default: true }
```

Output: `Vec<ExportInfo>`


### 3. Import binding extraction

Named imports: `import { Foo, Bar as B } from './mod'`
The `import_entries` in `ModuleRecord` give both the imported name and the local binding.

Algorithm:
```
for each entry in import_entries where specifier matches target:
  match entry.import_name:
    Name(n)          -> local = entry.local_name, import = n
    Default          -> local = entry.local_name, import = "default"
    NamespaceObject  -> local = entry.local_name, is_namespace = true
  emit ImportBinding { import_name, local_name, spans, has_alias, module_specifier }
```

Output: `Vec<ImportBinding>`


### 4. Module resolution (three tiers)

Given a specifier and importing file, resolve to an absolute path:

**Tier 1: oxc_resolver**
- Handles: relative paths, bare specifiers, tsconfig paths/baseUrl,
  extension probing (`.ts` `.tsx` `.js` `.jsx` `.mjs` `.cjs` `.json`),
  condition names (import/require/node/default), directory index files.
- Fallback needed: tsconfig `paths` without `baseUrl` (common with
  `moduleResolution: "Bundler"`). Extract paths manually, inject as aliases.

**Tier 2: bare specifier via package registry**
- When oxc fails on `@scope/pkg/sub`: extract package name from specifier,
  look up in `repo_packages` table, resolve subpath against target repo's file index.
- Cross-repo capable.

**Tier 3: suffix matching**
- Fallback for non-JS files and fully unresolved specifiers.
- Build suffix map from all indexed file paths. Match specifier as path suffix.

Resolver construction:
```
read tsconfig.json (strip JS comments first -- tsconfig allows them)
extract compilerOptions.paths -> convert wildcard patterns to alias entries
build Resolver with extensions + tsconfig + aliases + condition_names
```


### 5. Speculative resolution (no filesystem access)

For cases where the resolver can't run (no tsconfig, cross-repo):
```
strip known extensions from specifier
check if target path (without extension) ends with the stripped specifier
handle index files: specifier "foo" matches "foo/index.ts"
```

Output: `SpecMatch` enum `{ None | Direct | Barrel }`


### 6. Source rewriting (bottom-to-top)

Given a source string and a list of `(span_start, span_end, new_text)` rewrites:
```
sort rewrites descending by span_start
apply each rewrite as a byte-level splice
later rewrites don't shift earlier offsets because we work from the end
```

Output: `Vec<u8>` (new source bytes)


### 7. JSX component tree extraction

Walk the oxc AST to find all JSX element instantiations within a file.
Goal: extract which components render which other components, with nesting depth.

Key insight: `oxc_ast` uses `inherit_variants!` macros on several enums.
`JSXExpression`, `Argument`, `ExportDefaultDeclarationKind` do NOT have an
`Expression` variant -- they inherit `Expression`'s variants directly.
Match the specific variants (`LogicalExpression`, `ConditionalExpression`, etc.)
rather than a wrapped `Expression` variant.

Algorithm:
```
walk_stmt(stmt, depth, parent_span, results):
  match stmt:
    FunctionDeclaration -> walk_function_body
    ExportNamedDeclaration -> walk Declaration inner (FunctionDeclaration | VariableDeclaration)
    ExportDefaultDeclaration -> match kind:
      FunctionDeclaration | ArrowFunctionExpression | JSXElement | JSXFragment

walk_element(element, depth, parent_span, results):
  name = element_name(element.opening_element.name)
  if name starts with uppercase:  // PascalCase = component, skip intrinsics
    emit JsxElementInfo { name, span, depth, parent_span }
    walk children with depth+1, parent_span=this span
  walk attribute values -- catches <Layout header={<Header />} />

walk_child(child, depth, parent_span, results):
  match child:
    Element(e)             -> walk_element
    Fragment(f)            -> walk_fragment
    ExpressionContainer(e) -> walk_jsx_expr(e.expression)

walk_jsx_expr(expr):  -- JSXExpression uses inherit_variants!
  match:
    JSXElement              -> walk_element
    JSXFragment             -> walk_fragment
    LogicalExpression       -> walk both sides        -- {a && <B />}
    ConditionalExpression   -> walk both branches     -- {c ? <A /> : <B />}
    ParenthesizedExpression -> recurse
    CallExpression          -> walk arguments         -- {items.map(() => <Item />)}
    ArrowFunctionExpression -> walk body
    _                       -> skip
```

element_name resolution:
```
Identifier(name)        -> lowercase, skip  (HTML intrinsic like "div")
IdentifierReference(name) -> keep if PascalCase  (component reference)
MemberExpression        -> flatten to "Namespace.Member"  (e.g. Icons.Check)
ThisExpression | NamespacedName -> skip
```

Output: `Vec<JsxElementInfo>`
```rust
struct JsxElementInfo {
    name: String,                    // component name
    span_start: u32,                 // opening tag start byte
    span_end: u32,                   // opening tag end byte
    depth: u32,                      // 0 = top-level render
    parent_span_start: Option<u32>,  // nearest ancestor component's span
}
```


## Test fixture structure

Fixtures are `.tsx` files with structured comments that serve as the test spec.
Tests are data-driven: parse the comments, run extraction, assert against them.

Comment format:
```
// EXPECTED_EDGES: ComponentA -> DepX, ComponentA -> DepY
// TIER: N
// SCOPE: (optional: description of the pattern being tested)
// EXPECTED_NO_EDGES: ComponentA -> DepZ  (optional: must NOT appear)
// EXPECTED_DEPTH: ComponentA -> DepX: 0, ComponentA -> DepY: 1  (optional)
```

TIER scale (complexity of the JSX pattern):
```
1 -- direct instantiation in return JSX
2 -- variable alias, destructured import alias
3 -- conditional (ternary, logical &&, switch)
4 -- function returning JSX, arrow function returning JSX
5 -- JSX in prop values, render props, children-as-function
```

Test helper pattern:
```rust
fn run_fixture(path: &str) {
    let source = read_fixture(path);
    let expected_edges = parse_expected_edges(&source);
    let expected_no_edges = parse_expected_no_edges(&source);
    let expected_depths = parse_expected_depths(&source);
    let elements = extract_jsx_elements(Path::new(path), &source);
    let edges = build_edge_set(&elements);
    for (parent, child) in &expected_edges {
        assert!(edges.contains(&(parent, child)), "missing edge {parent} -> {child}");
    }
    for (parent, child) in &expected_no_edges {
        assert!(!edges.contains(&(parent, child)), "false edge {parent} -> {child}");
    }
}
```

`parse_expected_edges`: find `// EXPECTED_EDGES:` line, split on `,`, parse `A -> B` pairs.
`parse_expected_no_edges`: same for `// EXPECTED_NO_EDGES:`.
`parse_expected_depths`: parse `A -> B: N` triples for depth assertions.

`build_edge_set`: from `Vec<JsxElementInfo>`, for each element with a `parent_span_start`,
find the element whose span contains that `parent_span_start`, emit `(parent.name, child.name)`.


## File classification for source type

```
.tsx             -> SourceType::tsx()
.ts              -> SourceType::ts()
.jsx             -> SourceType::jsx()
.js .mjs .cjs   -> SourceType::mjs()
anything else   -> SourceType::tsx()   (superset, safe fallback)
```


## Known oxc_ast sharp edges

**`inherit_variants!` macro**: several enums inherit `Expression` variants directly.
Do NOT match `Expression(e)` and then match `e`. Match the concrete variants:
- `JSXExpression`: `EmptyExpression | JSXElement | JSXFragment | LogicalExpression | ...`
- `Argument`: `JSXElement | JSXFragment | SpreadElement | ...`
- `ExportDefaultDeclarationKind`: `FunctionDeclaration | ArrowFunctionExpression | JSXElement | ...`

**`IdentifierReference` vs `Identifier` in `JSXElementName`:**
- `Identifier` -> lowercase HTML tag (div, span, button) == skip
- `IdentifierReference` -> PascalCase component reference == keep

**`MemberExpression` in `JSXElementName`:**
- flatten recursively: `object.property` -> `"Namespace.Member"`
- handles `Icons.Check`, `UI.Button`, etc.
