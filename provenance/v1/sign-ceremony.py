#!/usr/bin/env python3
"""sign-ceremony.py — execute the Ed25519 layer of the architect's ceremony.

Generates a fresh Ed25519 keypair, signs the JCS-canonical bytes of
attestation.json (minus signature + _signing_instructions), writes the
public key to pubkey.txt as `ed25519:<base64>`, fills in signer +
declaredAt + signature in attestation.json, and prints the base64-encoded
private key on stdout for the caller to stash somewhere durable
(typically `vault kv put -mount=secret sugar/provenance-ed25519
private_key=-`).

The private key is never written to disk by this script.

Idempotency:
- If attestation.json.signature is already populated (not a placeholder),
  the script refuses to overwrite. Re-running on a different version
  directory (v2/, v3/, ...) is the supported way to issue new
  attestations.

Usage:
    python3 provenance/v1/sign-ceremony.py
"""

import base64
import datetime
import json
import os
import sys
from pathlib import Path

try:
    from nacl.signing import SigningKey
except ImportError:
    print("Missing PyNaCl. Install with: pip3 install --user pynacl", file=sys.stderr)
    sys.exit(2)


def jcs(obj):
    """RFC 8785 JCS canonicalization (sufficient subset for our shapes)."""
    if isinstance(obj, dict):
        items = sorted(obj.items())
        return "{" + ",".join(
            f"{json.dumps(k, ensure_ascii=False)}:{jcs(v)}" for k, v in items
        ) + "}"
    if isinstance(obj, list):
        return "[" + ",".join(jcs(x) for x in obj) + "]"
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


def main():
    here = Path(__file__).resolve().parent
    attest_path = here / "attestation.json"
    pubkey_path = here / "pubkey.txt"

    attest = json.loads(attest_path.read_text())

    # Guard against re-signing.
    sig_field = attest.get("signature", "")
    if sig_field and not sig_field.startswith("<"):
        print(
            f"refusing to overwrite: {attest_path} already has a populated signature.",
            file=sys.stderr,
        )
        print("issue a new attestation in a fresh vN/ directory instead.", file=sys.stderr)
        sys.exit(3)

    # Generate the keypair.
    sk = SigningKey.generate()
    pk = sk.verify_key
    pub_b64 = base64.b64encode(bytes(pk)).decode()
    priv_b64 = base64.b64encode(bytes(sk)).decode()

    # Fill in the placeholder fields.
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    attest["declaredAt"] = now
    attest["signer"] = "ed25519:" + pub_b64
    attest.pop("_signing_instructions", None)

    # Sign the JCS-canonical bytes (excluding signature itself).
    msg_dict = {k: v for k, v in attest.items() if k != "signature"}
    msg = jcs(msg_dict).encode("utf-8")
    sig = sk.sign(msg).signature
    attest["signature"] = "ed25519:" + base64.b64encode(sig).decode()

    # Write back the populated attestation (preserving 2-space indent + trailing newline).
    out = json.dumps(attest, indent=2, ensure_ascii=False) + "\n"
    attest_path.write_text(out)

    # Public key file.
    pubkey_path.write_text("ed25519:" + pub_b64 + "\n")

    # Report on stderr; emit private key on stdout (caller stashes it).
    print(f"signed attestation.json at {now}", file=sys.stderr)
    print(f"public key (also in {pubkey_path.name}): ed25519:{pub_b64}", file=sys.stderr)
    print(f"signature length: {len(sig)} bytes", file=sys.stderr)
    print(f"caller: stash the following private key (base64, never check in):", file=sys.stderr)
    sys.stdout.write(priv_b64)
    sys.stdout.flush()


if __name__ == "__main__":
    main()
