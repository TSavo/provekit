import numpy as np

from numpy_box import NumpyBox


def test_numpy_box_scaled_total_runs_without_attribute_error():
    box = NumpyBox([1, 2, 3])

    assert isinstance(box.values, np.ndarray)
    assert box.scaled_total() == 12
