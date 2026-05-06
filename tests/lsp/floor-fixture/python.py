# Forward-propagation floor fixture for Python
# Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback


def checkPositive(x: int) -> bool:
    if x <= 0:
        return False  # pre: x > 0
    return True


def callerSatisfiesPre() -> bool:
    result = checkPositive(5)  # satisfies pre (x=5 > 0)
    return result


def callerViolatesPre() -> bool:
    result = checkPositive(-1)  # violates pre (x=-1 <= 0)
    return result


def callerWithLoop() -> bool:
    for i in range(10):
        result = checkPositive(i)  # top fallback at loop entry
        if not result:
            return False
    return True
