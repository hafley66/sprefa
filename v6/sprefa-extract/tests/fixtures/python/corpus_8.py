# Pairs with corpus_9.py under `extract --resolve`.
# EXPECTED: the `Widget()` edge from corpus_9.py carries callee_name="Widget".
# Observed: callee_name is null. A class is not minted as a CallF def, so the
# shared def index answers with its TypeF site and `name_at` at
# src/project.rs:889 probes only the CallF span table. NOT fixed here: the
# name lookup lives outside src/lang/python.
class Widget:
    def draw(self):
        return 1
