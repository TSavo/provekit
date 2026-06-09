import numpy as np

from sklearn.metrics import accuracy_score


def test_multilabel_accuracy_score_contradiction():
    # Contradiction twin for the sklearn vendor row:
    # metrics/tests/test_classification.py::test_multilabel_accuracy_score_subset_accuracy
    # exact row `assert accuracy_score(y1, y1) == 1`.
    y1 = np.array([[0, 1, 1], [1, 0, 1]])

    score_y1 = accuracy_score(y1, y1)

    assert score_y1 == 1
    assert score_y1 == 0
