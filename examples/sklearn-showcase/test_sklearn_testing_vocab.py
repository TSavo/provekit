import numpy as np
from sklearn.metrics import accuracy_score
from sklearn.utils._testing import assert_array_equal


def test_sklearn_testing_exact_scalar_row():
    # sklearn.utils._testing is sklearn's own testing vocabulary. The one
    # assertion lifter derives assert_array_equal as exact equality and refuses
    # the allclose/tolerance family as residuals.
    #
    # The metric contract is the same sklearn 1.9.0 vendor row used in
    # test_sklearn_metrics.py, expressed through sklearn's exact assertion
    # helper to cover the learned-vocabulary surface.
    y1 = np.array([[0, 1, 1], [1, 0, 1]])

    score_y1 = accuracy_score(y1, y1)

    assert_array_equal(score_y1, 1)
