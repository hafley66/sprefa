# 31687 if-statements over 1417 files, 1371 asserts over 338 files.
# EXPECTED: the `if` condition, the `elif` condition, the assert expression and
# the raise operand each flow, so ready / pending / valid mint call_res and
# Failure mints a `new`.
# Observed before the fix: none of the four produced any df node; only the
# `while` condition was walked.
def gate(job):
    if ready(job):
        return 1
    elif pending(job):
        return 2
    assert valid(job)
    raise Failure(job)
