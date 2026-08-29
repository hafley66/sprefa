## Description

Implement one smoke artifact only if the Terra seam report identifies an
existing engine command requiring zero Rust changes.

## Acceptance Criteria

- [ ] Zero files under `v6/sprefa-engine-rs` change.
- [ ] Zero Rust files are added under V7.
- [ ] Existing engine command consumes one V7-generated temporary artifact.
- [ ] Output is compared exactly.
- [ ] No additional test file is created when the kernel oracle can host the
      command.

## Test Run

Run one exact engine command once. Run no suite.

## Stop condition

Record the blocker and add no workaround when engine or TS code would change.
