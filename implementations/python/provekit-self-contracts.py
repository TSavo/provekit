# SPDX-License-Identifier: Apache-2.0
#
# provekit-self-contracts.py
#
# Side A orchestrator for the Python kit. Walks the canonical Python
# slab, mints each contract as a layered signed memento under the
# foundation key, and bundles them into a `.proof` envelope whose
# filename IS its catalog CID.
#
# Mirrors the rust/cpp/go/ts/ruby Side A pattern. The contractSetCid we
# produce is the BLAKE3-512 of the JCS encoding of the sorted list of
# signer-independent contractCids; identical inputs produce byte-
# identical CIDs across kits with the same authoring choices.
#
# Run modes:
#   * Direct CLI:      python3 bin/mint-python-self-contracts [outDir]
#   * Lift-protocol:   python3 bin/mint-python-self-contracts --rpc
#
# References:
#   * implementations/ruby/lib/provekit/self_contracts.rb
#   * implementations/rust/provekit-self-contracts/src/lib.rs
#   * protocol/specs/2026-04-30-lift-plugin-protocol.md
#   * protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md
#   * protocol/specs/2026-05-03-contract-set-extension.md
#
# Note on layout: this orchestrator lives at
# `implementations/python/provekit-self-contracts.py` (not under the
# `provekit-lift-py-tests` package) because it is the kit's
# self-contracts authoring entry point, not a public library API. The
# bin shim `bin/mint-python-self-contracts` invokes `main(argv)` here.

from __future__ import annotations

import base64
import json
import os
import os.path
import sys
import tempfile
import shutil
from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional


# ---------------------------------------------------------------------------
# sys.path setup: locate provekit-lift-py-tests/src without requiring
# `pip install -e .`. The `mint-python` Makefile target does not run a
# build-python step; we depend on path-injection to find the modules.
# ---------------------------------------------------------------------------

_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
_LIB_SRC = os.path.join(_THIS_DIR, "provekit-lift-py-tests", "src")
if _LIB_SRC not in sys.path:
    sys.path.insert(0, _LIB_SRC)


from provekit_lift_py_tests.canonicalizer import (  # noqa: E402
    blake3_512_of,
)
from provekit_lift_py_tests.claim_envelope import (  # noqa: E402
    AuthoringKitAuthor,
    ClaimEnvelope,
    compute_contract_set_cid,
)
from provekit_lift_py_tests.ir import (  # noqa: E402
    ContractDecl,
    atomic,
    ctor,
    eq,
    gte,
    make_var,
    num,
    str_const,
    bool_const,
)
from provekit_lift_py_tests.proof_envelope import (  # noqa: E402
    ProofEnvelopeInput,
    build_proof_envelope,
)
from provekit_lift_py_tests.signing import (  # noqa: E402
    FOUNDATION_V0_SEED,
    Signer,
    ed25519_pubkey_string,
)


# ---------------------------------------------------------------------------
# Constants (cross-kit canonical, mirror ruby/rust).
# ---------------------------------------------------------------------------

PRODUCED_BY = "provekit-python-self-contracts@1.0"
DECLARED_AT = "2026-04-30T18:00:00.000Z"
CATALOG_NAME = "@provekit/python-self-contracts"
CATALOG_VERSION = "1.0.0"


# ---------------------------------------------------------------------------
# Slab authoring
#
# A slab is the set of contracts authored against one Python module of
# the kit's own crypto substrate. Each slab returns a list of
# ContractDecl values. Names MUST be unique across slabs. Predicates
# use the kit's IR primitives, mirroring ruby's slab style: shape-level
# claims (length, determinism) about the public API. Z3 cannot
# discharge most of these in isolation; the value is the living-doc IR
# shape the verifier indexes against.
# ---------------------------------------------------------------------------


def _eq_lengths(a, b):
    """Helper: return an `eq` Formula asserting two terms are equal."""
    return eq(a, b)


def slab_blake3() -> List[ContractDecl]:
    """Contracts about `provekit_lift_py_tests.canonicalizer.blake3_512_of`.

    The kit's own BLAKE3-512 binding (returns the self-identifying
    string `"blake3-512:" + 128-hex`).
    """
    s = make_var("s")
    h_str = lambda x: ctor("blake3_512_of", [x])  # noqa: E731
    return [
        ContractDecl(
            name="python_blake3_512_of_total_length_eq_139",
            out_binding="out",
            post=eq(
                ctor("string_length", [h_str(s)]),
                num(139),
            ),
        ),
        ContractDecl(
            name="python_blake3_512_of_has_blake3_512_prefix",
            out_binding="out",
            post=gte(
                ctor("string_length", [h_str(s)]),
                num(11),
            ),
        ),
        ContractDecl(
            name="python_blake3_512_of_is_deterministic",
            out_binding="out",
            post=eq(h_str(s), h_str(s)),
        ),
    ]


def slab_jcs() -> List[ContractDecl]:
    """Contracts about `provekit_lift_py_tests.canonicalizer.encode_jcs`.

    JCS (RFC 8785) is canonical-JSON serialization. Determinism is the
    operative property; null/true literal lengths pin the spec corner.
    """
    v = make_var("v")
    enc = lambda x: ctor("encode_jcs", [x])  # noqa: E731
    return [
        ContractDecl(
            name="python_jcs_encode_is_deterministic",
            out_binding="out",
            post=eq(enc(v), enc(v)),
        ),
        ContractDecl(
            name="python_jcs_encode_true_length_eq_4",
            out_binding="out",
            post=eq(
                ctor("string_length", [enc(bool_const(True))]),
                num(4),
            ),
        ),
        ContractDecl(
            name="python_jcs_encode_null_length_eq_4",
            out_binding="out",
            post=eq(
                ctor("string_length", [enc(ctor("null", []))]),
                num(4),
            ),
        ),
    ]


def slab_cbor() -> List[ContractDecl]:
    """Contracts about `cbor2.dumps(_, canonical=True)` as used by
    proof_envelope. Encoded length lower bounds and determinism."""
    s = make_var("s")
    b = make_var("b")
    k = make_var("k")
    return [
        ContractDecl(
            name="python_cbor_encode_tstr_length_gte_1",
            out_binding="out",
            post=gte(
                ctor("byte_length", [ctor("cbor_encode_tstr", [s])]),
                num(1),
            ),
        ),
        ContractDecl(
            name="python_cbor_encode_bstr_length_gte_1",
            out_binding="out",
            post=gte(
                ctor("byte_length", [ctor("cbor_encode_bstr", [b])]),
                num(1),
            ),
        ),
        ContractDecl(
            name="python_cbor_encode_key_is_deterministic",
            out_binding="out",
            post=eq(
                ctor("cbor_encode_key", [k]),
                ctor("cbor_encode_key", [k]),
            ),
        ),
    ]


def slab_signing() -> List[ContractDecl]:
    """Contracts about `provekit_lift_py_tests.signing` Ed25519 helpers.

    Length-pinning + determinism for the seed-driven path. RFC 8032 §5.1.5
    fixes 32-byte seeds, 32-byte pubkeys, 64-byte signatures.
    """
    seed = make_var("seed")
    msg = make_var("msg")
    return [
        ContractDecl(
            name="python_ed25519_signature_length_eq_64",
            out_binding="out",
            post=eq(
                ctor("byte_length", [
                    ctor("ed25519_sign_with_seed", [seed, msg]),
                ]),
                num(64),
            ),
        ),
        ContractDecl(
            name="python_ed25519_pubkey_bytes_length_eq_32",
            out_binding="out",
            post=eq(
                ctor("byte_length", [
                    ctor("ed25519_pubkey_bytes", [seed]),
                ]),
                num(32),
            ),
        ),
        ContractDecl(
            name="python_ed25519_sign_is_deterministic",
            out_binding="out",
            post=eq(
                ctor("ed25519_sign_with_seed", [seed, msg]),
                ctor("ed25519_sign_with_seed", [seed, msg]),
            ),
        ),
    ]


def slab_proof_envelope() -> List[ContractDecl]:
    """Contracts about `provekit_lift_py_tests.proof_envelope.build_proof_envelope`.

    The catalog CID is the BLAKE3-512 of the signed CBOR bytes; per
    `protocol/specs/2026-04-30-proof-file-format.md`, the
    self-identifying form is `"blake3-512:" + 128-hex`, total length
    139. Build is deterministic for fixed inputs.
    """
    inp = make_var("input")
    build = lambda x: ctor("build_proof_envelope_cid", [x])  # noqa: E731
    return [
        ContractDecl(
            name="python_proof_envelope_cid_has_blake3_512_prefix",
            out_binding="out",
            post=gte(
                ctor("string_length", [build(inp)]),
                num(11),
            ),
        ),
        ContractDecl(
            name="python_proof_envelope_cid_total_length_eq_139",
            out_binding="out",
            post=eq(
                ctor("string_length", [build(inp)]),
                num(139),
            ),
        ),
        ContractDecl(
            name="python_proof_envelope_build_is_deterministic",
            out_binding="out",
            post=eq(build(inp), build(inp)),
        ),
    ]


@dataclass(frozen=True)
class _Slab:
    label: str
    path: str
    contracts: List[ContractDecl]


def author_all_invariants() -> List[_Slab]:
    return [
        _Slab(
            label="blake3",
            path="implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/canonicalizer.py",
            contracts=slab_blake3(),
        ),
        _Slab(
            label="jcs",
            path="implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/canonicalizer.py",
            contracts=slab_jcs(),
        ),
        _Slab(
            label="cbor",
            path="implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/proof_envelope.py",
            contracts=slab_cbor(),
        ),
        _Slab(
            label="signing",
            path="implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/signing.py",
            contracts=slab_signing(),
        ),
        _Slab(
            label="proof_envelope",
            path="implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/proof_envelope.py",
            contracts=slab_proof_envelope(),
        ),
    ]


# ---------------------------------------------------------------------------
# Mint orchestrator
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class MintResult:
    cid: str
    contract_set_cid: str
    bytes_len: int
    path: str
    member_count: int
    total_contracts: int
    per_source_counts: List[Dict[str, Any]]


def mint_self_proof(out_dir: str) -> MintResult:
    """Mint every authored contract as a layered memento, bundle into a
    .proof, write to ``<out_dir>/<catalog-cid>.proof``, and return a
    :class:`MintResult`.
    """
    os.makedirs(out_dir, exist_ok=True)

    slabs = author_all_invariants()
    seed = FOUNDATION_V0_SEED
    signer = Signer.foundation_v0(producer_id=PRODUCED_BY)

    # Catalog `signer` field: BLAKE3-512(JCS-encoded UTF-8 of the
    # self-identifying pubkey string). Mirrors ruby (`signer_cid =
    # Blake3.hex(signer_pubkey)`) and rust.
    pubkey_str = ed25519_pubkey_string(seed)
    signer_cid = blake3_512_of(pubkey_str.encode("utf-8"))

    members: Dict[str, bytes] = {}
    seen_names: Dict[str, bool] = {}
    content_cids: List[str] = []
    per_source_counts: List[Dict[str, Any]] = []
    total = 0

    for slab in slabs:
        per_source_counts.append({"label": slab.label, "count": len(slab.contracts)})
        for decl in slab.contracts:
            if decl.name in seen_names:
                raise RuntimeError(
                    f"duplicate contract name `{decl.name}` across slabs"
                )
            seen_names[decl.name] = True
            envelope = ClaimEnvelope.from_contract_decl(
                decl,
                signer,
                produced_at=DECLARED_AT,
                authoring=AuthoringKitAuthor(
                    author=PRODUCED_BY,
                    note=f"self-contract from {slab.path}",
                ),
            )
            content_cids.append(envelope.contract_cid)
            members[envelope.cid] = envelope.canonical_bytes
            total += 1

    inp = ProofEnvelopeInput(
        name=CATALOG_NAME,
        version=CATALOG_VERSION,
        members=members,
        signer_cid=signer_cid,
        declared_at=DECLARED_AT,
        signer_seed=seed,
    )
    built = build_proof_envelope(inp)

    if not built.cid.startswith("blake3-512:"):
        raise RuntimeError(f"internal: cid missing blake3-512 prefix: {built.cid}")

    cset_cid = compute_contract_set_cid(content_cids)
    out_path = os.path.join(out_dir, f"{built.cid}.proof")
    with open(out_path, "wb") as fh:
        fh.write(built.bytes)

    return MintResult(
        cid=built.cid,
        contract_set_cid=cset_cid,
        bytes_len=len(built.bytes),
        path=out_path,
        member_count=len(members),
        total_contracts=total,
        per_source_counts=per_source_counts,
    )


# ---------------------------------------------------------------------------
# RPC mode (lift-plugin-protocol over NDJSON stdio).
#
# Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
# Daemon-lifecycle pattern (PR #220 ts Side A): persistent stdio loop,
# EOF on stdin = graceful shutdown, explicit `shutdown` method acks
# then exits.
# ---------------------------------------------------------------------------


def _write_rpc(out, payload: Dict[str, Any]) -> None:
    out.write(json.dumps(payload, separators=(",", ":")))
    out.write("\n")
    out.flush()


def run_rpc_mode(stdin=None, stdout=None) -> int:
    if stdin is None:
        stdin = sys.stdin
    if stdout is None:
        stdout = sys.stdout

    while True:
        line = stdin.readline()
        if not line:  # EOF -> graceful shutdown
            return 0
        line = line.strip()
        if not line:
            continue

        try:
            req = json.loads(line)
        except ValueError as e:
            _write_rpc(stdout, {
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32700, "message": f"Parse error: {e}"},
            })
            continue

        req_id = req.get("id")
        method = str(req.get("method", ""))

        if method == "initialize":
            _write_rpc(stdout, {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "name": "python-self-contracts",
                    "version": "1.0.0",
                    "protocol_version": "provekit-lift/1",
                    "capabilities": {
                        "authoring_surfaces": ["python-self-contracts"],
                        "ir_version": "v1.1.0",
                        "emits_signed_mementos": True,
                    },
                },
            })
        elif method == "lift":
            tmp = tempfile.mkdtemp(prefix="provekit-python-rpc-")
            try:
                mint = mint_self_proof(tmp)
                with open(mint.path, "rb") as fh:
                    raw = fh.read()
                b64 = base64.b64encode(raw).decode("ascii")
                _write_rpc(stdout, {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "kind": "proof-envelope",
                        "filename_cid": mint.cid,
                        "contract_set_cid": mint.contract_set_cid,
                        "bytes_base64": b64,
                        "diagnostics": [],
                    },
                })
            except Exception as e:  # noqa: BLE001
                _write_rpc(stdout, {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {"code": 1005, "message": f"LIFT_FAILED: {e}"},
                })
            finally:
                shutil.rmtree(tmp, ignore_errors=True)
        elif method == "shutdown":
            _write_rpc(stdout, {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": None,
            })
            return 0
        else:
            _write_rpc(stdout, {
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {"code": -32601, "message": f"METHOD_NOT_FOUND: {method}"},
            })


# ---------------------------------------------------------------------------
# Direct CLI (also used by smoke tests).
# ---------------------------------------------------------------------------


def main(argv: List[str]) -> int:
    if "--rpc" in argv:
        return run_rpc_mode()

    out_dir = (
        argv[0]
        if argv and not argv[0].startswith("--")
        else os.path.join(
            tempfile.gettempdir(),
            f"provekit-python-self-contracts-{os.getpid()}",
        )
    )
    det_dir = os.path.join(
        tempfile.gettempdir(),
        f"provekit-python-self-determinism-{os.getpid()}",
    )
    shutil.rmtree(det_dir, ignore_errors=True)

    print("== ProvekIt Python self-contracts orchestrator ==")
    print()
    print(f"output dir: {out_dir}")

    try:
        mint_a = mint_self_proof(det_dir)
        mint_b = mint_self_proof(out_dir)
    except Exception as e:  # noqa: BLE001
        print(f"ERROR: mint failed: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        return 1

    print()
    print("authored:")
    for row in mint_b.per_source_counts:
        print(f"  {row['label']:>22}  {row['count']:>2} contracts")
    print(f"  {'[ALL]':>22}  {mint_b.total_contracts:>2} contracts (TOTAL)")

    print()
    print("minted:")
    print(f"  .proof file:        {mint_b.path}")
    print(f"  bytes:              {mint_b.bytes_len}")
    print(f"  members:            {mint_b.member_count}")
    print(f"  total contracts:    {mint_b.total_contracts}")
    print(f"  catalog CID:        {mint_b.cid}")
    print(f"  contractSetCid:     {mint_b.contract_set_cid}")

    if mint_a.cid != mint_b.cid or mint_a.contract_set_cid != mint_b.contract_set_cid:
        print()
        print("ERROR: byte-determinism check FAILED:", file=sys.stderr)
        print(f"  run A CID:              {mint_a.cid}", file=sys.stderr)
        print(f"  run B CID:              {mint_b.cid}", file=sys.stderr)
        print(f"  run A contractSetCid:   {mint_a.contract_set_cid}", file=sys.stderr)
        print(f"  run B contractSetCid:   {mint_b.contract_set_cid}", file=sys.stderr)
        shutil.rmtree(det_dir, ignore_errors=True)
        return 2

    shutil.rmtree(det_dir, ignore_errors=True)
    print("  determinism check:  OK (two runs produced identical CIDs)")
    print()
    print("== done. Python self-application: live. ==")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
