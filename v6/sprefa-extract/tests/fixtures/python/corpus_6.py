# PEP 572, 274 occurrences over 115 stdlib files.
# EXPECTED: `size := len(items)` mints a let_bind for `size` fed by the
# call_res of `len(items)`, and the `return size` reads that binding.
# Observed before the fix: `named_expression` bound nothing.
def measure(items):
    if (size := len(items)) > 0:
        return size
    return 0
