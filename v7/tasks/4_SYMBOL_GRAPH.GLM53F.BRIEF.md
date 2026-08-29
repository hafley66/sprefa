## Description

Lower prefix forms into module, product, callable, colon-edge, and normalized
fact/rule rows. Resolve names by reading colon edges from the current owner.

## Signature

```prolog
lower_dl7(+ModulePath, +Forms, -Rules, -Seeds, -Requests, -Diagnostics).
```

## Timeline and storage

Reserve top-level names, construct product owners, resolve targets, then lower
applications by appending the declared return position.

## Acceptance Criteria

- [ ] Canonical edge rows are `':'(Owner, Name, Target, Index)`.
- [ ] `(Owner, Name)` and `(Owner, Index)` are functional keys.
- [ ] No `member`, `binding`, synthetic edge ID, or public application wrapper.
- [ ] Value and type calls use one application-lowering predicate.
- [ ] Production changes stay under `v7/1_KERNEL/`.
- [ ] Adds no standalone test file.

## Test Run

Use one direct SWI receipt until the single oracle lands.

## Stop condition

Hail the parent when a missing callable declaration or node-identity ruling
would require inference.
