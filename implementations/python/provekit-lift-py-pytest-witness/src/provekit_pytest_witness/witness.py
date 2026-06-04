# SPDX-License-Identifier: Apache-2.0
"""pytest witness lifter — the proofchain-native correctness producer.

Instead of lifting a test's assertions into a symbolic consistency claim, this
RUNS the test: pytest is the deterministic transform ``k``, the code-under-test
is ``I``, the observed pass/fail is ``t``.  The run is content-addressed into a
witness memento with the substrate's own CID machinery (``jcs_hash``).

A witness is a k(I)=t proofchain link.  It pins the code CID and the runtime CID
and records the observed outcome.  VERIFICATION IS RECOMPUTATION — re-run on the
pinned code, rebuild the witness, memcmp the CID.  Nothing is trusted.

The teeth: the witness is cryptographically bound to the exact code.  A wrong
implementation has a different code CID, and when run it yields a ``failed``
outcome — so it can neither borrow a correct program's ``passed`` witness (CID
mismatch) nor mint a ``passed`` one of its own.  You cannot witness code right
that runs wrong.
"""
from __future__ import annotations

import os
import platform
import subprocess
import sys
from dataclasses import dataclass
from typing import List, Tuple

from provekit_lift_py_tests.canonicalizer import blake3_512_of, jcs_hash, vobj, vstr


def code_cid(code_paths: List[str]) -> str:
    """Content-address the code under test (order-independent over files)."""
    parts = []
    for p in sorted(code_paths):
        with open(p, "rb") as f:
            parts.append(os.path.basename(p).encode("utf-8") + b"\0" + f.read())
    return blake3_512_of(b"\0".join(parts))


def runtime_cid() -> str:
    """Pin the runtime that makes the run reproducible.  The witness only holds
    where this CID reproduces — for pure code that is trivial; for FP/SIMD
    kernels it is exactly where reproducibility must be earned and stated."""
    desc = (
        f"python={tuple(sys.version_info[:3])};"
        f"impl={platform.python_implementation()};"
        f"platform={platform.platform()}"
    )
    return blake3_512_of(desc.encode("utf-8"))


def _witness_value(code: str, runtime: str, test_id: str, outcome: str):
    # Field order is irrelevant: encode_jcs sorts keys before hashing.
    return vobj([
        ("kind", vstr("pytest-witness")),
        ("codeCid", vstr(code)),
        ("outcome", vstr(outcome)),
        ("runtimeCid", vstr(runtime)),
        ("test", vstr(test_id)),
    ])


@dataclass(frozen=True)
class Witness:
    code_cid: str
    runtime_cid: str
    test_id: str
    outcome: str  # "passed" | "failed"
    cid: str


def run_and_witness(project_dir: str, test_id: str, code_paths: List[str]) -> Witness:
    """Run ``test_id`` under pytest in ``project_dir`` and content-address the run."""
    cc = code_cid(code_paths)
    rc = runtime_cid()
    proc = subprocess.run(
        [sys.executable, "-m", "pytest", test_id, "-q", "-p", "no:cacheprovider"],
        cwd=project_dir,
        capture_output=True,
        text=True,
    )
    outcome = "passed" if proc.returncode == 0 else "failed"
    cid = jcs_hash(_witness_value(cc, rc, test_id, outcome))
    return Witness(cc, rc, test_id, outcome, cid)


def verify(witness: Witness, project_dir: str, code_paths: List[str]) -> Tuple[str, str]:
    """Verify a witness by RECOMPUTATION.  Returns (verdict, reason).

    DISCHARGED iff: the supplied code hashes to the witness's ``codeCid`` (the
    binding), re-running reproduces the witness CID, and the witnessed outcome is
    ``passed``.  Anything else is REFUSED — fail-closed.
    """
    actual = code_cid(code_paths)
    if actual != witness.code_cid:
        return ("REFUSED", "code CID mismatch — this witness is not about this code")
    recomputed = run_and_witness(project_dir, witness.test_id, code_paths)
    if recomputed.cid != witness.cid:
        return ("REFUSED", f"witness did not reproduce (re-run outcome: {recomputed.outcome})")
    if witness.outcome != "passed":
        return ("REFUSED", f"witnessed outcome is {witness.outcome!r}, not a discharge")
    return ("DISCHARGED", "re-ran on pinned code; assertions held; witness CID reproduced")
