# sklearn showcase

`sklearn-showcase` adds the missing scikit-learn logo to the product wall with
the same proof shape as `pandas-showcase`: real library code, a venv-backed
witness rerun, scalar consistency claims discharged by z3, and one deliberately
contradictory twin refused by both axes.

```sh
./run.sh
```

## Vendor rows

The exact claims come from scikit-learn 1.9.0 vendor tests:

- `metrics/tests/test_classification.py::test_multilabel_accuracy_score_subset_accuracy`
  for `accuracy_score(y1, y1) == 1`, `accuracy_score(y2, y2) == 1`, and
  `accuracy_score(y1, np.logical_not(y1)) == 0`.
- `metrics/tests/test_classification.py::test_multilabel_zero_one_loss_subset`
  for `zero_one_loss(y1, y1) == 0` and
  `zero_one_loss(y1, np.logical_not(y1)) == 1`.
- `cluster/tests/test_mean_shift.py::test_mean_shift_zero_bandwidth` for
  `bandwidth == 0`.

The current Python scalar lifter lifts these point-wise claims once the call
result is bound to a local name. Direct multi-argument call assertions are left
as a named residual rather than forced into the lifter.

## Axes

| axis | mechanism | good code | bad twin |
|---|---|---|---|
| consistency | z3 over lifted scalar assertions | metric and utility rows are mutually consistent | `score_y1 == 1` and `score_y1 == 0` is UNSAT |
| witness | pytest under real scikit-learn | good tests reproduce | the contradiction test fails |

`test_sklearn_testing_vocab.py` also exercises the learned
`sklearn.utils._testing` vocabulary. `assert_array_equal` derives as exact
equality; allclose and tolerance helpers are residuals.

## Residuals

This showcase intentionally skips tolerance assertions (`assert_allclose`,
`pytest.approx`, `assert_array_almost_equal`), fitted-model assertions,
randomness-dependent tests, direct multi-argument call assertions, tuple
assignment paths, and identity/effect claims such as `is` checks. Those are not
contracts here.

## Environment

The witness lifter runs pytest over real scikit-learn, so `run.sh` provisions
`/tmp/sklearn-witness-venv` with `scikit-learn==1.9.0`, `pytest`, `pynacl`,
`blake3`, and `cbor2`. The lift manifests use that interpreter so CI does not
depend on system Python packages.
