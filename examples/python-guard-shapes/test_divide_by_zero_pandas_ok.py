# Guard shape #4 -- divide by zero (pandas), ok case. pandas inherits numpy: zero divisor yields inf, finiteness assertion fails
# The witness RUNS the real pandas code: this case is witnessed (discharged).
import numpy as np
import pandas as pd


def test_divide_by_zero_pandas_ok():
    r = pd.Series([1.0]) / pd.Series([2.0])
    assert bool(np.isfinite(r.iloc[0]))
