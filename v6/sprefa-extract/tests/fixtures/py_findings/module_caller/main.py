from helper import imported_fn


def local_fn():
    return 1


result = local_fn()
other = imported_fn()
