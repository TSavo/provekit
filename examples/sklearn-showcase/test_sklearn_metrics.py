import numpy as np

from sklearn.metrics import accuracy_score, zero_one_loss


def test_multilabel_accuracy_score_exact_rows():
    # Vendor source: sklearn 1.9.0
    # metrics/tests/test_classification.py::test_multilabel_accuracy_score_subset_accuracy
    # exact rows:
    #   assert accuracy_score(y1, y1) == 1
    #   assert accuracy_score(y2, y2) == 1
    #   assert accuracy_score(y1, np.logical_not(y1)) == 0
    #
    # The current scalar lifter handles the same contract when the call result is
    # named first. Direct multi-arg call assertions are a named residual.
    y1 = np.array([[0, 1, 1], [1, 0, 1]])
    y2 = np.array([[0, 0, 1], [1, 0, 1]])

    score_y1 = accuracy_score(y1, y1)
    score_y2 = accuracy_score(y2, y2)
    score_not_y1 = accuracy_score(y1, np.logical_not(y1))

    assert score_y1 == 1
    assert score_y2 == 1
    assert score_not_y1 == 0


def test_multilabel_zero_one_loss_exact_rows():
    # Vendor source: sklearn 1.9.0
    # metrics/tests/test_classification.py::test_multilabel_zero_one_loss_subset
    # exact rows:
    #   assert zero_one_loss(y1, y1) == 0
    #   assert zero_one_loss(y1, np.logical_not(y1)) == 1
    y1 = np.array([[0, 1, 1], [1, 0, 1]])

    loss_y1 = zero_one_loss(y1, y1)
    loss_not_y1 = zero_one_loss(y1, np.logical_not(y1))

    assert loss_y1 == 0
    assert loss_not_y1 == 1
