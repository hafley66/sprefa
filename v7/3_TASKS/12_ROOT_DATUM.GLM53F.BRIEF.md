# GLM53F brief: finish DL7 root datums

Read `v7/2_DESIGN/2_BASEMENT_TO_DATALOG.PLAN.md` first and implement milestone
1 only.

## Scope

- Work only in the assigned worktree.
- Modify only the four `v7/0_SWIPL` files permitted by milestone 1 and
  `v7/3_TASKS/00_PROGRESS.md`.
- Add `'name` as `literal(symbol(Name))` without adding list quotation,
  macros, booleans, floats, dotted names, or semantic name resolution.
- Pin empty forms, nested forms, bare atoms, symbol data, and variable sharing
  in the existing snapshot test. Add no test file.

## Gates

Run once:

```text
swipl -q -g "load_files(['v7/0_SWIPL/test/0_reader.test.pl'],[silent(true)]),run_tests,halt"
```

Run `git diff --check`. Run no other suite.

## Commit

Create at least one commit with exact subject:

```text
v7: finish root datum reader
```

Add trailer `Refs-Issue: @dl7-root-datums`. Do not push. If the exact symbol
lexing conflicts with the current reader grammar, stop and report the node
shape and source span involved instead of inventing syntax.
