# FIX 1 REGRESSION GUARD — PROVEN (SYMBOLIC PARAM ARG: NO CROSS-LOCATION UNIFY).
#
# Before Fix 1, two SEPARATE test functions calling the SAME callee with a
# SYMBOLIC (parameter) argument of the same NAME (``x``) were spuriously
# UNIFIED cross-location.  The SSA name ``x`` is the same bare string in both
# functions, so the arg_sig matched, the EUF base was shared, and
# mint's coalesce-by-name conjoined:
#
#   and( =(callresult_f_a1(x), 1), =(callresult_f_a1(x), 2) )
#
# -> UNSAT -> REFUSED.  But the two ``x`` params are INDEPENDENTLY BOUND in
# each function: at runtime they can have DIFFERENT values.  The unification is
# UNSOUND — it produced 10 spurious FALSE-REFUSALS in the real corpus sweep.
#
# After Fix 1 (VALUE-AWARE EUF): symbolic arguments (params / locals / opaque
# vars) fall back to LOCATION-KEYED free vars (no cross-location unification).
# The two functions emit INDEPENDENT ::assertion invs:
#
#   make_value_sym@<file>:N:C::assertion  =(loc_var_1, 1)  -> SAT
#   make_value_sym@<file>:M:D::assertion  =(loc_var_2, 2)  -> SAT
#
# Each is independently satisfiable -> PROVEN (not false-refused).
#
# CORRECT verdict: PROVEN-consistent (both functions, independent).


def test_euf_sym_a(x):
    assert make_value_sym(x) == 1


def test_euf_sym_b(x):
    assert make_value_sym(x) == 2
