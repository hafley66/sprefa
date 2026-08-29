# Pairs with corpus_8.py. See that file for the expected fact.
from .corpus_8 import Widget


def build():
    w = Widget()
    return w.draw()
