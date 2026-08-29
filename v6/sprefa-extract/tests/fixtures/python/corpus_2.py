# PEP 695, 54 occurrences over 9 stdlib files.
# EXPECTED: two type entities  kind=alias name=Alias and kind=alias name=Pair.
# Observed before the fix: neither; `type_alias_statement` reached no arm of
# `walk_py_entities`, so a named type declared this way was invisible.
type Alias = list[int]
type Pair[T] = tuple[T, T]
