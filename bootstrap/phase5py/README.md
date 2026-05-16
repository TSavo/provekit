# Phase-5-Py-v1 n=1 self-trip receipt

Phase-5-Py-v1 retires the v0 body-template gap and records the next blocker.

This directory records the Python n=1 case for the libprovekit self-host arc.
v1 adds a libprovekit-specific Python body-template catalog for the four D7
Value constructor surfaces and runs the same realize, parse, lift, compare
loop captured by v0.

The self-trip under test is:

1. read the D7 Rust lift fixture
2. invoke provekit-realize-python-core with python-canonical-bodies-libprovekit.json
3. parse the emitted Python with ast.parse
4. invoke provekit-lift-python-source on that source
5. compare the lifted Python body CID with the fixture ProofIR term CID

Fixtures used:

- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v0_value_null.json
- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_boolean.json
- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_integer.json
- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_string.json

Generated artifacts:

- bootstrap/phase5py/driver_v1.py
- bootstrap/phase5py/libprovekit_py_v1.py
- bootstrap/phase5py/v1_receipt.json
- menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-libprovekit.json
- bootstrap/phase5py/README.md

Per-fixture verdicts:

| Fixture | Verdict | Diff class | Substrate CID match |
| --- | --- | --- | --- |
| null | CHARACTERIZED_DIFF | lift-python-substrate-namespace-mismatch | false |
| boolean | CHARACTERIZED_DIFF | lift-python-substrate-namespace-mismatch | false |
| integer | CHARACTERIZED_DIFF | lift-python-substrate-namespace-mismatch | false |
| string | CHARACTERIZED_DIFF | lift-python-substrate-namespace-mismatch | false |

The v0 fallback-stub behavior is retired for this catalog.
The realized v1 functions are valid Python and use Value.NULL, Value.boolean,
Value.integer, and Value.string bodies.

The current stop condition is in lift_python.
The source lifter emits python:* body terms for these Python idioms.
It does not currently map them back to the D7 ProofIR return/call:new terms.
No Rust code, substrate code, or memento type was changed for this receipt.
