# Trinity fixture 01: arithmetic add
# Exercises: concept:add, concept:mul, concept:sub
# All three Trinity languages (Python, Java, Rust) have first-class morphisms
# for these concepts; the chain should close with zero loss.

def compute_sum(a: int, b: int) -> int:
    total = a + b
    scaled = total * 2
    reduced = scaled - 1
    return reduced


if __name__ == "__main__":
    result = compute_sum(3, 4)
    print(result)  # expected: 13
