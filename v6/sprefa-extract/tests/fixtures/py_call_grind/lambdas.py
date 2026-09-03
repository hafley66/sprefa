def func(callback):
    callback()

def inline_func(callback):
    callback()

def make():
    return (lambda value: value + 1)

module_lambda = lambda value: value + 1

module_lambda(1)
func(module_lambda)
inline_func(lambda value: value + 2)

made = make()
made()
