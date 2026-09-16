# dl8 at the repository root. Earlier engines: `just v5 <recipe>`, `just v6 <recipe>`.

default:
    @just --list

build:
    cargo build

test *args:
    cargo test --no-fail-fast {{args}}

# Build the sqlite_ivm loadable extension the `_18_sqlite_emit` tests load.
ivm-ext:
    cargo build --release --features extension --manifest-path sqlite_ivm/Cargo.toml

book:
    mdbook build book

v5 *args:
    just --justfile v5/justfile --working-directory v5 {{args}}

v6 *args:
    just --justfile v6/justfile --working-directory v6 {{args}}
