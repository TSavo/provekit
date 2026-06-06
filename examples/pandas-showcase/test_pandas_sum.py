# pandas's own pytest vocabulary mints the contract on a real pandas operation
# (Series.sum) -- no synthetic test, this is how pandas is tested. The scalar
# assertion is where z3's teeth are: the plain pytest consistency seat lifts it
# to =(total, 6); the witness seat RE-RUNS it under real pandas.
import pandas as pd


def test_column_sum_is_six():
    df = pd.DataFrame({"a": [1, 2, 3]})
    total = df["a"].sum()
    assert total == 6
