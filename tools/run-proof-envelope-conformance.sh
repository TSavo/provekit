#!/bin/sh
# Build + run the C++ ↔ TS proof-envelope conformance test.
#
# Bazel currently can't drive this build because of its workspace-root
# include-path validation against /usr/local/opt/openssl@3. Direct
# clang invocation is the temporary path; γ-step-3 wires it back into
# Bazel via rules_foreign_cc + libsodium.
#
# Usage:
#   tools/run-proof-envelope-conformance.sh

set -e

OPENSSL_PREFIX="${OPENSSL_PREFIX:-/usr/local/opt/openssl@3}"
WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
CPP_DIR="$WORKSPACE/implementations/cpp/provekit"
OUT_BIN="$(mktemp -t proof_envelope_conformance.XXXXXX)"
trap 'rm -f "$OUT_BIN"' EXIT

clang++ -std=c++17 -O2 -Wall -Wextra \
    -I"$OPENSSL_PREFIX/include" \
    -L"$OPENSSL_PREFIX/lib" \
    "$CPP_DIR/proof-envelope/cbor.cpp" \
    "$CPP_DIR/proof-envelope/sign_ed25519.cpp" \
    "$CPP_DIR/proof-envelope/proof_envelope.cpp" \
    "$CPP_DIR/proof-envelope/proof_envelope_test.cpp" \
    "$CPP_DIR/canonicalizer/sha256.cpp" \
    -lcrypto \
    -o "$OUT_BIN"

"$OUT_BIN"
