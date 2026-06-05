import pytest
@pytest.mark.parametrize("v", [1, 2])
def test_param_row(v):
    assert v == 1
