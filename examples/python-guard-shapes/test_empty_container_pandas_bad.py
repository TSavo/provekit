# Guard shape #3 -- empty container (pandas), bad case. positional access on non-empty is witnessed; empty .iloc[0] raises IndexError
# The witness RUNS the real pandas code: this case is refused.
import pandas as pd


def test_empty_container_pandas_bad():
    s = pd.Series([], dtype=float)
    assert s.iloc[0] == 0
