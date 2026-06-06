# Guard shape #4 -- divide by zero (pandas), bad case. pandas inherits numpy: zero divisor yields inf, finiteness assertion fails
# The witness RUNS the real pandas code: this case is refused.
import numpy as np
import pandas as pd


def test_divide_by_zero_pandas_bad():
    r = pd.Series([1.0]) / pd.Series([0.0])
    assert bool(np.isfinite(r.iloc[0]))
