# Guard shape #3 -- empty container (pandas), ok case. positional access on non-empty is witnessed; empty .iloc[0] raises IndexError
# The witness RUNS the real pandas code: this case is witnessed (discharged).
import pandas as pd


def test_empty_container_pandas_ok():
    s = pd.Series([1, 2, 3])
    assert s.iloc[0] == 1
