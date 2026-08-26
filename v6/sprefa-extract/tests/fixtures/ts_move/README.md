# ts_move fixture

The resolution cases `lang/ts_resolve.rs` is graded on. `vendor/` is copied to
`node_modules/` at test setup: this repository gitignores `node_modules` at any
depth (`.gitignore:107`), so the package cannot be committed under that name.
