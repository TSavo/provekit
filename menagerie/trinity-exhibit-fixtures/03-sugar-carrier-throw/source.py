# Trinity fixture 03: sugar-carrier transport -- concept:throw
# Exercises: concept:throw (Python+Java have morphisms; Rust does NOT)
# The chain must preserve the throw concept via comment carrier through the Rust hop.

def safe_divide(a: int, b: int) -> int:
    if b == 0:
        raise ValueError("division by zero")
    return a // b


if __name__ == "__main__":
    try:
        print(safe_divide(10, 2))   # expected: 5
        print(safe_divide(7, 0))    # expected: raises ValueError
    except ValueError as e:
        print(f"caught: {e}")       # expected: caught: division by zero
