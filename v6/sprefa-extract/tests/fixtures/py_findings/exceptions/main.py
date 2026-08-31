class A(Exception):
    def __init__(self):
        pass

class B:
    class C(Exception):
        def __init__(self):
            pass

raise A

a = A
raise a

raise B.C
