import pytest
@pytest.fixture
def val():
    return 5
def test_uses_fixture_contradictory(val):
    assert val == 1
    assert val == 2
def test_uses_fixture_consistent(val):
    assert val == 1
    assert val != 2
