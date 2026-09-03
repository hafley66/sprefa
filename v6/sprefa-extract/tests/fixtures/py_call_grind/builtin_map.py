def func(item):
    pass

def func2(item):
    pass

def keep(item):
    return item

map(func, [1, 2, 3])
map([1, 2, 3], func2)
list(filter(keep, [1, 2, 3]))
