# 1580 `except ... as name` handlers over 375 stdlib files.
# EXPECTED: `err` mints a let_bind that `report(err)` reads.
# Observed before the fix: no binding, so the handler body read a free name.
def guard(work):
    try:
        return work()
    except ValueError as err:
        return report(err)
