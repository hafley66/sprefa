# Fixture: every EMITTED Python callable kind for examples/callable-coverage.dl.
# free/nested def -> function; method/__init__/staticmethod/property/dunder ->
# method; lambda expression -> lambda.


def free_function(seed):
    # nested def -> function
    def nested_helper(inner):
        return inner + 1

    # lambda expressions -> lambda
    bound = lambda factor: factor * 2
    mapped = sum(map(lambda value: value + seed, [1, 2, 3]))
    return nested_helper(bound(mapped))


async def async_free(payload):
    return payload


def generator_fn(limit):
    for index in range(limit):
        yield index


class Widget:
    # __init__ constructor -> method
    def __init__(self, size):
        self._size = size

    # instance method -> method
    def area(self):
        return self._size * self._size

    # static method -> method
    @staticmethod
    def unit():
        return Widget(1)

    # property getter / setter -> method (share one sym)
    @property
    def width(self):
        return self._size

    @width.setter
    def width(self, value):
        self._size = value

    # dunder / operator -> method
    def __add__(self, other):
        return Widget(self._size + other._size)
