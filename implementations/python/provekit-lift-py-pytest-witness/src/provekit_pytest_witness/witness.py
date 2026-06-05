# SPDX-License-Identifier: Apache-2.0
"""pytest witness lifter -- the proofchain-native correctness producer.

Instead of lifting a test's assertions into a symbolic consistency claim, this
RUNS the test: pytest is the deterministic transform ``k``, the code-under-test
is ``I``, the observed pass/fail is ``t``.  The run is content-addressed into a
witness memento with the substrate's own CID machinery (``jcs_hash``).

A witness is a k(I)=t proofchain link.  It pins the code CID + runtime CID +
outcome and is SELF-DESCRIBING about which code it covers (project-relative
paths), so a verifier holding only the project root can recompute.  VERIFICATION
IS RECOMPUTATION -- re-run on the pinned code, rebuild the witness, memcmp.
Nothing is trusted.

Teeth: the witness is cryptographically bound to the exact code.  A wrong impl
has a different code CID (can't borrow a correct program's passing witness) and,
when run, yields a ``failed`` outcome (can mint no passing witness of its own).
You cannot witness code right that runs wrong.
"""
from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
from dataclasses import dataclass
from typing import List, Tuple

from provekit_lift_py_tests.canonicalizer import (
    blake3_512_of,
    encode_jcs,
    jcs_hash,
    vobj,
    vstr,
)


def code_cid(project_dir: str, code_files: List[str]) -> str:
    """Content-address the code under test by PROJECT-RELATIVE path + content,
    so the same witness re-runs from any checkout of the same project."""
    parts = []
    for rel in sorted(code_files):
        with open(os.path.join(project_dir, rel), "rb") as f:
            parts.append(rel.encode("utf-8") + b"\0" + f.read())
    return blake3_512_of(b"\0".join(parts))


def runtime_cid() -> str:
    """Pin the runtime that makes the run reproducible.  The witness only holds
    where this CID reproduces -- trivial for pure code; for FP/SIMD kernels it is
    exactly where reproducibility must be earned and stated."""
    desc = (
        f"python={tuple(sys.version_info[:3])};"
        f"impl={platform.python_implementation()};"
        f"platform={platform.platform()}"
    )
    return blake3_512_of(desc.encode("utf-8"))


def _witness_value(code: str, runtime: str, test_id: str, outcome: str, code_files: List[str]):
    return vobj([
        ("kind", vstr("pytest-witness")),
        ("codeCid", vstr(code)),
        ("codeFiles", vstr(",".join(sorted(code_files)))),
        ("outcome", vstr(outcome)),
        ("runtimeCid", vstr(runtime)),
        ("test", vstr(test_id)),
    ])


@dataclass(frozen=True)
class Witness:
    code_cid: str
    runtime_cid: str
    test_id: str
    outcome: str          # "passed" | "failed"
    code_files: Tuple[str, ...]  # project-relative
    cid: str


def run_and_witness(project_dir: str, test_id: str, code_files: List[str]) -> Witness:
    """Run ``test_id`` under pytest in ``project_dir`` and content-address the run."""
    cc = code_cid(project_dir, code_files)
    rc = runtime_cid()
    proc = subprocess.run(
        [sys.executable, "-m", "pytest", test_id, "-q", "-p", "no:cacheprovider"],
        cwd=project_dir, capture_output=True, text=True,
    )
    outcome = "passed" if proc.returncode == 0 else "failed"
    cid = jcs_hash(_witness_value(cc, rc, test_id, outcome, code_files))
    return Witness(cc, rc, test_id, outcome, tuple(sorted(code_files)), cid)


# ---------------------------------------------------------------------------
# WitnessMemento: the signed pointer the `.proof` carries INSTEAD of the run body.
# ---------------------------------------------------------------------------

# The prover's witness-signing seed. A witness is OUR signed mark; in production
# this is the prover's provenance key (vault). Fixed here so the memento is
# reproducible in tests.
WITNESS_SIGNER_SEED = bytes([0x77]) * 32  # 'w' for witness


def witness_memento(w: "Witness", seed: bytes = WITNESS_SIGNER_SEED) -> dict:
    """Build a signed WitnessMemento. The `.proof` carries THIS -- a pointer +
    hash + signature -- not the run body. The body (the recorded run) goes to the
    witness package, resolved + re-verified by the Witness Oracle: signature
    always (whose mark), recompute when the runtime pin reproduces."""
    import nacl.signing

    sk = nacl.signing.SigningKey(seed)
    return {
        "kind": "witness-memento",
        "witness_cid": w.cid,
        "witness_kind": "pytest-witness",
        "signer": "ed25519:" + sk.verify_key.encode().hex(),
        "signature": sk.sign(w.cid.encode("utf-8")).signature.hex(),
        "runtime_cid": w.runtime_cid,
        "code_cid": w.code_cid,
        "test": w.test_id,
        "outcome": w.outcome,
        "code_files": list(w.code_files),
    }


def verify(witness: Witness, project_dir: str) -> Tuple[str, str]:
    """Verify a witness BY RECOMPUTATION against ``project_dir``.

    DISCHARGED iff the project's code (at the witness's own code paths) hashes to
    the witness's ``codeCid`` (binding), re-running reproduces the witness CID,
    and the witnessed outcome is ``passed``.  Anything else is REFUSED.
    """
    actual = code_cid(project_dir, list(witness.code_files))
    if actual != witness.code_cid:
        return ("REFUSED", "code CID mismatch -- this witness is not about this code")
    recomputed = run_and_witness(project_dir, witness.test_id, list(witness.code_files))
    if recomputed.cid != witness.cid:
        return ("REFUSED", f"witness did not reproduce (re-run outcome: {recomputed.outcome})")
    if witness.outcome != "passed":
        return ("REFUSED", f"witnessed outcome is {witness.outcome!r}, not a discharge")
    return ("DISCHARGED", "re-ran on pinned code; assertions held; witness CID reproduced")


# ---------------------------------------------------------------------------
# Pipeline-native: persist as a content-addressed .proof memento (the IR
# EvidenceTerm{proofType:"custom"} shape) and discharge BY RECOMPUTE.
# ---------------------------------------------------------------------------


def _witness_envelope_value(w: Witness):
    proof_data = json.dumps(
        {"codeCid": w.code_cid, "runtimeCid": w.runtime_cid, "test": w.test_id,
         "outcome": w.outcome, "codeFiles": list(w.code_files)},
        sort_keys=True, separators=(",", ":"),
    )
    cert = vobj([
        ("tool", vstr("pytest")),
        ("version", vstr(w.runtime_cid)),
        ("formulaHash", vstr(w.cid)),
        ("proofData", vstr(proof_data)),
    ])
    return vobj([
        ("kind", vstr("evidence")),
        ("proofType", vstr("custom")),
        ("certificate", cert),
    ])


def emit_witness_proof(w: Witness, out_dir: str) -> str:
    """Write the witness as a content-addressed ``.proof`` (filename = its CID)."""
    val = _witness_envelope_value(w)
    envelope_cid = jcs_hash(val)
    path = os.path.join(out_dir, envelope_cid.replace(":", "_") + ".proof")
    with open(path, "w", encoding="utf-8") as f:
        f.write(encode_jcs(val))
    return path


def load_witness_from_proof(proof_path: str) -> Witness:
    env = json.loads(open(proof_path, encoding="utf-8").read())
    pd = json.loads(env["certificate"]["proofData"])
    code_files = tuple(sorted(pd["codeFiles"]))
    cid = jcs_hash(_witness_value(pd["codeCid"], pd["runtimeCid"], pd["test"], pd["outcome"], list(code_files)))
    return Witness(pd["codeCid"], pd["runtimeCid"], pd["test"], pd["outcome"], code_files, cid)


def discharge_from_proof(proof_path: str, project_dir: str) -> Tuple[str, str]:
    """Pipeline discharge: load the witness memento and settle it BY RECOMPUTE.

    A forged ``passed`` envelope over failing code is caught here -- the re-run
    mints a ``failed`` witness whose CID will not match the forged one.
    """
    w = load_witness_from_proof(proof_path)
    return verify(w, project_dir)
