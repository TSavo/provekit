# Guard shape #5 -- key access (numpy), bad case. existing structured field is witnessed; absent field raises ValueError
# The witness RUNS the real numpy code: this case is refused.
import numpy as np


def test_key_access_numpy_bad():
    a = np.array([(1,)], dtype=[("x", "i4")])
    assert a["y"][0] == 1
