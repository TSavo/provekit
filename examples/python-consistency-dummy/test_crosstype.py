# Cross-type literal distinctness gate (Python `==` semantics). The consistency
# pass encodes str/None literals as opaque uninterpreted Int constants made
# distinct from each other and from concrete int/bool values; bool encodes to
# its int value (True==1, False==0). See
# provekit_ir_compiler_smt_lib::literal_encoding.

# r == "5" and r == 5 — CONTRADICTORY (str "5" != int 5). Verdict: REFUSED.
# If DISCHARGED, the Int-universe encoding doesn't distinguish types (falsePass).
def test_crosstype_contradictory(x):
    r = make_value(x)
    assert r == "5"
    assert r == 5


# r is None and r == 5 — CONTRADICTORY (None != int 5). Verdict: REFUSED.
def test_none_vs_int_contradictory(x):
    r = make_value(x)
    assert r is None
    assert r == 5


# r == True and r == 1 — CONSISTENT (Python True == 1). Verdict: PROVEN.
# OVER-DISTINCTNESS GUARD: a REFUSED here means bool was wrongly made distinct
# from int. This case must stay DISCHARGED.
def test_true_vs_one_consistent(x):
    r = make_value(x)
    assert r == True
    assert r == 1
