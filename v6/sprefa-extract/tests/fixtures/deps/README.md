The diet module resolver's fixture corpus. `app.ts` writes one specifier per
resolution policy in `src/deps.rs`, so the CLI golden in `7_diet_deps_cli.rs`
covers every rule the resolver applies:

  ./lib/util.ts    relative_exact              (and a second name via export-from)
  ./lib/helper.js  relative_emitted_rewrite    (NodeNext names the emitted file)
  ./lib/bare       relative_extension_inferred
  ./widget         relative_index_file
  @app/mapped      tsconfig_paths              (via the paths pattern below)
  rxjs             node_modules_boundary       (a stated stop, no edge)
  ./gone.ts        relative_unresolved         (no such file, no edge)
  ./side.ts        a side-effect import, which still carries a module

`tsconfig.json` deliberately carries a comment and trailing commas: real
tsconfigs do, and the reader has to survive them or every bare specifier would
silently downgrade to the node_modules boundary.
