# numpy's own testing vocabulary mints the contract on numpy.add — lifted by
# the numpy.testing seat, no synthetic unit test needed.
import numpy as np
from numpy.testing import assert_equal


def test_add_is_five():
    result = np.add(2, 3)
    assert_equal(result, 5)
    assert_equal(result, 5)
