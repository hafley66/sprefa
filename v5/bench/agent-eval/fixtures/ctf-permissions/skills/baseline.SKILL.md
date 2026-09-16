# Working in this repository — tooling guide

You have shell access with the standard read/search tools: `bash`, `grep`
(ripgrep `rg` is available and faster), `glob`, and file `read`. Use them to
navigate and answer the question. There is no build step required to answer —
you do NOT need to run `cargo`, `tsc`, `npm`, or any compiler, and doing so
wastes your time budget.

## The codebase

A small permission-gated service with a **TypeScript** app (`app-ts/`) and a
**Rust** app (`app-rs/`). Permission enforcement is spread across both.

## Suggested method (count first, then verify)

1. **Enumerate before you read.** Start with a broad search to build a
   candidate list, then open only the files that matter. Example:
   `rg -n "canExport|can_export|PERM_EXPORT|CanExport|export" app-ts app-rs`.
2. **Follow the indirection.** A permission is often referenced through a
   named constant or enum, not the literal string. If you find
   `const PERM_EXPORT = "can_export"`, then search for `PERM_EXPORT` to find
   the sites that USE it — the string itself won't appear there. Likewise a
   Rust `Permission::CanExport` enum value, or a helper function like
   `require_export` / `check_export_flag`, marks a gate without the literal.
3. **Trace helper calls.** When enforcement is wrapped in a function (a guard,
   a service method, a config-rule check), grep for CALLS to that function to
   find every gate site: `rg -n "require_export|check_export_flag" app-rs`.
4. **Distinguish enforcement from decoys.** Names containing "export" are not
   all permission checks. A UI feature toggle (should a button render), a
   build flag, or an unrelated field is NOT an enforcement site. Read the
   surrounding code: an enforcement site DENIES a request (throws, returns an
   error, sends 403) when the permission is absent.
5. **Both languages.** Check `app-ts/` and `app-rs/`. The same concept is
   gated in each.

## Answer format

Report line numbers as they appear in the file (1-based). Follow the exact JSON
output shape the question asks for. Count your sites before answering — missing
a gate hurts recall, listing a decoy hurts precision.
