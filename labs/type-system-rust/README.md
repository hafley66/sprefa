# Rust type-system lab

Standalone reconnaissance application for the type-system/tooling direction.

Run the normal demo:

    cargo run --manifest-path labs/type-system-rust/Cargo.toml

Run the stress demo:

    /usr/bin/time -l cargo run --release --manifest-path labs/type-system-rust/Cargo.toml -- --stress 100000

The program exercises:

- ena type-variable unification
- la-arena declaration storage
- lasso symbol interning
- nested records, arrays, maps, optionals, unions, and generic application
- recursive dotted-path enumeration with array and map wildcards
- serde_json serialization
- miette diagnostics

This crate is outside the workspace member list and imports no sprefa code.
