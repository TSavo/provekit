# Guard shape #2 -- index bounds (pandas), ok case. iloc in range is witnessed; iloc[5] raises IndexError
# The witness RUNS the real pandas code: this case is witnessed (discharged).
import pandas as pd


def test_index_bounds_pandas_ok():
    s = pd.Series([10, 20, 30])
    assert s.iloc[1] == 20
