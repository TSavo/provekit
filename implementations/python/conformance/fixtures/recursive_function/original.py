def factorial(n: int) -> int:
    return 1 if n == 0 else n * factorial(n - 1)
