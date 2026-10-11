# Bootstrap typegen lab compared with TypeSpec

Research date: 2026-07-20

## Targets

- Local lab: [`labs/bootstrap-typegen-lab`](../labs/bootstrap-typegen-lab/README.md)
- TypeSpec homepage: <https://typespec.io/>
- TypeSpec documentation: <https://typespec.io/docs/>
- TypeSpec repository: <https://github.com/microsoft/typespec>
- TypeSpec compiler package: <https://www.npmjs.com/package/@typespec/compiler>
- TypeSpec version observed in the current documentation: 1.14.0

## Executive index

The lab implements a small semantic compiler in one Rust binary. Its present language covers JSON-shaped data, literal string unions, aliases, typed delimiter patterns, consumers, fact projection, Rust model/server generation, and JavaScript fetch-client generation.

TypeSpec has a substantially larger declaration and extension language: packages, imports, namespaces, scalars, models, composition, operations, interfaces, templates, enums, unions, intersections, decorators, values, visibility, libraries, compiler APIs, emitters, diagnostics, formatting, and editor tooling. Its standard emitters include OpenAPI, JSON Schema, and Protobuf. See the [language overview](https://typespec.io/docs/language-basics/overview/) and [emitter overview](https://typespec.io/docs/extending-typespec/emitters-basics/).

The lab's specific experiment absent from TypeSpec's documented string-template semantics is a typed, bidirectional delimiter pattern. A pattern can be bound into a string, matched back into typed bindings, destructured, composed, and enumerated. The same pattern declaration can label HTTP, channel, queue, filesystem, object-path, or other consumers without placing those protocols in the pattern type.

## Capability matrix

| Capability | Bootstrap lab | TypeSpec |
|---|---|---|
| Implementation/runtime | Rust, one binary, no path dependencies | TypeScript/JavaScript compiler and npm libraries |
| Record models | `type User { ... }` | `model User { ... }` |
| Arrays | `Array<T>` | `T[]` and `Array<T>` |
| Key/value records | `Map<K, V>` | `Record<T>` and additional-properties composition |
| Optionality | `Optional<T>` | Optional properties with `?`, defaults, unions with `null` |
| Aliases | Yes | Yes |
| Literal types | String literals | String, numeric, boolean, null, enum and union members |
| Unions | Anonymous string-literal unions | Anonymous and named unions over general types |
| Enums | String unions lower to generated Rust enums | Named string and numeric enums, including enum spread |
| User-defined generics | Absent | Templates on aliases, models, operations, and interfaces |
| Generic constraints/defaults | Absent | `extends`, defaults, named arguments, and `valueof` parameters |
| Model composition | Nested references only | Spread, `extends`, and `is` |
| Operations | `consumer` entries with verb, pattern, and result | First-class `op`, parameters, return type, reuse, templates, and metadata references |
| Interfaces | Absent | Operation groups with extension and templates |
| Imports and namespaces | Absent | File/library imports, namespaces, and `using` |
| Extensibility | Fixed Rust compiler modules | JavaScript decorators, libraries, compiler API, semantic walker, emitter framework |
| Values | Bindings supplied to pattern evaluation | Object, array, scalar, and null values; `const`; `typeof` |
| String templates | Backtick patterns with `{name: Type}` and `:name` slots | `${expression}` interpolation in string literals |
| Runtime pattern matching | Typed bind, match, destructure, and compose | No general bidirectional matcher documented for string templates |
| Slot introspection | Ordered slot enumeration with resolved types | Compiler API exposes string-template spans |
| Structural path enumeration | Records, `[*]`, `{key}`, optionals, recursive-cycle stop | Compiler APIs and walkers expose semantic types; no equivalent path-template operation documented |
| Rule substrate | Fixed projection into five fact relations | Decorators and compiler state maps; no Datalog rule language |
| OpenAPI | Absent | OpenAPI 3.0, 3.1, and 3.2 emitter options |
| Generated server | Std-only Rust HTTP server and typed path matcher | Usually described through protocol libraries and emitted specifications or SDK emitters |
| Generated client | JavaScript fetch client | Ecosystem emitters and SDK tooling |
| Bootstrap | Semantic model Rust types generated; trusted parser/emitter boundary recorded | Compiler implemented in TypeScript/JavaScript |
| Diagnostics/tooling | Parser/check errors and tests | Diagnostic framework, formatter, language server, package/library conventions |

## Equivalent model sample

Lab source:

```text
type UserId = String
type EventKind = "created" | "deleted"

type User {
  id: UserId
  tags: Array<String>
  metadata: Map<String, String>
}

pattern UserPath = `/users/{id: UserId}`

consumer http {
  get UserPath -> User
}
```

Approximate TypeSpec model and HTTP binding:

```typespec
import "@typespec/http";

using TypeSpec.Http;

alias UserId = string;
alias EventKind = "created" | "deleted";

model User {
  id: UserId;
  tags: string[];
  metadata: Record<string>;
}

@route("/users/{id}")
@get
op getUser(@path id: UserId): User;
```

In TypeSpec, HTTP decorators classify the operation and parameter. In the lab, `UserPath` is independently addressable semantic data and `consumer http` associates it with a protocol action and result.

## String template semantics

TypeSpec supports `${}` interpolation in string literals. Any valid expression can appear, while literal-resolvable interpolation can become a `valueof string`; other cases remain semantic string-template objects for decorators or emitters to interpret. See [Type literals](https://typespec.io/docs/language-basics/type-literals/) and [decorator string-template marshalling](https://typespec.io/docs/extending-typespec/create-decorators/#string-templates-and-marshalling).

TypeSpec also separates values and types. Literals can resolve in either domain according to context, `const` declares values, and `typeof` retrieves a value's declared or inferred type. See [Values](https://typespec.io/docs/language-basics/values/).

The lab's pattern AST normalizes both slot spellings:

```text
`users/{id: UserId}/events/{kind: EventKind}`
`users/:id/events/:kind`
```

Brace syntax carries an explicit type. Colon syntax can resolve a named declaration where present and otherwise defaults to `String`. Delimiters remain literal pattern text, so `/`, `.`, `:`, and other separators have no protocol-specific meaning in the pattern evaluator.

Current evaluator operations are in [`src/9_eval.rs`](../labs/bootstrap-typegen-lab/src/9_eval.rs):

```rust
pub fn bind(...)
pub fn match_pattern(...)
pub fn destructure(...)
pub fn compose(...)
pub fn enumerate_slots(...)
pub fn enumerate_paths(...)
```

This produces a reusable type/value bridge: the declaration is a type-level pattern, binding creates a value, matching validates a value and recovers bindings, and enumeration exposes the pattern's typed structure to generators and rules.

## Type-system surface

### Present in both

- Named records and aliases
- Arrays and JSON-style key/value structures
- Optional nested values
- String literal unions
- Nested type references
- Semantic traversal for generation
- Multiple outputs derived from one schema

### TypeSpec surface absent from the lab

- Custom scalar declarations and inheritance
- Numeric, boolean, and null literal syntax
- General named unions and explicit enums
- Intersections
- User-defined templates/generics
- Template defaults, constraints, named arguments, and value parameters
- Model spread, `extends`, and `is`
- First-class operations and interfaces
- Imports, namespaces, packages, and access modifiers
- Decorators, augment decorators, functions, and library state
- Object/array constants, defaults, examples, and `typeof`
- Lifecycle visibility and metadata projection
- Documentation and validation decorators
- Versioning and protocol libraries
- Public custom-emitter API
- Formatter, linter, language server, and package tooling

TypeSpec template parameters can target aliases, models, operations, and interfaces and support constraints, defaults, named arguments, and values. See [Templates](https://typespec.io/docs/language-basics/templates/). TypeSpec models also support optional/default properties and three composition mechanisms. See [Models](https://typespec.io/docs/language-basics/models/).

### Lab surface absent from documented TypeSpec language operations

- Two normalized slot syntaxes, `{name: Type}` and `:name`
- Bidirectional typed pattern evaluation
- Pattern composition with duplicate-binding checks
- Generic delimiter treatment across protocols and structural paths
- First-class slot enumeration
- JSON structural path enumeration using `[*]` and `{key}` templates
- Direct fact projection from types, fields, slots, consumers, and paths
- Generated native Rust matcher and server in the prototype itself
- Explicit stage-zero bootstrap artifact and trust-boundary report

## Facts and rules

The lab projects the semantic store into these facts in [`src/10_facts.rs`](../labs/bootstrap-typegen-lab/src/10_facts.rs):

```text
TypeKind
Field
SlotType
Consumer
Path
```

[`src/11_rules.rs`](../labs/bootstrap-typegen-lab/src/11_rules.rs) performs one deterministic population pass. There is currently no parser for user-authored rules, recursion, joins, antijoins, aggregation, provenance, or incremental maintenance. The Datalog-shaped part is therefore a stable relational view over the compiler store rather than a general logic engine.

TypeSpec extension code uses decorators, program state maps, compiler reflection, semantic walkers, and emitter traversal. These APIs provide programmable semantic queries in JavaScript. They do not expose a Datalog source language in the documented compiler surface. See [Decorators](https://typespec.io/docs/language-basics/decorators/) and [Emitters](https://typespec.io/docs/extending-typespec/emitters-basics/).

## Code generation

The lab currently emits:

- Rust structs, aliases, and enums
- JavaScript model JSDoc
- JavaScript fetch functions
- A std-only Rust HTTP server
- Typed runtime path matchers
- A smoke client
- `facts.txt`
- Stage-zero semantic model Rust source
- `bootstrap-boundary.txt`

TypeSpec supplies a compiler API, semantic walker, custom traversal, and emitter framework. Current standard emitters cover OpenAPI, JSON Schema, and Protobuf. The OpenAPI emitter accepts 3.0.0, 3.1.0, and 3.2.0 output selections. See [Emitter basics](https://typespec.io/docs/extending-typespec/emitters-basics/) and [OpenAPI emitter options](https://typespec.io/docs/emitters/openapi3/reference/emitter/).

## Bootstrap boundary

The lab's `bootstrap` command generates Rust definitions for its semantic model from the schema. Parsing, lowering, evaluation, rule projection, and code emitters remain handwritten Rust modules. [`src/14_bootstrap.rs`](../labs/bootstrap-typegen-lab/src/14_bootstrap.rs) writes this boundary explicitly.

A full bootstrap would require the schema language to describe enough of the compiler to regenerate at least:

1. Syntax and semantic node definitions.
2. Pattern and type checking tables or rules.
3. Rust emission for those definitions and rules.
4. A stage comparison proving that generated stage one regenerates an equivalent stage two.

The current prototype reaches item 1 for semantic model definitions. Its parser and emitter algorithms have no schema representation yet.

## Current test evidence

The implementation review recorded:

- 10 lab tests passing
- Generated Rust server compiling independently with `rustc`
- Generated server matcher test passing
- Generated JavaScript client passing `node --check`
- Live generated client-to-generated-server request returning the generated `User` JSON shape

The test source is [`src/main.rs`](../labs/bootstrap-typegen-lab/src/main.rs). The README contains reproduction commands.

## Documentation inventory used

TypeSpec documentation sections consulted:

- [Language overview](https://typespec.io/docs/language-basics/overview/)
- [Models](https://typespec.io/docs/language-basics/models/)
- [Operations](https://typespec.io/docs/language-basics/operations/)
- [Unions](https://typespec.io/docs/language-basics/unions/)
- [Templates](https://typespec.io/docs/language-basics/templates/)
- [Type literals](https://typespec.io/docs/language-basics/type-literals/)
- [Values](https://typespec.io/docs/language-basics/values/)
- [Visibility](https://typespec.io/docs/language-basics/visibility/)
- [Emitter basics](https://typespec.io/docs/extending-typespec/emitters-basics/)
- [OpenAPI emitter reference](https://typespec.io/docs/emitters/openapi3/reference/)
- [Protobuf guide](https://typespec.io/docs/emitters/protobuf/guide/)

Local implementation sections consulted:

- [`README.md`](../labs/bootstrap-typegen-lab/README.md)
- [`schema.dl`](../labs/bootstrap-typegen-lab/schema.dl)
- [`src/4_parser.rs`](../labs/bootstrap-typegen-lab/src/4_parser.rs)
- [`src/7_store.rs`](../labs/bootstrap-typegen-lab/src/7_store.rs)
- [`src/9_eval.rs`](../labs/bootstrap-typegen-lab/src/9_eval.rs)
- [`src/10_facts.rs`](../labs/bootstrap-typegen-lab/src/10_facts.rs)
- [`src/11_rules.rs`](../labs/bootstrap-typegen-lab/src/11_rules.rs)
- [`src/12_codegen_rust.rs`](../labs/bootstrap-typegen-lab/src/12_codegen_rust.rs)
- [`src/13_codegen_js.rs`](../labs/bootstrap-typegen-lab/src/13_codegen_js.rs)
- [`src/14_bootstrap.rs`](../labs/bootstrap-typegen-lab/src/14_bootstrap.rs)
- [`src/15_cli.rs`](../labs/bootstrap-typegen-lab/src/15_cli.rs)

## Research gaps

- No current TypeSpec issue and discussion census was used. Search results did not surface an authoritative issue describing a general bidirectional typed string-pattern facility.
- TypeSpec's complete package ecosystem and SDK emitters were outside this comparison.
- The archived `claude-research` notes contain version-specific observations about property access and experimental functions. Those observations were excluded until rechecked against TypeSpec 1.14.0.
- Memory and throughput comparisons between the Rust lab and TypeSpec were not run. Their implementation scope and workloads differ enough that a direct process-memory number would need a shared schema corpus and output contract.
