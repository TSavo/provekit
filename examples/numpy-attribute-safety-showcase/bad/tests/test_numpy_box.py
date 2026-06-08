import numpy as np

from numpy_box import NumpyBox


def test_numpy_box_scaled_total_refuses_when_attribute_is_not_guaranteed():
    box = NumpyBox([1, 2, 3])

    assert isinstance(box.values, np.ndarray)
    assert box.scaled_total() == 12
