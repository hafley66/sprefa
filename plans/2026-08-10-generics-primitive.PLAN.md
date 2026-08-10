# Generics primitive lab

## Implemented path

`0_generic_expand.pl` runs through the retained phase-5 `option` slot before enum expansion.  It collects
`list(T)` occurrences nested under `option(...)`, sorts unique normalized
instances, emits typed `artifact(decl(...))` records, lowers them to
declarations, then delegates option declarations to `0_option_expand.pl`.
Enum remains the phase-10 adapter.  The current list template is a LAB
STANDIN: `id:int`, `value:text`, and key `[1]`; it supplies an identity for the
option reference-companion path only.

## Naming

`canonical_type_encoding/2` is length-prefixed structural input.  The emitted
name is a readable stem plus the first 16 hex characters of SHA-256:

    canonical_type_name(option(list(text)), Name)
    Name = '__gen__option_list_text_0d79110cd49d2728'

The full encoded term feeds the digest.  The expansion pass checks every
generated name against author declaration and rule names before lowering and
throws `generic_generated_name_collision(Name)`.  It also checks that distinct
generic instances did not receive the same truncated name.  Name generation
reads no counters, fixture paths, or declaration order.

## Fixture receipt

`0_generic_expand.pl` contains one oracle fixture.  It includes an enum,
two `option(text)` parents, two `option(int)` parents, one option relation
companion, `option(list(text))`, minted-relation reads, author-derived rows,
and arrivals plus retractions over five ticks.  Its full expanded term is
`0_generic_expand.golden`.

## Validation

    swipl -q -l v6/prolog/conformance/go.pl -g go -g halt
    swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt

The corpus-count receipt changes `refcount` and `refcount_staging` from 313 to
320 because the new fixture participates in the catalog scan.

2026-08-10 runs:

    swipl -q -l v6/prolog/conformance/go.pl -g go -g halt
    # PASS, including generic_expansion_end_to_end

    swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt
    # 574 tests run; 22 failures outside the generic-expansion focused group

    just sweep
    # RUN total=247 identical=246 wrong=0 emitted_crash=0 rejection=1

The sweep's compile stage records one Unicode-output crash for
`json_nfc_and_nfd_keys_stay_distinct` under the available locale.  It added
the generic fixture to the manifest and produced 246 runtime matches.  Sweep
output changes, including its deletion of `compile/out/pokeapi_shape.ts`, were
restored after the run.

## Fork matrix and receipts

| Fork | Probe | Receipt | Retained arm |
| --- | --- | --- | --- |
| Template vocabulary | `expand_generic_program/2` versus `expand_generic_program_raw/2` over the complete fixture | both emit 39 declarations and the same 1,340-byte canonical declaration term | typed `artifact(decl(...))`; one lowering boundary |
| Artifact order | global canonical sort versus dependency topo plus canonical tie-break | the current list template reports no generated-template dependencies; an edgeless graph's topo tie-break is the global sort | canonical sort, represented by `generic_artifact_order/3` |
| Name encoding | full digest versus readable stem plus 16-hex digest prefix | golden name is `__gen__list_text_df210f232c1299bd`; a generated-name collision has a named throw check | 64-bit digest prefix with expansion-time collision check |
| Placement | generic pass before enum versus enum folded into generic fixpoint | generic uses phase 5 and enum uses phase 10; complete oracle fixture passes | phase 5 before enum |

The declaration-permutation receipt reverses all declarations of
`generic_expansion_end_to_end`, runs the complete expansion driver twice, and
compares `term_string/2` byte-for-byte.  The test is
`generic_e2e_declaration_permutation_is_byte_deterministic`.

Rules remain author-written in round 1.  Templates emit declarations only.
