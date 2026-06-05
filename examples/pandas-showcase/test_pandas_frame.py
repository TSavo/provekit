# Exercises the pandas.testing seat specifically. assert_frame_equal with
# check_exact PINNED is lifted as exact equality (keying the pandas op for the
# witness axis); the witness seat RE-RUNS it under real pandas.
#
# An UN-pinned assert_frame_equal would be LOUDLY REFUSED by the pandas.testing
# seat: pandas compares floats with a tolerance by default, so lifting it as `=`
# would claim an exactness pandas never checked. See the commented line below.
import pandas as pd
from pandas.testing import assert_frame_equal


def test_frame_round_trips_exactly():
    df = pd.DataFrame({"a": [1, 2, 3]})
    expected = pd.DataFrame({"a": [1, 2, 3]})
    assert_frame_equal(df, expected, check_exact=True)
    # assert_frame_equal(df, expected)            # <- approximate -> REFUSED
