# Guard shape #5 -- key access (pandas), ok case. existing column is witnessed; missing column raises KeyError
# The witness RUNS the real pandas code: this case is witnessed (discharged).
import pandas as pd


def test_key_access_pandas_ok():
    df = pd.DataFrame({"a": [1]})
    assert df["a"].iloc[0] == 1
