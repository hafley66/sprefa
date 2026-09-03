class Base:
    def __init__(self):
        pass

    def func(self):
        pass

class Left(Base):
    pass

class Right(Base):
    def func(self):
        pass

class Leaf(Left, Right):
    pass

leaf = Leaf()
leaf.func()
