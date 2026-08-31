def param_func():
    pass

def other():
    pass

def func(a):
    a()

func(param_func)

def kw(a, b):
    a()
    b()

kw(other, b=param_func)
