# Facet to Datalog-style facts lab

Standalone experiment. It has no dependency on the `sprefa` package or the
repository workspace.

Run it with:

```sh
cargo run --manifest-path labs/facet-datalog-lab/Cargo.toml
```

The Rust input types are ordinary `#[derive(Facet)]` declarations:

```rust
struct User {
    id: String,
    profile: Profile,
    orders: Vec<Order>,
    metadata: HashMap<String, String>,
}
```

The lowering pass consumes `User::SHAPE` and `Page::<User>::SHAPE`. It emits
four relation-shaped fact families:

- `Type(type, kind)`
- `Field(owner, field, type)`
- `Collection(owner, path, kind, element_type)`
- `Path(root, path_template, leaf_type)`

The executable demonstrates these queries:

```text
Path(User, "orders[*].id", String)
Path(User, "metadata{key}", String)
Path(Page<User>, "items[*].profile.avatar", String)
```

The path syntax is deliberately data-shaped. A later DSL can make these facts
the output of rules such as:

```text
path(Root, P, T) :- field(Root, Name, FieldType), join(P, Name, Tail), path(FieldType, Tail, T).
path(Root, P[*], T) :- list(Root, Element), path(Element, P, T).
path(Root, P{key}, T) :- map(Root, Value), path(Value, P, T).
```

Facet supplies the Rust-side reflection graph, including generic applications
such as `Page<User>`, struct fields, list element shapes, map key/value shapes,
and option inner shapes. The lab's `Fact` enum is the temporary interchange
layer. A real implementation can replace the vector scan with a relational
engine once the fact vocabulary and rule semantics settle.

The JSON section is a serialization check for the same fact graph. It makes the
intermediate representation inspectable by TypeScript tooling or a future
language server without moving the type declarations into JSON first.
