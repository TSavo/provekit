import numpy as np
from numpy import array as make_array
from numpy import int64, sum as numpy_sum


class NumpyBox:
    def __init__(self, values):
        self.values = make_array(values, dtype=int64)
        self.scale = 2

    def scaled_total(self):
        total = int(numpy_sum(self.values))
        return total * self.scale
