# Bootstrap typegen lab

This is a standalone single-binary experiment. It does not import `sprefa` and
does not invoke Soufflé.

The Rust compiler binary reads `schema.dl`, parses its own type vocabulary and
the application API vocabulary, then emits:

```text
target/bootstrap-generated/models.rs
target/bootstrap-generated/server.rs
target/bootstrap-generated/client.mjs
target/bootstrap-generated/client-smoke.mjs
target/bootstrap-generated/stage1.rs
```

The schema declares the compiler-facing types first:

```text
type TypeDecl { ... }
type Field { ... }
type Endpoint { ... }
```

It then declares `User` and a `GET /users/{id}` endpoint. The same internal
type graph drives all three outputs. The generated server uses only
`std::net::TcpListener`, so the generated API has no runtime dependency.

Run the compiler:

```sh
cargo run --manifest-path labs/bootstrap-typegen-lab/Cargo.toml
```

Compile and run the generated server:

```sh
rustc labs/bootstrap-typegen-lab/target/bootstrap-generated/server.rs \
  -o labs/bootstrap-typegen-lab/target/bootstrap-generated/server
labs/bootstrap-typegen-lab/target/bootstrap-generated/server
```

Then request the generated endpoint:

```sh
curl http://127.0.0.1:4000/users/abc
```

The generated `stage1.rs` contains the generated `TypeDecl`, `Field`, and
`Endpoint` structs and a parser that uses those generated structs. Compile and
run it:

```sh
rustc labs/bootstrap-typegen-lab/target/bootstrap-generated/stage1.rs \
  -o labs/bootstrap-typegen-lab/target/bootstrap-generated/stage1
labs/bootstrap-typegen-lab/target/bootstrap-generated/stage1 \
  labs/bootstrap-typegen-lab/schema.dl
```

The generated `client-smoke.mjs` imports the generated client and calls the
generated endpoint. With the generated server running, execute:

```sh
node labs/bootstrap-typegen-lab/target/bootstrap-generated/client-smoke.mjs
```

This is a stage-1 bootstrap: the stage-0 compiler still contains the parser
template, while the stage-1 compiler parses using the generated model types.
