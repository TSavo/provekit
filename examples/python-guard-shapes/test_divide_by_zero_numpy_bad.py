# Guard shape #4 -- divide by zero (numpy), bad case. non-zero divisor stays finite (witnessed); zero divisor yields inf (refused)
# The witness RUNS the real numpy code: this case is refused.
import numpy as np


def test_divide_by_zero_numpy_bad():
    r = np.array([1.0]) / np.array([0.0])
    assert bool(np.isfinite(r[0]))
