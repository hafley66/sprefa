# 2026-08-17 language stress probes

Hand programs pairing constructs the 452-fixture corpus never composes.
Report: `docs/audits/2026-08-17-lang-stress.md`.

| file | role |
|---|---|
| `p*.dl6` | one construct PAIR each, chosen from the 71 pairs `coverage_matrix.py` reports as never co-occurring |
| `n*.dl6` | narrowing controls that isolate which half of a `p*` result caused it |
| `run.sh` | compile every probe on the text door, one row per probe, buckets matching `sweep.pl` |
| `coverage_matrix.py` | which construct pairs the committed corpus already composes |
| `classify_unsupported.py` | split the manifest's `unsupported` bucket into both-doors-agree vs door split |

These are PROBES, not fixtures. They carry no expectations and no gate reads
them. Three of them (`p21`, `n9`, `n10`) compile and produce a wrong answer;
they stay here as the reproduction until card `lang-enum-column-coercion`
turns them into conformance fixtures.

    bash v6/prolog/conformance/probes/2026-08-17-stress/run.sh
    cd v6/prolog && python3 conformance/probes/2026-08-17-stress/coverage_matrix.py
    cd v6/prolog && python3 conformance/probes/2026-08-17-stress/classify_unsupported.py
