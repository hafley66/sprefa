class MyClass:
    def __init__(self):
        self.stored = self.func3

    def func1(self):
        pass

    def func2(self):
        return self.func1

    def func3(self):
        pass

    def run(self):
        self.stored()

instance = MyClass()
handle = instance.func3
handle()

first, (second, third) = instance.func1, (instance.func2, instance.func3)
first()

returned = instance.func2()
returned()
instance.func2()()
