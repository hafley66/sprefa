# option(T) moves to the standard library

User decision 2026-08-14: "I would like to host option in the standard
library, but we would have to make it possible." This plan prices "possible":
option becomes an ordinary enum template in a stdlib `.dl6` file, and the
compiler builtin retires. Target spelling, under the bound-in-parens decision
(`rulings.pl: template_bound_spelling`):

```
rel option(T)(some(value: T) ; none).
```

## TOC
1. What option is today (receipts)
2. The three compiler special cases that need userland spellings
3. Slices
4. Gates and the parity law
5. Forks needing the user

## 1. What option is today

| fact | where |
|---|---|
| closed wrapper inventory, beside the list flavors | `0_type_plane.pl:147` `type_wrapper(option, endpoint)` |
| desugars at expansion phase 5, before the scope tree exists (dot resolution is phase 44) | `1_expansion.pl:31,44` |
| scalar elements: `__opt_<t>` enum + companion split rel; reference elements: companion rel via `desugar_reference_option` | `0_option_expand.pl:39-49` |
| `option(<enum>)` throws | `0_option_expand.pl:45` |
| self/rel reference option compiles but mints two same-named fk columns (open defect) | probed 2026-08-14, `desugar_reference_option` |
| schema boundary: option columns drop from `required` | `4_emit_jsonschema.pl:121` |
| semantic rows ride `option_column/3` markers merged late | `0_enum_expand.pl` `merge_option_type_rows/2` (unity C2) |
| enum templates do not exist: `rel_template` bodies are columns only | `parse_dl_dcg.pl:477` surface, template instantiation in `0_generic_expand.pl:401-458` substitutes column specs, no variant arm |

## 2. The three special cases, each with the spelling that retires it

**2a. Representation: the scalar-vs-reference split.** Today the compiler
picks storage by element kind. A library enum gets ONE uniform lowering (tag +
variant tables). The specializations survive as SHAPE-KEYED optimizations: any
enum with exactly one nullary variant and one single-payload variant (the
option shape, recognized structurally) is eligible for (i) scalar payloads:
nullable-column storage, the current `__opt_<t>` layout, and (ii) reference
payloads: the companion-rel layout. Keying on shape means any user enum of
that shape gets the same treatment — the optimization stops being option's
private privilege. rx lowering unchanged: the tag join is a filter+map either
way.

**2b. The JSON boundary: null projection.** Today `option(text)` renders
`value | null`; a plain enum renders tagged objects. The shape recognition in
2a also licenses the boundary projection: the option-shaped enum renders its
payload-or-null instead of `{"some": ...}`. The "key absent vs present-null"
gap (CLAUDE.md open item) stays exactly as open as it is now; this arc neither
fixes nor worsens it.

**2c. Identity and origin rows.** The wrapper-inventory row and
`option_column/3` markers retire; option instances become ordinary template
instances and ride the C2/C3 semantic-row machinery that already exists.
Emitters that consume `option_column/3` (schema fold at
`4_emit_jsonschema.pl:121`, TS/Rust type rendering) re-key on the structural
recognition from 2a.

## 3. Slices, dependency-ordered

1. **Enum templates.** `rel option(T)(some(value: T) ; none).` parses
   (parameter parens + variant body), `0_generic_expand.pl` instantiation
   substitutes T inside variant payload specs, minted enum decls flow into
   enum expansion (phase order 5 -> 10 already correct). Conformance fixtures:
   a user-defined `result(T)`-style enum template FIRST, so the machinery
   lands without touching option at all.
2. **Shape recognition.** `option_shaped(EnumName, PayloadType)` derived
   structurally; the scalar/reference storage specializations re-keyed on it;
   plunit pins that a user enum of the same shape gets the same DDL.
3. **Boundary projection.** JSON render + jsonschema + TS/Rust emitters keyed
   on `option_shaped`; byte-parity ratchet against every existing option
   fixture.
4. **The stdlib file and the mount.** `std.dl6` carrying `option(T)` (and
   nothing else, first landing); resolution rides the qualified-type-path
   machinery (the `use "..." as name` mounts + `type_path` resolution in
   `0_dot_expand.pl`/`parse_dl_dcg.pl`, in flight on this branch). Bare
   `option(` resolves prelude-style so no existing program changes spelling.
5. **Retire `0_option_expand.pl`.** The phase-5 wrapper row goes; the
   `option(<enum>)` throw and the self-ref duplicate-column defect die with
   it. Every option conformance fixture must grade byte-identical, or each
   diff is named per artifact in the landing report.

## 4. Gates and the parity law

Every slice: the full battery (conformance, sweep, plunit exactly
CI-KNOWN-RED, RUST-GRADE ratchet, typegen golden). Slices 3 and 5 carry the
hard law: existing option fixtures byte-identical, any exception named
per-fixture per-artifact with the cause. Baselines at this plan's writing:
conformance 428/0, sweep RUN 324/321 wrong=0, grade 428/320.

## 5. Forks needing the user

| fork | options |
|---|---|
| prelude vs explicit mount | bare `option(` auto-resolves to std (zero migration) vs `use "std.dl6" as std` + `std.option(` everywhere (explicit, breaks every fixture) |
| shape recognition scope | option-shape only, or generalize to any single-payload+nullary enum from day one |
| stdlib contents at first landing | option alone, or option + result(T, E) to prove the machinery is generic |
| `option(list(T))` and `option(<enum>)` | both become ordinary nested instantiations once 1-2 land; conformance fixtures for each, or deferred |
