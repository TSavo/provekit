# SPDX-License-Identifier: Apache-2.0
import os
import io
import json
import contextlib

from provekit_pytest_witness import (
    Witness, run_and_witness, verify, emit_witness_proof, discharge_from_proof,
)
from provekit_pytest_witness.discharge_cli import main as discharge_main

GOOD = "def add(a, b):\n    return a + b\n"
BAD = "def add(a, b):\n    return a + b + 1\n"
TESTSRC = "from impl import add\n\ndef test_add():\n    assert add(2, 3) == 5\n"
CODE = ["impl.py"]  # project-relative


def _project(tmp_path, impl_src):
    (tmp_path / "impl.py").write_text(impl_src)
    (tmp_path / "test_add.py").write_text(TESTSRC)
    return str(tmp_path)


# --- The witnessed k(I)=t link ------------------------------------------------


def test_good_code_witness_discharges_and_reproduces(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    assert w.outcome == "passed", w
    assert run_and_witness(proj, "test_add.py", CODE).cid == w.cid, "must reproduce"
    assert verify(w, proj)[0] == "DISCHARGED"


def test_bad_code_cannot_borrow_good_witness(tmp_path):
    proj = _project(tmp_path, GOOD)
    w_good = run_and_witness(proj, "test_add.py", CODE)
    (tmp_path / "impl.py").write_text(BAD)  # mutate
    verdict, reason = verify(w_good, proj)
    assert verdict == "REFUSED" and "code CID mismatch" in reason, reason


def test_bad_code_produces_only_a_failed_witness(tmp_path):
    proj = _project(tmp_path, BAD)
    w = run_and_witness(proj, "test_add.py", CODE)
    assert w.outcome == "failed", w
    assert verify(w, proj)[0] == "REFUSED"


# --- Pipeline: .proof memento + discharge-by-recompute ------------------------


def test_emit_proof_is_content_addressed_and_discharges(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    out = tmp_path / "out"; out.mkdir()
    path = emit_witness_proof(w, str(out))
    assert os.path.basename(path).startswith("blake3-512_") and path.endswith(".proof")
    assert discharge_from_proof(path, proj)[0] == "DISCHARGED"


def test_forged_passed_witness_over_failing_code_is_caught_by_recompute(tmp_path):
    proj = _project(tmp_path, BAD)  # add(2,3)==6, test fails
    w_real = run_and_witness(proj, "test_add.py", CODE)
    assert w_real.outcome == "failed"
    forged = Witness(w_real.code_cid, w_real.runtime_cid, w_real.test_id,
                     "passed", w_real.code_files, w_real.cid)
    out = tmp_path / "out"; out.mkdir()
    path = emit_witness_proof(forged, str(out))
    verdict, reason = discharge_from_proof(path, proj)
    assert verdict == "REFUSED" and "did not reproduce" in reason, reason


# --- The verifier<->kit contract: the discharge command -----------------------


def _run_discharge(proof_path, proj):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = discharge_main([proof_path, proj])
    return rc, json.loads(buf.getvalue())


def test_discharge_command_good(tmp_path):
    proj = _project(tmp_path, GOOD)
    out = tmp_path / "out"; out.mkdir()
    path = emit_witness_proof(run_and_witness(proj, "test_add.py", CODE), str(out))
    rc, j = _run_discharge(path, proj)
    assert rc == 0 and j["verdict"] == "DISCHARGED", j


def test_discharge_command_refuses_mutated(tmp_path):
    proj = _project(tmp_path, GOOD)
    out = tmp_path / "out"; out.mkdir()
    path = emit_witness_proof(run_and_witness(proj, "test_add.py", CODE), str(out))
    (tmp_path / "impl.py").write_text(BAD)
    rc, j = _run_discharge(path, proj)
    assert rc == 1 and j["verdict"] == "REFUSED", j
