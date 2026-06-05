# Guard shape #3 -- empty container (numpy), ok case. reduction over non-empty is witnessed; empty .max() raises ValueError
# The witness RUNS the real numpy code: this case is witnessed (discharged).
import numpy as np


def test_empty_container_numpy_ok():
    a = np.array([1, 2, 3])
    assert a.max() == 3
