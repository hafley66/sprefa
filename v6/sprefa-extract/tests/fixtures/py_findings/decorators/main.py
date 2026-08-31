def dec(f):
    def wrapper():
        return f()
    return wrapper

def dec_id(f):
    return f

@dec
def func():
    pass

@dec_id
def func2():
    pass

func()
func2()
