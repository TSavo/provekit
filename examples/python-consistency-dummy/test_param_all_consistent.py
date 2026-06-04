import pytest
@pytest.mark.parametrize("v", [1, 2])
def test_param_ok(v):
    assert v > 0
