# Trinity fixture 06: function-name roundtrip via fn_name_sugar (R14.5)
# Exercises: multiple named functions with semantically meaningful names
# Per R14.5: fn_name_sugar rides through bind stdout, recovered by lower.
# All function names must appear in the final Python output unchanged.

def factorial(n: int) -> int:
    if n <= 1:
        return 1
    return n * factorial(n - 1)


def sum_squares(limit: int) -> int:
    total = 0
    i = 1
    while i <= limit:
        total = total + (i * i)
        i = i + 1
    return total


def is_even(value: int) -> bool:
    return value % 2 == 0


if __name__ == "__main__":
    print(factorial(5))      # expected: 120
    print(sum_squares(4))    # expected: 30 (1+4+9+16)
    print(is_even(6))        # expected: True
    print(is_even(7))        # expected: False
