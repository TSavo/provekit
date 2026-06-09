import numpy as np

from sklearn.cluster import estimate_bandwidth


def test_mean_shift_zero_bandwidth_exact_row():
    # Vendor source: sklearn 1.9.0
    # cluster/tests/test_mean_shift.py::test_mean_shift_zero_bandwidth exact row:
    #   assert bandwidth == 0
    #
    # The sibling identity row, `get_bin_seeds(...) is X`, is intentionally out
    # of scope for this showcase because identity/effects are not lifted here.
    X = np.array([1, 1, 1, 2, 2, 2, 3, 3], dtype=np.float64).reshape(-1, 1)

    bandwidth = estimate_bandwidth(X)

    assert bandwidth == 0
