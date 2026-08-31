def func1():
    pass

def func2():
    pass

g = func1
g()

a, b = func1, func2
a()
b()

h, *rest, z = func1, func2, func2, func2
rest[0]()

d = {"k": func1}
d["k"]()

ls = [func1, func2]
ls[1]()
