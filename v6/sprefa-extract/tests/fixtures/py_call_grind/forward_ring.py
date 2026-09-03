def f0(callback):
    callback(f1)

def f1(callback):
    callback(f2)

def f2(callback):
    callback(f3)

def f3(callback):
    callback(f4)

def f4(callback):
    callback(f5)

def f5(callback):
    callback(f6)

def f6(callback):
    callback(f7)

def f7(callback):
    callback(f8)

def f8(callback):
    callback(f9)

def f9(callback):
    callback(f0)

f0(f1)
