# Type Surface Gaps

## Context

Commit `8c2019615` freezes one canonical semantic type graph after generated
types are minted. `prolog/0_generic_expand.pl` now retains declarations,
members, member roles, applications, and ordered arguments without requiring
later consumers to reconstruct them from `type_decl/2` and `col_type/3`.

Three authored-language surfaces remain between that graph and the TypeSpec
modeling examples:

1. `schema_member_rows/2` is callable by compiler code, but authored DL6
   cannot query canonical declarations, members, roles, applications, and
   arguments as compiler relations.
2. `->` is accepted only by `relation_arrow_output//3` on a top-level relation
   declaration. `type_expr//1` cannot parse an arrow inside a field, wrapper,
   generic argument, anonymous product, or anonymous sum.
3. Body expressions parse brace and list terms, including nested expressions,
   but there is no contextual JSON literal that accepts copy-pasted JSON and
   lowers directly to the existing `json` value representation.

This arc contains those three gaps. HTTP relations, OpenAPI emission,
defaults, annotation inheritance, and storage projection changes remain
outside it.

## Type signatures

Authored compiler-time reflection reads storage-free views:

```text
type_decl(Type: type, Name: text, Kind: text, Phase: text)
type_member(Member: type, Owner: type, Position: int, Name: text, Value: type)
type_member_role(Member: type, Role: text)
type_application(Application: type, Constructor: type)
type_argument(Argument: type, Application: type, Position: int, Value: type)
```

Inline arrows are ordinary anonymous relation types with one member carrying
the `return` role:

```text
((id: int) -> Pet)

product_type([
  field(id, int),
  field(return, Pet)
]) + anonymous relation origin + return role
```

JSON literals use the existing expression value domain:

```text
json_object([(text, JsonValue)]) -> json
json_array([JsonValue]) -> json
JsonValue = null | bool | number | text | object | array
```

## Instance timelines

### Reflection

1. Parse and mint types.
2. Freeze canonical semantic rows.
3. Project the five reflection relations from that frozen graph.
4. Evaluate authored compiler rules.
5. Erase reflection rows and proofs before runtime planning.

### Inline arrow

1. Parse the arrow as a recursive `type_expr` node.
2. Mint an anonymous relation owned by the containing member and source path.
3. Append its `return` member and role to the canonical graph.
4. Reuse ordinary anonymous-relation lowering, matching, and target emission.

### JSON literal

1. Parse JSON tokens into a distinct JSON-literal AST.
2. Validate keys, numbers, and `null` without treating identifiers as DL6
   variables.
3. Lower nested values to the existing canonical JSON value term.
4. Reuse SQLite JSON1, Rust serde, TypeScript JSON, and boundary serialization.

## Storage

Reflection has compilation lifetime and creates no SQLite tables, arrivals,
or catalog-local identities.

Inline arrows use the same generated relation storage as an explicitly named
anonymous product. Their generated names remain physical implementation data;
canonical owner and source-path identities drive semantic equality.

JSON literals store through the existing `json` column contract. Object
member order is canonicalized at the value boundary. Arrays retain order.
SQL `NULL` is not introduced as a DL6 value.

## Read and write sequence

```text
source
  -> parse type expressions and JSON literals
  -> resolve modules
  -> mint generic and anonymous declarations
  -> freeze canonical type rows
  -> expose reflection relations
  -> evaluate compiler rules
  -> erase compiler-only rows
  -> lower ordinary relations and JSON values
  -> emit SQLite, TypeScript, Rust, and schema artifacts
```

## Decisions

1. Reflection projects existing canonical rows. It does not persist a second
   field schema.
2. Reflection relation names are ordinary compiler relations with declared
   columns and `type` values.
3. Inline arrows are recursive type expressions and mint ordinary anonymous
   relation declarations.
4. The arrow output is one ordinary member named `return`.
5. JSON literals have a distinct AST from object patterns and relation-value
   construction.
6. JSON `null` lowers to the existing DL6 JSON-null representation.
7. Tree-sitter, printer, parse-print fixpoint, Prolog compiler, oracle, and
   both executable targets receive the same accepted syntax.

Rejected alternatives:

- Persisted reflection proxy rows duplicate canonical member information.
- A service or operation special form duplicates ordinary relation types.
- Treating every brace term as JSON breaks existing object-pattern and
  relation-value semantics.

## Dependency graph

```text
canonical-type-reflection
            |
            +--------------------+
            |                    |
            v                    v
inline-arrow-type-expr     native-json-literals
```

<!-- todo(feature): Expose the frozen canonical type graph as authored compiler relations without runtime storage. -->
<!-- todo(feature): Accept inline arrow relations in every recursive type-expression position and lower them as anonymous relation types. -->
<!-- todo(feature): Accept copy-pasted JSON object and array literals and lower them to the existing canonical JSON value representation. -->

## Verification

Compiler CI additions:

- authored reflection rules enumerate exact declarations, members, roles,
  applications, and ordered arguments for imported, generic, wrapped, and
  anonymous types;
- compiler and oracle produce identical reflected rows and erase them before
  runtime planning;
- inline arrows parse and print at a field, wrapper, generic argument,
  anonymous product, and anonymous sum site;
- explicit named relation and equivalent inline arrow produce equal canonical
  member and role graphs;
- copy-pasted nested JSON covers quoted keys, strings, integers, floats,
  booleans, null, empty object, empty array, and nested arrays/objects;
- JSON values round-trip through Prolog, SQLite, TypeScript, and Rust;
- the exhaustive golden fixture uses each new surface;
- complete Prolog compiler CI passes.

## Staffing

- Worktree: `/private/tmp/sprefa-type-surface-gaps`
- Branch: `feature/type-surface-gaps`
- Base: `feature/canonical-type-row-plan` rebased onto current `origin/main`
- Sequence: reflection first; arrow and JSON may proceed independently after
  reflection CI is green.
- Suite budget: focused compiler units during implementation, then one full
  Prolog compiler CI run before integration.
