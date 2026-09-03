def walk(items):
    total = 0
    for item in items:
        if item < 0:
            continue
        elif item > 100:
            break
        else:
            total += item
    while total > 10:
        total -= 1
    try:
        check(total)
    except ValueError:
        raise
    match total:
        case 0:
            return -1
        case _:
            pass
    return total


def emit(items):
    for each in items:
        yield each
    return
