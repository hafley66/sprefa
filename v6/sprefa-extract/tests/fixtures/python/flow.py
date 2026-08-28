def total(items, factor):
    acc = 0
    for item in items:
        acc = acc + item * factor
    while acc > 100:
        acc = acc - 1
    squares = [n * n for n in items if n > 0]
    table = {"k": acc, **squares}
    pair = (acc, factor)
    pick = acc if factor else 0
    double = lambda v: v * 2
    out = double(acc)
    box = Box(acc, label=pick)
    name = box.label
    first = items[0]
    return -acc


def walk(tree):
    def inner(node):
        return node
    return inner(tree)
