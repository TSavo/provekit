# Guard shape #2 -- index bounds (numpy), ok case. in-bounds is witnessed; a[5] raises IndexError
# The witness RUNS the real numpy code: this case is witnessed (discharged).
import numpy as np


def test_index_bounds_numpy_ok():
    a = np.array([10, 20, 30])
    assert a[1] == 20
