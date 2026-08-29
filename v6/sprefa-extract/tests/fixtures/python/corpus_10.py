# PEP 750 t-strings, Python 3.14. Valid source that tree-sitter-python 0.23.6
# cannot parse: it yields an ERROR node at the `t` prefix and every fact for
# the enclosing scope is lost. 67 ERROR nodes over 5 stdlib files.
# NOT fixed here: the grammar version is pinned in Cargo.toml, outside
# src/lang/python.
def render(name):
    return t"hello {name}"
