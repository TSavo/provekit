# Trinity fixture 02: control flow -- while + conditional
# Exercises: concept:while, concept:conditional, concept:eq, concept:seq
# All four concepts have first-class morphisms in Python, Java, and Rust.

def count_down(n: int) -> int:
    total = 0
    i = n
    while i > 0:
        if i % 2 == 0:
            total = total + i
        i = i - 1
    return total


if __name__ == "__main__":
    result = count_down(6)
    print(result)  # expected: 12 (2 + 4 + 6)
