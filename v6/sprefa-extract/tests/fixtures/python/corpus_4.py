# 17639 with-statements over 847 files; 6137 of them bind an `as` name.
# EXPECTED: `open(path)` mints a call_res, `fh` mints a let_bind fed by it,
# and `fh.read()` reads that binding.
# Observed before the fix: the context expression produced no df nodes at all
# and `fh` was an unbound var_read.
def load(path):
    with open(path) as fh:
        return fh.read()
