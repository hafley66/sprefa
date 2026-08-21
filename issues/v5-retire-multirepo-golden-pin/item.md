---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: high
epic: usurp-v4-v5
---

## Description

`just multirepo-golden` is the ONE live green gate that still requires the v5
binary. It is the last automated consumer of v5, and removing its dependency is
the day v5 stops building.

## Receipts

| fact | receipt |
|---|---|
| the gate hard-fails without the binary | `v6/tsv2/goldens/multirepo_crawl/2_gate.sh:132-141` `resolve_v5_bin`, "no v5 dl binary at $release. A gate does not build" |
| it is a `green-all` leg | `v6/tools/green-parallel.sh:34` |
| it is NOT on the known-red allowlist | `.github/CI-KNOWN-RED.md:142-159` |
| **the v5 side is already a checked-in file** | `2_gate.sh:5-6`: "v5: CHECKED-IN OUTPUT of the root `dl` binary running `examples/version-skew.dl` BYTE-UNMODIFIED, pinned under ./v5_golden/" |
| CI has run no v5 gate since 2026-08-11 | `.github/workflows/ci.yml:3` |
| the only other v5-binary gate leg is already red-and-allowed | `.github/CI-KNOWN-RED.md:115`, `allow: flagship` at `:146` |

## The change

The gate reads a saved golden and compares v6 against it. It does not need to
re-run v5 to do that; it resolves the binary and then never uses it for the
comparison legs. Delete `resolve_v5_bin` and every call to it, and add a content
digest assertion on `v5_golden/` so the saved answer cannot drift silently:

```bash
# today
resolve_v5_bin            # fails if target/release/dl is absent
...compare v6 against ./v5_golden/...

# after
assert_golden_digest      # shasum of ./v5_golden/, pinned in the manifest
...compare v6 against ./v5_golden/...
```

The corpus digest is already asserted (`2_gate.sh:16` "The corpus digest and the
v5 program's blob sha are asserted before anything"), so the shape exists; this
adds the same treatment to the golden itself and drops the binary lookup.

If any leg genuinely re-runs v5 (read the script before assuming it does not),
that leg becomes a checked-in expectation the same way, or it is deleted with
its `allow:` line the way `lsp-diags` and `flagship-flow` already were
(`.github/CI-KNOWN-RED.md:39-41`).

## Gate

```bash
mv target/release/dl /tmp/dl-hidden 2>/dev/null || true
cd v6 && timeout 900 just multirepo-golden        # must stay green
```

## Then

`plans/2026-08-21-v5-retirement.PLAN.md` steps 2 to 9 follow. Step 5 is the only
one that needs Chris.
