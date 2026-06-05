# Guard shape #5 -- key access (numpy), ok case. existing structured field is witnessed; absent field raises ValueError
# The witness RUNS the real numpy code: this case is witnessed (discharged).
import numpy as np


def test_key_access_numpy_ok():
    a = np.array([(1,)], dtype=[("x", "i4")])
    assert a["x"][0] == 1
