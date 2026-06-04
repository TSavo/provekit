# SPDX-License-Identifier: Apache-2.0
import os
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


# --- Pipeline-native: witness .proof memento + discharge-by-recompute ---------

import json
from provekit_pytest_witness import emit_witness_proof, discharge_from_proof, run_and_witness as _raw


def test_emit_proof_is_content_addressed_and_discharges(tmp_path):
    proj, code = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", code)
    out = tmp_path / "out"; out.mkdir()
    path = emit_witness_proof(w, str(out))
    # content-addressed: filename is the envelope CID
    assert os.path.basename(path).startswith("blake3-512_")
    assert os.path.basename(path).endswith(".proof")
    verdict, reason = discharge_from_proof(path, proj, code)
    assert verdict == "DISCHARGED", reason


def test_forged_passed_witness_over_failing_code_is_caught_by_recompute(tmp_path):
    # ANTI-FORGERY: hand-write a .proof claiming `passed` for code that FAILS.
    # Discharge re-runs; the re-run mints a `failed` witness whose CID != the
    # forged one -> REFUSED. You cannot forge a passing witness for failing code.
    proj, code = _project(tmp_path, BAD)  # add(2,3)==6, test fails
    w_real = run_and_witness(proj, "test_add.py", code)
    assert w_real.outcome == "failed"
    # forge: same pins, but outcome flipped to passed
    forged = Witness(w_real.code_cid, w_real.runtime_cid, w_real.test_id, "passed", w_real.cid)
    out = tmp_path / "out"; out.mkdir()
    path = emit_witness_proof(forged, str(out))
    verdict, reason = discharge_from_proof(path, proj, code)
    assert verdict == "REFUSED", reason
    assert "did not reproduce" in reason


from provekit_pytest_witness import Witness  # noqa: E402  (used above)


# --- The verifier<->kit contract: the discharge command ----------------------

from provekit_pytest_witness.discharge_cli import main as discharge_main
import json as _json, io, contextlib

def _run_discharge(proof_path, proj, code):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = discharge_main([proof_path, proj, *code])
    return rc, _json.loads(buf.getvalue())

def test_discharge_command_good(tmp_path):
    proj, code = _project(tmp_path, GOOD)
    from provekit_pytest_witness import run_and_witness as rw, emit_witness_proof as ep
    out = tmp_path/"out"; out.mkdir()
    path = ep(rw(proj,"test_add.py",code), str(out))
    rc, j = _run_discharge(path, proj, code)
    assert rc == 0 and j["verdict"] == "DISCHARGED", j

def test_discharge_command_refuses_mutated(tmp_path):
    proj, code = _project(tmp_path, GOOD)
    from provekit_pytest_witness import run_and_witness as rw, emit_witness_proof as ep
    out = tmp_path/"out"; out.mkdir()
    path = ep(rw(proj,"test_add.py",code), str(out))
    (tmp_path/"impl.py").write_text(BAD)  # mutate after witnessing
    rc, j = _run_discharge(path, proj, code)
    assert rc == 1 and j["verdict"] == "REFUSED", j
