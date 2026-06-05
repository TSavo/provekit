# SPDX-License-Identifier: Apache-2.0
import os
import io
import json
import contextlib

import pytest

from provekit_pytest_witness import (
    Witness, run_and_witness, verify, emit_witness_proof, discharge_from_proof,
    witness_memento, write_witness_package, read_witness_body,
)
from provekit_pytest_witness.discharge_cli import main as discharge_main
from provekit_lift_py_tests.witness_oracle import (
    WitnessOracleRefusal, resolve_witness,
)

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
    out = tmp_path / "out"
    out.mkdir()
    path = emit_witness_proof(w, str(out))
    assert os.path.basename(path).startswith("blake3-512_") and path.endswith(".proof")
    assert discharge_from_proof(path, proj)[0] == "DISCHARGED"


def test_forged_passed_witness_over_failing_code_is_caught_by_recompute(tmp_path):
    proj = _project(tmp_path, BAD)  # add(2,3)==6, test fails
    w_real = run_and_witness(proj, "test_add.py", CODE)
    assert w_real.outcome == "failed"
    forged = Witness(w_real.code_cid, w_real.runtime_cid, w_real.test_id,
                     "passed", w_real.code_files, w_real.cid)
    out = tmp_path / "out"
    out.mkdir()
    path = emit_witness_proof(forged, str(out))
    verdict, reason = discharge_from_proof(path, proj)
    assert verdict == "REFUSED" and "did not reproduce" in reason, reason


# --- The kit MINTS, the Witness Oracle VERIFIES -------------------------------
# The witness lifter is the minter (we ran the test, we sign the mark); the
# Witness Oracle is the verifier (signature always, recompute when re-runnable).
# That minter/verifier pair is WHY the oracle exists: no external author wrote
# the witness, so we sign our own and re-check it.


def test_minted_memento_carries_no_run_body(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    m = witness_memento(w)
    # A pointer + hash + signature -- the run body lives in the witness package.
    assert m["witness_cid"] == w.cid
    assert m["signer"].startswith("ed25519:") and m["signature"]
    assert "proof_data" not in m and "ast_template" not in m


def test_oracle_verifies_minted_witness_by_signature(tmp_path):
    proj = _project(tmp_path, GOOD)
    m = witness_memento(run_and_witness(proj, "test_add.py", CODE))
    # No package, not re-run here -> signature is the universal check.
    assert resolve_witness(m)["verified_by"] == "signature"


def test_oracle_recomputes_minted_witness_via_rerun(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    m = witness_memento(w)
    # The pytest-witness is re-runnable + deterministic: the oracle's recompute_fn
    # re-runs the test and re-derives the CID. Reproduces -> recompute-verified.
    def recompute(_m):
        return run_and_witness(proj, w.test_id, list(w.code_files)).cid
    assert resolve_witness(m, recompute_fn=recompute)["verified_by"] == "recompute"


def test_oracle_refuses_minted_witness_when_code_drifts(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    m = witness_memento(w)
    (tmp_path / "impl.py").write_text(BAD)  # the bytes you'd run drifted
    def recompute(_m):
        return run_and_witness(proj, w.test_id, list(w.code_files)).cid
    with pytest.raises(WitnessOracleRefusal, match="recompute misaligned"):
        resolve_witness(m, recompute_fn=recompute)


def test_oracle_refuses_minted_witness_with_tampered_signature(tmp_path):
    proj = _project(tmp_path, GOOD)
    m = witness_memento(run_and_witness(proj, "test_add.py", CODE))
    m["signature"] = "00" * 64  # forge the mark
    with pytest.raises(WitnessOracleRefusal, match="signature invalid"):
        resolve_witness(m)


# --- Witness PACKAGE: CID-named bodies, deployed separately --------------------


def test_package_writes_cid_named_witness_files(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    pkg = tmp_path / "wpkg"
    paths = write_witness_package([w], str(pkg))
    # the filename IS the CID (":" -> "_"), extension ".witness"
    assert len(paths) == 1
    assert os.path.basename(paths[0]) == w.cid.replace(":", "_") + ".witness"


def test_package_body_content_addresses_to_the_pinned_cid(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    pkg = tmp_path / "wpkg"
    write_witness_package([w], str(pkg))
    body = read_witness_body(w.cid, str(pkg))
    # the Witness Oracle's content-address path: bytes blake3 == pinned CID
    m = witness_memento(w)
    assert resolve_witness(m, witness_content=body)["verified_by"] == "content-address"


def test_package_tamper_is_refused_by_content_address(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    pkg = tmp_path / "wpkg"
    paths = write_witness_package([w], str(pkg))
    with open(paths[0], "ab") as f:
        f.write(b" tampered")  # swap the body under the same name
    body = read_witness_body(w.cid, str(pkg))
    with pytest.raises(WitnessOracleRefusal, match="content misaligned"):
        resolve_witness(witness_memento(w), witness_content=body)


# --- The oracle speaks RPC: resolve returns the BODY, not a verdict -----------
# Verification lives in the rust CLI; the kit oracle is untrusted and only hands
# over the body bytes over RPC. The verifier blake3's them and audits.


def _rpc(method, params):
    import subprocess, sys
    from provekit_lift_py_tests.canonicalizer import blake3_512_of  # noqa: F401
    req = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    proc = subprocess.run(
        [sys.executable, "-m", "provekit_pytest_witness.lift_lsp", "--rpc"],
        input=req + "\n", capture_output=True, text=True, timeout=60,
    )
    # the kit may print pytest noise; the RPC reply is the last JSON line
    for line in reversed(proc.stdout.strip().splitlines()):
        try:
            return json.loads(line)
        except json.JSONDecodeError:
            continue
    raise AssertionError(f"no JSON-RPC reply; stderr={proc.stderr}")


def test_resolve_witness_rpc_returns_body_that_addresses_to_cid_from_package(tmp_path):
    import base64
    from provekit_pytest_witness import write_witness_package
    from provekit_lift_py_tests.canonicalizer import blake3_512_of
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    pkg = tmp_path / "wpkg"; write_witness_package([w], str(pkg))
    reply = _rpc("provekit.plugin.resolve_witness", {
        "memento": witness_memento(w), "package_dir": str(pkg),
    })
    assert "result" in reply, reply
    body = base64.b64decode(reply["result"]["body_b64"])
    assert reply["result"]["resolved_by"] == "package"
    # the check the rust verifier does: recompute the CID over the resolved body
    assert blake3_512_of(body) == w.cid


def test_resolve_witness_rpc_recompute_reruns_and_returns_body(tmp_path):
    import base64
    from provekit_lift_py_tests.canonicalizer import blake3_512_of
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    # no package -> the oracle re-runs the pinned test and rebuilds the body
    reply = _rpc("provekit.plugin.resolve_witness", {
        "memento": witness_memento(w), "workspace_root": proj,
    })
    assert "result" in reply, reply
    assert reply["result"]["resolved_by"] == "recompute"
    body = base64.b64decode(reply["result"]["body_b64"])
    assert blake3_512_of(body) == w.cid


def test_resolve_witness_rpc_recompute_with_empty_code_files(tmp_path):
    # REGRESSION: an all-tests project pins an EMPTY code_files (the code under
    # test is the installed library, e.g. numpy/pandas, not a local file). An
    # empty list is FALSY, so a truthiness guard on `code_files` wrongly declared
    # such a witness "not re-runnable". It is trivially re-runnable -- just rerun
    # the test -- and the empty list reconstructs into the pinned witness body.
    import base64
    from provekit_lift_py_tests.canonicalizer import blake3_512_of
    (tmp_path / "test_solo.py").write_text("def test_solo():\n    assert 1 == 1\n")
    w = run_and_witness(str(tmp_path), "test_solo.py", [])  # empty code_files
    assert w.code_files == ()
    reply = _rpc("provekit.plugin.resolve_witness", {
        "memento": witness_memento(w), "workspace_root": str(tmp_path),
    })
    assert "result" in reply, reply  # NOT an error ("not re-runnable")
    assert reply["result"]["resolved_by"] == "recompute"
    assert blake3_512_of(base64.b64decode(reply["result"]["body_b64"])) == w.cid


def test_resolve_witness_rpc_refuses_recompute_on_tampered_memento(tmp_path):
    # The body is a pure function of the memento's own fields, so a memento whose
    # fields don't reconstruct its pinned CID is tampered. The oracle must refuse
    # BEFORE executing the (attacker-controlled) test path, not after.
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    m = witness_memento(w)
    m["outcome"] = "failed"  # cid still pins the passing run -> no longer reconstructs
    reply = _rpc("provekit.plugin.resolve_witness", {"memento": m, "workspace_root": proj})
    assert "error" in reply and "do not reconstruct" in reply["error"]["message"], reply


def test_resolve_witness_rpc_errors_when_unresolvable(tmp_path):
    proj = _project(tmp_path, GOOD)
    w = run_and_witness(proj, "test_add.py", CODE)
    m = witness_memento(w)
    m.pop("test"); m.pop("code_files")  # not re-runnable, no package
    reply = _rpc("provekit.plugin.resolve_witness", {"memento": m})
    assert "error" in reply and "cannot resolve" in reply["error"]["message"]


# --- The verifier<->kit contract: the discharge command -----------------------


def _run_discharge(proof_path, proj):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = discharge_main([proof_path, proj])
    return rc, json.loads(buf.getvalue())


def test_discharge_command_good(tmp_path):
    proj = _project(tmp_path, GOOD)
    out = tmp_path / "out"
    out.mkdir()
    path = emit_witness_proof(run_and_witness(proj, "test_add.py", CODE), str(out))
    rc, j = _run_discharge(path, proj)
    assert rc == 0 and j["verdict"] == "DISCHARGED", j


def test_discharge_command_refuses_mutated(tmp_path):
    proj = _project(tmp_path, GOOD)
    out = tmp_path / "out"
    out.mkdir()
    path = emit_witness_proof(run_and_witness(proj, "test_add.py", CODE), str(out))
    (tmp_path / "impl.py").write_text(BAD)
    rc, j = _run_discharge(path, proj)
    assert rc == 1 and j["verdict"] == "REFUSED", j
