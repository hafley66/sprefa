# Covers the ast-grep CST fallback in the capability-parity roster leg: a
# language with an ast-grep grammar and no native Source, so the only family it
# produces is cst. Kept deliberately small; the point is routing and reach, not
# python semantics.


def greet(name):
    message = "hello, " + name
    return message


class Greeter:
    def __init__(self, prefix):
        self.prefix = prefix

    def call(self, name):
        return self.prefix + greet(name)
