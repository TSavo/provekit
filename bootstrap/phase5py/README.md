# Phase-5-Py-v0 n=1 self-trip receipt

Phase-5-Py n=1 case is characterized, not verified.

This directory records the Python n=1 case for the libprovekit self-host arc.
The case starts from the four D7 Value constructor fixtures and asks whether the
existing Python realization and Python source lift kits can close the same
substrate-CID loop that D7 closed for Rust source.

The self-trip under test is:

1. read the D7 Rust lift fixture
2. invoke provekit-realize-python-core
3. parse the emitted Python with ast.parse
4. invoke provekit-lift-python-source on that source
5. compare the lifted Python body CID with the fixture ProofIR term CID

Fixtures used:

- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v0_value_null.json
- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_boolean.json
- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_integer.json
- implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_string.json

Generated artifacts:

- bootstrap/phase5py/driver_v0.py
- bootstrap/phase5py/libprovekit_py_v0.py
- bootstrap/phase5py/v0_receipt.json
- bootstrap/phase5py/README.md

Per-fixture verdicts:

| Fixture | Verdict | Diff class | Substrate CID match |
| --- | --- | --- | --- |
| null | CHARACTERIZED_DIFF | realize-python-template-gap | false |
| boolean | CHARACTERIZED_DIFF | realize-python-template-gap | false |
| integer | CHARACTERIZED_DIFF | realize-python-template-gap | false |
| string | CHARACTERIZED_DIFF | realize-python-template-gap | false |

The current result is a realize-python template gap.
provekit-realize-python-core accepts the invocations, but the canonical Python
body template catalog has no entries for the D7 Value constructor surfaces.
The kit therefore emits its documented fallback stubs.
Those stubs are valid Python and lift back into python:* body terms.
They do not lift back to the D7 ProofIR return/call:new terms.

No Rust code, substrate code, or memento type was changed for this receipt.
The driver records the behavior and stops at characterization.

The v1 chunk should retire the recorded realize-python template gap or choose
an explicit Python Value representation that lift_python can map back to the
same D7 ProofIR term CIDs.
v1 should keep array and object constructors out of scope unless they receive
their own fixtures and acceptance criteria.
