# Soufflé rule layer

`schema.dl` is the Datalog half of the isolated lab. It uses the same fact
vocabulary produced conceptually by the Rust Facet lowering pass:

```text
type_decl(name, kind)
field(owner, segment, child)
array(container, element)
map(container, value)
scalar(name)
```

The recursive rules derive path templates such as:

```text
User, id, String
User, orders[*].id, String
User, metadata{key}, String
Page<User>, items[*].profile.avatar, String
```

Run it when the Soufflé compiler is installed:

```sh
souffle -c schema.dl
./schema
```

The local environment used for this experiment does not currently have the
`souffle` executable, so this file is checked as source structure only. The
Rust executable remains runnable without it:

```sh
cargo run --manifest-path ../Cargo.toml
```
