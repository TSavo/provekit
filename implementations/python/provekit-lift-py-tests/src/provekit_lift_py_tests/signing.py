# SPDX-License-Identifier: Apache-2.0
#
# Ed25519 signing helper. v1.1.0 of the protocol mandates
# self-identifying signatures of the form:
#
#   "ed25519:" + base64-stdpad(64-byte-signature)
#
# And self-identifying public keys in the same form. The .proof file
# envelope itself stores its catalog signature as a RAW 64-byte CBOR
# byte string (not the prefixed string form): only the per-memento
# `producerSignature` field uses the prefixed string form, because
# memento envelopes are JCS-JSON.
#
# Backed by PyNaCl (libsodium-backed Ed25519). Byte-equivalent to
# the Rust ed25519-dalek peer: both implement RFC 8032 Ed25519.
#
# Reference:
#   implementations/rust/provekit-proof-envelope/src/sign.rs
#   implementations/csharp/Provekit.ProofEnvelope/Sign.cs

from __future__ import annotations

import base64

from nacl.signing import SigningKey, VerifyKey
from nacl.exceptions import BadSignatureError

ED25519_SIG_PREFIX = "ed25519:"
ED25519_KEY_PREFIX = "ed25519:"


def ed25519_sign_with_seed(seed: bytes, message: bytes) -> bytes:
    """Sign ``message`` with the Ed25519 private key derived from ``seed``.

    ``seed`` must be exactly 32 bytes (RFC 8032 §5.1.5 seed format).
    Returns the raw 64-byte signature. Byte-identical to the Rust
    ``ed25519_sign_with_seed`` for the same (seed, message) pair.
    """
    if len(seed) != 32:
        raise ValueError("seed must be exactly 32 bytes")
    sk = SigningKey(seed)
    return bytes(sk.sign(message).signature)


def ed25519_sign_string(seed: bytes, message: bytes) -> str:
    """Sign and return the spec's self-identifying string form.

    Format: ``"ed25519:" + base64-stdpad(64-byte-signature)``.
    Mirrors ``ed25519_sign_string`` in the Rust kit.
    """
    sig = ed25519_sign_with_seed(seed, message)
    return ED25519_SIG_PREFIX + base64.b64encode(sig).decode("ascii")


def ed25519_pubkey_string(seed: bytes) -> str:
    """Derive the public key from ``seed`` and return the self-identifying
    string form: ``"ed25519:" + base64-stdpad(32-byte-pubkey)``.

    Mirrors ``ed25519_pubkey_string`` in the Rust kit.
    """
    if len(seed) != 32:
        raise ValueError("seed must be exactly 32 bytes")
    sk = SigningKey(seed)
    vk = sk.verify_key
    return ED25519_KEY_PREFIX + base64.b64encode(bytes(vk)).decode("ascii")


def ed25519_verify_string(pubkey_string: str, sig_string: str, message: bytes) -> bool:
    """Verify ``message`` against ``sig_string`` using ``pubkey_string``.

    Both strings use the spec's self-identifying form
    (``"ed25519:" + base64-stdpad(bytes)``).

    Returns ``True`` iff the signature is valid. Returns ``False`` for
    any malformed input rather than raising, so the caller's fast-path
    and verify-failed-path stay separate.

    Mirrors ``ed25519_verify_string`` in the Rust kit.
    """
    if not pubkey_string.startswith(ED25519_KEY_PREFIX):
        return False
    if not sig_string.startswith(ED25519_SIG_PREFIX):
        return False
    try:
        pk_bytes = base64.b64decode(pubkey_string[len(ED25519_KEY_PREFIX):])
        sig_bytes = base64.b64decode(sig_string[len(ED25519_SIG_PREFIX):])
    except Exception:
        return False
    if len(pk_bytes) != 32 or len(sig_bytes) != 64:
        return False
    try:
        vk = VerifyKey(pk_bytes)
        vk.verify(message, sig_bytes)
        return True
    except BadSignatureError:
        return False
    except Exception:
        return False


# Foundation v0 seed: publicly-known, deterministic test seed.
# Documented as a test seed; v1 is HSM-generated.
# Source: tools/foundation-keygen/src/lib.rs FOUNDATION_V0_SEED.
FOUNDATION_V0_SEED: bytes = bytes([0x42] * 32)
