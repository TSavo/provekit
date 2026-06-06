# The DEGENERATE case: two contradictory scalar assertions about the SAME
# pandas result. Refused BOTH ways, the whole correctness claim in one file:
#   consistency : =(total, 6) ^ =(total, 7) -> z3 UNSAT -> refused.
#   witness     : real pandas runs Series.sum -> 6, so `assert total == 7`
#                 FAILS -> witness outcome `failed` -> refused by recompute.
import pandas as pd


def test_column_sum_contradiction():
    df = pd.DataFrame({"a": [1, 2, 3]})
    total = df["a"].sum()
    assert total == 6
    assert total == 7
