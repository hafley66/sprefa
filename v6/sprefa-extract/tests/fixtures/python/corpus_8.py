# Pairs with corpus_9.py under `extract --resolve`.
# EXPECTED (FLIPPED 2026-08-31, python ctor resolution): the `Widget()` edge
# from corpus_9.py carries callee_name="__init__" — PyCG oracle semantics: a
# constructor call resolves to the class's __init__ method, never to the class
# def itself (a class-name callee row is a bench false positive).
# Previous pin: callee_name="Widget" (the class TypeF def), chosen because a
# null callee_name breaks the dl6 4-col join. The null defect stays pinned by
# the `!contains("callee_name":null)` assertion; the class-name row is gone.
class Widget:
    def __init__(self):
        self.ready = True

    def draw(self):
        return 1
