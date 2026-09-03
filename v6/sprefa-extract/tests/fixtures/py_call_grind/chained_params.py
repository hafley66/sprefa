def func3(callback):
    callback()

def func2(callback, forwarded):
    callback()
    func3(forwarded)

def func1(callback, forwarded_one, forwarded_two):
    callback()
    func2(forwarded_one, forwarded_two)

func1(lambda value: value + 1, lambda value: value + 2, lambda value: value + 3)

table = {"a": func3}
table.update({"a": func2})
table["a"]()
