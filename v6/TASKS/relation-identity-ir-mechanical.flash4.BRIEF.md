# 041a stage 1: centralize existing relation target selection

Make this exact small refactor, test it, and commit it. Do not read plans, issue cards, history, fixtures, or unrelated modules.

1. In `v6/prolog/0_rel_record.pl`, export and implement:

```prolog
relplan_reference_target(RelPlans, TargetName)
relplan_reference_targets(RelPlans, TargetNames)
```

`relplan_reference_target/2` succeeds for each `TargetName` occurring as `ref(TargetName)` in any rel plan column storage type. `relplan_reference_targets/2` returns the sorted unique set, including `[]` when absent.

2. In `v6/prolog/lower.pl`, import `relplan_reference_target/2`. Replace the duplicated scan inside local `reference_target_ref/2` with delegation to the shared predicate. Keep `reference_target_ref/2` if its `Name/Arity` interface reduces call-site churn.

3. In `v6/prolog/compile/test/plunit_tests.pl`, add one focused unit beside existing rel-record tests. Construct synthetic rel plans that prove:

- duplicate `ref(span)` columns return one target from the set API
- `ref(person)` is also returned
- scalar, list-container, keyed, log, and level-shaped plans do not become targets merely because of kind or key
- result ordering is deterministic

4. Run only the new PlUnit unit and `git diff --check`.

5. Commit with subject `dl6: centralize relation identity targets` and trailer `Refs-Issue: @relation-identity-ir`.

Report the commit hash and exact test command. Do not attempt wrapper expansion in this stage.
