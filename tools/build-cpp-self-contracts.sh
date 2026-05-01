#!/bin/sh
# Build + run the C++ peer self-contracts orchestrator.
#
# Mirrors the Rust orchestrator (mint-self-contracts): walks every
# .invariant.cpp file by linking its registrar, mints all collected
# contracts under the foundation key, bundles into a deterministic
# .proof, asserts byte-determinism by minting twice.
#
# Prereqs:
#   - openssl@3 at $OPENSSL_PREFIX (or /usr/local/opt/openssl@3)
#   - blake3 at $BLAKE3_PREFIX (or /usr/local via `brew install blake3`)
#   - nlohmann/json header-only at $NLOHMANN_PREFIX (or /usr/local/include
#     via brew install nlohmann-json)
#
# Usage:
#   tools/build-cpp-self-contracts.sh [out_dir]
#
# Default out_dir is /tmp/provekit-cpp-self-out (cleaned and recreated).

set -e

OPENSSL_PREFIX="${OPENSSL_PREFIX:-/usr/local/opt/openssl@3}"
BLAKE3_PREFIX="${BLAKE3_PREFIX:-/usr/local}"
NLOHMANN_INC="${NLOHMANN_INC:-/usr/local/include}"
WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
CPP="$WORKSPACE/implementations/cpp/provekit"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"

OUT_DIR="${1:-/tmp/provekit-cpp-self-out}"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

OUT_BIN="$(mktemp -t mint_cpp_self_contracts.XXXXXX)"
trap 'rm -f "$OUT_BIN"' EXIT

clang++ -std=c++17 -O2 -Wall -Wextra \
    -I"$OPENSSL_PREFIX/include" \
    -I"$BLAKE3_PREFIX/include" \
    -I"$KIT_INC" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$NLOHMANN_INC" \
    -L"$OPENSSL_PREFIX/lib" \
    -L"$BLAKE3_PREFIX/lib" \
    "$CPP/canonicalizer/hash.cpp" \
    "$CPP/canonicalizer/jcs.cpp" \
    "$CPP/canonicalizer/property_hash.cpp" \
    "$CPP/proof-envelope/cbor.cpp" \
    "$CPP/proof-envelope/cbor_decoder.cpp" \
    "$CPP/proof-envelope/sign_ed25519.cpp" \
    "$CPP/proof-envelope/proof_envelope.cpp" \
    "$CPP/claim-envelope/mint.cpp" \
    "$CPP/claim-envelope/value_from_kit.cpp" \
    "$CPP/canonicalizer/jcs.invariant.cpp" \
    "$CPP/canonicalizer/hash.invariant.cpp" \
    "$CPP/canonicalizer/property_hash.invariant.cpp" \
    "$CPP/proof-envelope/cbor.invariant.cpp" \
    "$CPP/proof-envelope/sign_ed25519.invariant.cpp" \
    "$CPP/proof-envelope/proof_envelope.invariant.cpp" \
    "$CPP/claim-envelope/mint.invariant.cpp" \
    "$CPP/verifier/load_all_proofs.invariant.cpp" \
    "$CPP/verifier/enumerate_callsites.invariant.cpp" \
    "$CPP/verifier/resolve_target.invariant.cpp" \
    "$CPP/verifier/instantiate.invariant.cpp" \
    "$WORKSPACE/implementations/cpp/provekit-self-contracts/mint_cpp_self_contracts.cpp" \
    -lcrypto -lblake3 \
    -o "$OUT_BIN"

"$OUT_BIN" "$OUT_DIR"
