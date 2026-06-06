# Guard shape #3 -- empty container (numpy), bad case. reduction over non-empty is witnessed; empty .max() raises ValueError
# The witness RUNS the real numpy code: this case is refused.
import numpy as np


def test_empty_container_numpy_bad():
    a = np.array([])
    assert a.max() == 0
