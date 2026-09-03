def func1():
    pass

def func2():
    pass

def func3():
    pass

def func4():
    pass

table = {
    "a": func1,
    1: func2,
    "1": func3,
}

table["a"]()
table[1]()

nested = {"outer": {"inner": func1}}
nested["outer"]["inner"] = func4
nested["outer"]["inner"]()

slots = [func1, [func2], func3]
slots[0]()
slots[1][0]()
slots[2]()

def by_param(container):
    container["a"]()

by_param(table)
