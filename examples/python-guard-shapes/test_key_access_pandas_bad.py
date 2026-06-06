# Guard shape #5 -- key access (pandas), bad case. existing column is witnessed; missing column raises KeyError
# The witness RUNS the real pandas code: this case is refused.
import pandas as pd


def test_key_access_pandas_bad():
    df = pd.DataFrame({"a": [1]})
    assert df["missing"].iloc[0] == 1
