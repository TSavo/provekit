# SPDX-License-Identifier: Apache-2.0
#
# Witness lift surface (sugar-lift/1 NDJSON). At LIFT time this is the
# PRODUCER: it runs each test under pytest and emits a ContractDecl carrying the
# witnessed run as a `custom` EvidenceTerm. `mint` serializes it into a real
# signed .proof; `prove` then discharges it BY RECOMPUTE (the verifier's custom-
# evidence arm spawns the discharge command).
from __future__ import annotations

import json
import os
import sys
from typing import Any, List, Optional

from sugar_lift_py_tests.ir import (
    ContractDecl,
    EvidenceCertificate,
    EvidenceTerm,
    atomic,
    declarations_to_value,
)
from sugar_lift_py_tests.canonicalizer import encode_jcs, blake3_512_of

import base64

from .witness import (
    Witness, run_and_witness, run_file_witnesses, witness_memento, witness_body,
    write_witness_bundle, build_suite_bundle, witness_package_memento, runtime_cid,
    _cid_filename,
)

KIT_ID = "python-pytest-witness"
KIT_VERSION = "0.1.0"
KIT_DECLARATION_RPC_METHOD = "sugar.plugin.kit_declaration"
RESOLVE_WITNESS_RPC_METHOD = "sugar.plugin.resolve_witness"


def _send(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":"), ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _recv() -> Optional[dict]:
    line = sys.stdin.readline()
    if not line:
        return None
    try:
        return json.loads(line)
    except json.JSONDecodeError:
        return None


def _iter_python_files(workspace_root: str, source_paths: List[str]) -> List[str]:
    out: List[str] = []
    for sp in source_paths:
        base = sp if os.path.isabs(sp) else os.path.join(workspace_root, sp)
        if os.path.isfile(base) and base.endswith(".py"):
            out.append(base)
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in {".git", "__pycache__", ".pytest_cache"}]
            for fn in filenames:
                if fn.endswith(".py"):
                    out.append(os.path.join(dirpath, fn))
    return sorted(set(out))


def handle_lift(msg_id: Any, params: dict) -> None:
    ws = str(params.get("workspace_root", "."))
    sps = params.get("source_paths", ["."])
    try:
        pyfiles = _iter_python_files(ws, sps)
        rels = [os.path.relpath(p, ws) for p in pyfiles]
        code_rels = [r for r in rels if not os.path.basename(r).startswith("test_")]
        test_rels = [r for r in rels if os.path.basename(r).startswith("test_")]
        decls: List[ContractDecl] = []
        mementos: List[dict] = []
        if test_rels:
            # PER-TEST run, but ONE proof member. The whole suite is a WITNESS
            # PACKAGE: a content-addressed `.witness` bundle of per-test bodies,
            # cid = blake3(bundle). The proof carries ONE WitnessPackageMemento
            # (64 bytes) + ONE contract whose evidence pins the package cid -- not
            # N mementos. The verifier asks the oracle to discharge by re-running
            # the suite and reproducing the package cid (`discharge_bundle`).
            bundle_bytes, bundle_cid, witnesses = build_suite_bundle(ws, test_rels, code_rels)
            passed = sum(1 for w in witnesses if w.outcome == "passed")
            try:
                bundle_dir = os.path.join(ws, ".sugar", "witnesses")
                os.makedirs(bundle_dir, exist_ok=True)
                with open(os.path.join(bundle_dir, _cid_filename(bundle_cid, ".witness")), "wb") as f:
                    f.write(bundle_bytes)
            except OSError:
                pass  # the package is audit material; never fail the lift on a write error
            proof_data = json.dumps(
                {"kind": "witness-package", "packageCid": bundle_cid,
                 "testFiles": sorted(test_rels), "codeFiles": sorted(code_rels),
                 "count": len(witnesses), "passed": passed},
                sort_keys=True, separators=(",", ":"),
            )
            cert = EvidenceCertificate(
                tool="pytest", version=runtime_cid(), formula_hash=bundle_cid, proof_data=proof_data,
            )
            ev = EvidenceTerm(proof_type="custom", certificate=cert)
            decls.append(ContractDecl(name=f"witness-package:{bundle_cid}",
                                      inv=atomic("witnessed", []), evidence=ev))
            mementos.append(witness_package_memento(
                bundle_cid, test_rels, code_rels, len(witnesses), passed))
        ir = json.loads(encode_jcs(declarations_to_value(decls))) if decls else []
        # The signed WitnessMementos flow as `ir` members (kind "witness-memento"):
        # mint envelopes each into the .proof via its per-kind dispatch, so the
        # .proof carries the signed pointer the rust verifier enumerates. (Also
        # surfaced as `witness_mementos` for non-mint consumers.)
        ir = ir + mementos
        _send({"jsonrpc": "2.0", "id": msg_id, "result": {
            "kind": "ir-document", "ir": ir, "witness_mementos": mementos,
            "implications": [], "diagnostics": [], "warnings": [],
        }})
    except Exception as e:
        import traceback
        _send({"jsonrpc": "2.0", "id": msg_id, "error": {
            "code": -32603, "message": str(e), "data": traceback.format_exc()}})


def handle_resolve_witness(msg_id: Any, params: dict) -> None:
    """The ORACLE'S RPC resolve surface. Given a WitnessMemento (and where its
    body lives), RESOLVE the body bytes and return them base64-encoded. The
    oracle returns CONTENT, not a verdict: verification lives in the rust CLI,
    which blake3's these bytes itself and compares to the pinned witness_cid. The
    oracle is untrusted -- it must be verified -- so it only hands over the body.

    Resolution order:
      - PACKAGE: the body is a CID-named file in the witness package (a witness
        of ANY kind -- poem, CI log, compiler report -- resolves this way).
      - RECOMPUTE: a re-runnable pytest-witness is reproduced by re-running the
        pinned test and rebuilding the canonical body.
    A body that cannot be resolved -> error; the verifier treats that as refusal."""
    try:
        memento = params.get("memento") or {}
        cid = memento.get("witness_cid") or params.get("witness_cid")
        if not cid:
            raise RuntimeError("resolve_witness requires a witness_cid")
        ws = params.get("workspace_root")
        package_dir = params.get("package_dir")
        body: Optional[bytes] = None
        resolved_by: Optional[str] = None
        # 1. PACKAGE -- CID-named witness body, deployed separately.
        if package_dir:
            pdir = package_dir if os.path.isabs(package_dir) else os.path.join(ws or ".", package_dir)
            path = os.path.join(pdir, cid.replace(":", "_") + ".witness")
            if os.path.isfile(path):
                with open(path, "rb") as f:
                    body = f.read()
                resolved_by = "package"
        # 2a. PACKAGE RECOMPUTE -- a whole-suite WitnessPackageMemento reproduces
        # by re-running the suite and rebuilding the content-addressed bundle.
        if body is None and ws and memento.get("witness_kind") == "pytest-witness-package":
            from .witness import build_suite_bundle
            buf, rcid, _ = build_suite_bundle(
                ws, list(memento.get("test_files", [])), list(memento.get("code_files", []))
            )
            if rcid != cid:
                raise RuntimeError(
                    f"witness package did not reproduce: recomputed {rcid}, pinned {cid}"
                )
            body = buf
            resolved_by = "recompute"
        # 2. RECOMPUTE -- re-run the pinned test, rebuild the canonical body.
        # `code_files` is PRESENT-not-truthy: an all-tests project pins an EMPTY
        # code_files (the code under test is the installed library, not a local
        # file), and `[]` is falsy -- gating on truthiness would wrongly declare
        # a trivially re-runnable witness "not re-runnable". The reconstruction
        # below pins the empty list into the witness body, so this stays sound.
        if body is None and ws and memento.get("test") and memento.get("code_files") is not None:
            # Don't execute attacker-supplied paths on a memento whose own fields
            # don't even hash to its pinned CID. The witness body is a pure
            # function of (code_cid, runtime_cid, test, outcome, code_files), so a
            # consistent memento MUST reconstruct to `cid` before we run anything.
            probe = Witness(
                code_cid=str(memento.get("code_cid", "")),
                runtime_cid=str(memento.get("runtime_cid", "")),
                test_id=str(memento["test"]),
                outcome=str(memento.get("outcome", "")),
                code_files=tuple(sorted(str(c) for c in memento["code_files"])),
                cid=cid,
            )
            if blake3_512_of(witness_body(probe)) != cid:
                raise RuntimeError(
                    f"memento fields do not reconstruct witness_cid {cid}; "
                    "refusing to re-run a tampered memento"
                )
            w = run_and_witness(ws, memento["test"], list(memento["code_files"]))
            body = witness_body(w)
            resolved_by = "recompute"
        if body is None:
            raise RuntimeError(
                f"cannot resolve witness body for {cid}: no package file and not re-runnable"
            )
        _send({"jsonrpc": "2.0", "id": msg_id, "result": {
            "witness_cid": cid,
            "body_b64": base64.b64encode(body).decode("ascii"),
            "resolved_by": resolved_by,
        }})
    except Exception as e:
        import traceback
        _send({"jsonrpc": "2.0", "id": msg_id, "error": {
            "code": -32603, "message": str(e), "data": traceback.format_exc()}})


def main() -> None:
    while True:
        msg = _recv()
        if msg is None:
            break
        method = msg.get("method")
        mid = msg.get("id")
        if method == "initialize":
            _send({"jsonrpc": "2.0", "id": mid, "result": {
                "name": "sugar-lsp-pytest-witness", "version": KIT_VERSION,
                "protocol_version": "sugar-lsp-shared/1", "kit_id": KIT_ID,
                "capabilities": {"source_surfaces": ["python-pytest-witness"], "entry_kinds": [],
                                 "diagnostic_codes": [], "status_kinds": ["prove"]}}})
        elif method == KIT_DECLARATION_RPC_METHOD:
            _send({"jsonrpc": "2.0", "id": mid, "result": {
                "kit": {"id": KIT_ID, "language": "python", "version": KIT_VERSION},
                "rpc": {"methods": [{"name": "initialize", "required": True},
                                    {"name": KIT_DECLARATION_RPC_METHOD, "required": True},
                                    {"name": "lift", "required": True},
                                    {"name": RESOLVE_WITNESS_RPC_METHOD, "required": False},
                                    {"name": "shutdown", "required": False}]},
                "proofResolution": {"strategy": "pip"}, "effectKinds": [], "effectLeaves": [],
                "guardPredicates": [], "controlCarriers": [], "residueCategories": []}})
        elif method == "lift":
            handle_lift(mid, msg.get("params", {}))
        elif method == RESOLVE_WITNESS_RPC_METHOD:
            handle_resolve_witness(mid, msg.get("params", {}))
        elif method == "shutdown":
            _send({"jsonrpc": "2.0", "id": mid, "result": None})
            break
        elif mid is not None:
            _send({"jsonrpc": "2.0", "id": mid, "result": None})


if __name__ == "__main__":
    main()
