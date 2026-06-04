# SPDX-License-Identifier: Apache-2.0
from provekit_pytest_witness import run_and_witness, verify

GOOD = "def add(a, b):\n    return a + b\n"
BAD  = "def add(a, b):\n    return a + b + 1\n"
TESTSRC = "from impl import add\n\ndef test_add():\n    assert add(2, 3) == 5\n"

def _project(tmp_path, impl_src):
    (tmp_path / "impl.py").write_text(impl_src)
    (tmp_path / "test_add.py").write_text(TESTSRC)
    return str(tmp_path), [str(tmp_path / "impl.py")]

def test_good_code_witness_discharges_and_reproduces(tmp_path):
    proj, code = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", code)
    assert w.outcome == "passed", w
    # RECOMPUTE: a second run yields the SAME witness CID (the proofchain property)
    w2 = run_and_witness(proj, "test_add.py", code)
    assert w2.cid == w.cid, "witness must be byte-reproducible"
    verdict, reason = verify(w, proj, code)
    assert verdict == "DISCHARGED", reason

def test_bad_code_cannot_borrow_good_witness(tmp_path):
    # TEETH: the good witness is bound to the good code's CID. Swap in wrong code
    # and the witness no longer applies — it cannot be reused to bless bad code.
    proj, code = _project(tmp_path, GOOD)
    w_good = run_and_witness(proj, "test_add.py", code)
    (tmp_path / "impl.py").write_text(BAD)
    verdict, reason = verify(w_good, proj, code)
    assert verdict == "REFUSED", reason
    assert "code CID mismatch" in reason

def test_bad_code_produces_only_a_failed_witness(tmp_path):
    # TEETH: wrong impl, when run, yields a `failed` outcome — there is NO
    # `passed` witness it can mint. Wrong code is un-dischargeable.
    proj, code = _project(tmp_path, BAD)
    w = run_and_witness(proj, "test_add.py", code)
    assert w.outcome == "failed", w
    verdict, _ = verify(w, proj, code)
    assert verdict == "REFUSED"
