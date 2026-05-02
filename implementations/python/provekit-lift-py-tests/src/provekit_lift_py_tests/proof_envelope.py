# SPDX-License-Identifier: Apache-2.0
#
# Proof-envelope catalog stub: stored-only binaryCid + metadata field.
#
# Status: STORED-ONLY. The Rust kit at provekit-proof-envelope/src/proof.rs
# emits a *signed CBOR catalog* whose BLAKE3-512 hash IS the .proof bundle
# CID. Porting CBOR + ed25519 signing for one optional field is out of
# scope for this PR; the full envelope kit will land separately.
#
# What this module provides:
#   - ProofEnvelopeInput dataclass holding the same logical shape as the
#     Rust input (binaryCid, metadata, members, signer, declaredAt).
#   - envelope_body_to_value(): emits the *unsigned body* as a
#     canonicalizer Value tree (NOT the signed CBOR bytes the protocol
#     normatively specifies). Useful only for round-tripping the field
#     set in Python pipelines and pinning JCS conformance of the body
#     payload, not for producing real .proof bundles.
#
# DO NOT use envelope_body_to_value() bytes as a CID. The protocol CID
# is BLAKE3-512 of the signed CBOR bytes, not the JCS-Value bytes.
# Running `provekit verify` against a bundle built only from this
# module's output WILL FAIL until the CBOR envelope is ported.
#
# Reference (authoritative):
#   implementations/rust/provekit-proof-envelope/src/proof.rs
#   protocol/specs/2026-04-30-proof-file-format.md

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple

from .canonicalizer import Value, vobj, vstr


@dataclass
class ProofEnvelopeInput:
    """Logical input shape for a .proof catalog memento.

    Mirrors `ProofEnvelopeInput` in
    implementations/rust/provekit-proof-envelope/src/proof.rs. Field
    ``binary_cid`` corresponds to the Rust ``binaryCid`` (back-pin from
    the .proof bundle to the binary it attests).

    members: map from member CID (e.g. ``blake3-512:<hex>``) to that
    member's canonical bytes (JCS-JSON for memento envelopes).
    """

    name: str
    version: str
    members: Dict[str, bytes]
    signer_cid: str
    declared_at: str  # ISO-8601, ms precision, trailing 'Z'
    binary_cid: Optional[str] = None
    metadata: Optional[Dict[str, str]] = None


def envelope_body_to_value(env: ProofEnvelopeInput) -> Value:
    """Logical body shape as a canonicalizer Value tree.

    STORED-ONLY representation. NOT the signed CBOR catalog bytes.
    The canonicalizer's JCS pass sorts keys; insertion order here
    is for human-readability and parity with the Rust emit order in
    `body_pairs_unsigned`.

    members are emitted as ``{cid -> base16(bytes)}`` for round-trip
    purposes; the real protocol stores raw CBOR byte strings, not hex.
    """
    pairs: List[Tuple[str, Value]] = [
        ("kind", vstr("catalog")),
        ("name", vstr(env.name)),
        ("version", vstr(env.version)),
        # Insertion-order members; JCS sorts at emit.
        ("members", vobj([
            (cid, vstr(_bytes_to_hex(b))) for cid, b in env.members.items()
        ])),
        ("signer", vstr(env.signer_cid)),
        ("declaredAt", vstr(env.declared_at)),
    ]
    if env.binary_cid is not None:
        pairs.append(("binaryCid", vstr(env.binary_cid)))
    if env.metadata is not None:
        pairs.append((
            "metadata",
            vobj([(k, vstr(v)) for k, v in env.metadata.items()]),
        ))
    return vobj(pairs)


def _bytes_to_hex(b: bytes) -> str:
    if not isinstance(b, (bytes, bytearray)):
        raise TypeError("members values must be bytes")
    return bytes(b).hex()
