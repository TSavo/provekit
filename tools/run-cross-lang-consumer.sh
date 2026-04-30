#!/bin/sh
# Build + run the C++ cross-language consumer demo:
#   Go publishes → C++ verifies → C++ catches parse_int(num(0)).
#
# Prereqs:
#   - z3 installed and on PATH
#   - openssl@3 at $OPENSSL_PREFIX (or /usr/local/opt/openssl@3)
#   - nlohmann/json header-only at $NLOHMANN_PREFIX (or /usr/local/include
#     via brew install nlohmann-json)
#
# Usage:
#   tools/run-cross-lang-consumer.sh

set -e

OPENSSL_PREFIX="${OPENSSL_PREFIX:-/usr/local/opt/openssl@3}"
NLOHMANN_INC="${NLOHMANN_INC:-/usr/local/include}"
WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
CPP="$WORKSPACE/implementations/cpp/provekit"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"
GO_PUBLISH="$WORKSPACE/implementations/go/provekit-ir-symbolic"
GO_OUT_DIR="${GO_OUT_DIR:-/tmp/go-kit-out}"

# ---- 1. Run the Go publisher ----
mkdir -p "$GO_OUT_DIR"
( cd "$GO_PUBLISH" && go run ./cmd/go-kit-publish "$GO_OUT_DIR" )
GO_PROOF="$(ls "$GO_OUT_DIR"/*.proof | head -1)"
if [ -z "$GO_PROOF" ]; then
    echo "ERROR: Go publisher did not produce a .proof in $GO_OUT_DIR" >&2
    exit 1
fi
echo "Go .proof: $GO_PROOF"

# ---- 2. Build the C++ consumer ----
OUT_BIN="$(mktemp -t cross_lang_consumer.XXXXXX)"
trap 'rm -f "$OUT_BIN"' EXIT

clang++ -std=c++17 -O2 -Wall -Wextra \
    -I"$OPENSSL_PREFIX/include" \
    -I"$KIT_INC" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$NLOHMANN_INC" \
    -L"$OPENSSL_PREFIX/lib" \
    "$CPP/canonicalizer/sha256.cpp" \
    "$CPP/canonicalizer/jcs.cpp" \
    "$CPP/canonicalizer/property_hash.cpp" \
    "$CPP/proof-envelope/cbor.cpp" \
    "$CPP/proof-envelope/cbor_decoder.cpp" \
    "$CPP/proof-envelope/sign_ed25519.cpp" \
    "$CPP/proof-envelope/proof_envelope.cpp" \
    "$CPP/claim-envelope/mint.cpp" \
    "$CPP/claim-envelope/value_from_kit.cpp" \
    "$CPP/verifier/load_all_proofs.cpp" \
    "$CPP/verifier/enumerate_callsites.cpp" \
    "$CPP/verifier/resolve_target.cpp" \
    "$CPP/verifier/instantiate.cpp" \
    "$CPP/verifier/smt_emitter.cpp" \
    "$CPP/verifier/solve_obligation.cpp" \
    "$CPP/verifier/report.cpp" \
    "$CPP/verifier/runner.cpp" \
    "$WORKSPACE/implementations/cpp/provekit-ir-symbolic/example/cross_lang_consumer.cpp" \
    -lcrypto \
    -o "$OUT_BIN"

# ---- 3. Run ----
"$OUT_BIN" "$GO_PROOF"
