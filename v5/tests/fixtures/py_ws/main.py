"""Entry point wiring the aliased import, method call, and ambiguous name."""

# Aliased import -- exercises the module_binding alias hop against compiler
# ground truth (load_user is the local name, fetch_user the real export).
from pkg.api import fetch_user as load_user, helper
from pkg.service import UserService


def greet(id):
    name = load_user(id)
    service = UserService()
    tagged = service.get_user(id)
    tag = helper()
    return "hello " + name + " " + tagged + " " + tag


if __name__ == "__main__":
    print(greet(1))
