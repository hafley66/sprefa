# CPython stdlib shape, 43 occurrences over 41 files.
# EXPECTED: one specifier row  kind=named name=annotations module=__future__.
# Observed before the fix: zero specifier rows; tree-sitter parses this as
# `future_import_statement`, a node kind `py_walk_imports` never matched.
from __future__ import annotations, generator_stop
