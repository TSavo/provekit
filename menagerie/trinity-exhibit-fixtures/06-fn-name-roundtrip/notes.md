# Fixture 06: notes

R14.5 (PR #1154) established that function names are sugar at the algebra layer: they ride
through bind stdout as `fn_name_sugar` and are recovered by lower, but are stripped from the
canonical CID bytes so CIDs stay name-independent.

This fixture is the concrete test case for that ruling. Three functions with distinct names
cover the full mechanism: lift populates `fn_name_sugar`, bind carries it in the wire payload,
lower injects it into the realize-request, and the final Python output restores the names.

The fixture intentionally uses snake_case names (`sum_squares`, `is_even`) which will undergo
camelCase conversion in Java intermediate output. The name restoration across the camelCase hop
is an explicit check on the fn_name_sugar threading.

The fixture also uses recursion (`factorial`) to exercise the self-call path where the function
name appears both in the definition AND in a call expression.
