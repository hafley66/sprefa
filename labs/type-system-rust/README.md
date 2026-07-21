# Rust type-system lab

Standalone reconnaissance application for the type-system/tooling direction.

Run the normal demo:

    cargo run --manifest-path labs/type-system-rust/Cargo.toml

Run the stress demo:

    cargo run --release --manifest-path labs/type-system-rust/Cargo.toml -- --stress arena-lasso 100000 deep

Compare storage variants:

    cargo run --release --manifest-path labs/type-system-rust/Cargo.toml -- --stress flat-lasso 100000 repeated
    cargo run --release --manifest-path labs/type-system-rust/Cargo.toml -- --stress flat-lasso 100000 unique
    cargo run --release --manifest-path labs/type-system-rust/Cargo.toml -- --stress flat-strings 100000 repeated
    cargo run --release --manifest-path labs/type-system-rust/Cargo.toml -- --stress flat-lasso 1000 wide

The program exercises:

- ena type-variable unification
- la-arena declaration storage
- lasso symbol interning
- nested records, arrays, maps, optionals, unions, and generic application
- recursive dotted-path enumeration with array and map wildcards
- serde_json serialization
- miette diagnostics

Stress variants are arena-lasso, flat-lasso, and flat-strings. Workloads are
repeated, unique, wide, deep, unions, and generic. The process includes a
measuring global allocator so peak allocated bytes are reported even when the
host blocks ps and RSS inspection.

This crate is outside the workspace member list and imports no sprefa code.
