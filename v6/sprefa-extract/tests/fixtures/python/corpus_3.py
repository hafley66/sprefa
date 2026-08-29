# 3194 occurrences over 547 stdlib files.
# EXPECTED: `total += size(n)` mints a let_bind for `total` fed by the
# call_res of `size(n)`, and `return total` reads THAT binding.
# Observed before the fix: no let_bind, the call_res dangled with no consumer,
# and the return read the `total = 0` binding, attributing a stale value.
def run(n):
    total = 0
    total += size(n)
    return total
