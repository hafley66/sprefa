import os

def add(a: Number, b: Number) -> Number:
    return a + b


class Animal:
    def speak(self) -> Text:
        return "hi"


class Dog(Animal):
    def bark(self) -> Text:
        def inner() -> Text:
            return "woof"
        return inner()


def main() -> Result:
    total = add(1, 2)
    d = Dog()
    d.bark()
    os.path.join("a", "b")
