#!/bin/sh
# Build + run the C++ proof-envelope smoke test (v1.1.0).
#
# During the v1.1.0 transition the TypeScript reference implementation
# is being ported in parallel. Until TS lands, this is a self-conformance
# gate (determinism + filename-CID shape + trust-root coherence).
#
# Bazel currently can't drive this build because of its workspace-root
# include-path validation against /usr/local/opt/openssl@3. Direct
# clang invocation is the temporary path; γ-step-3 wires it back into
# Bazel via rules_foreign_cc + libsodium + libblake3.
#
# Usage:
#   tools/run-proof-envelope-conformance.sh

set -e

OPENSSL_PREFIX="${OPENSSL_PREFIX:-/usr/local/opt/openssl@3}"
BLAKE3_PREFIX="${BLAKE3_PREFIX:-/usr/local}"
WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
CPP_DIR="$WORKSPACE/implementations/cpp/provekit"
OUT_BIN="$(mktemp -t proof_envelope_conformance.XXXXXX)"
trap 'rm -f "$OUT_BIN"' EXIT

clang++ -std=c++17 -O2 -Wall -Wextra \
    -I"$OPENSSL_PREFIX/include" \
    -I"$BLAKE3_PREFIX/include" \
    -L"$OPENSSL_PREFIX/lib" \
    -L"$BLAKE3_PREFIX/lib" \
    "$CPP_DIR/proof-envelope/cbor.cpp" \
    "$CPP_DIR/proof-envelope/sign_ed25519.cpp" \
    "$CPP_DIR/proof-envelope/proof_envelope.cpp" \
    "$CPP_DIR/proof-envelope/proof_envelope_test.cpp" \
    "$CPP_DIR/canonicalizer/hash.cpp" \
    -lcrypto -lblake3 \
    -o "$OUT_BIN"

"$OUT_BIN"
