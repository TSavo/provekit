#!/bin/sh
# Build + run the C++ cross-language consumer demo:
#   Go publishes, C++ verifies, C++ catches parse_int(num(0)).
#
# Prereqs:
#   - z3 installed and on PATH
#   - openssl@3 at $OPENSSL_PREFIX (or /usr/local/opt/openssl@3)
#   - blake3 at $BLAKE3_PREFIX (or /usr/local via `brew install blake3`)
#   - nlohmann/json header-only at $NLOHMANN_PREFIX (or /usr/local/include
#     via brew install nlohmann-json)
#
# Usage:
#   tools/run-cross-lang-consumer.sh

set -e

OPENSSL_PREFIX="${OPENSSL_PREFIX:-/usr/local/opt/openssl@3}"
BLAKE3_PREFIX="${BLAKE3_PREFIX:-/usr/local}"
NLOHMANN_INC="${NLOHMANN_INC:-/usr/local/include}"
WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
CPP="$WORKSPACE/implementations/cpp/provekit"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"
GO_PUBLISH="$WORKSPACE/implementations/go/provekit-ir-symbolic"
GO_OUT_DIR="${GO_OUT_DIR:-/tmp/go-kit-out}"

# ---- 1. Run the Go publisher (skipped if Go side hasn't ported v1.1.0 yet) ----
GO_PROOF=""
if [ -d "$GO_PUBLISH" ] && command -v go >/dev/null 2>&1; then
    mkdir -p "$GO_OUT_DIR"
    if ( cd "$GO_PUBLISH" && go run ./cmd/go-kit-publish "$GO_OUT_DIR" ); then
        GO_PROOF="$(ls "$GO_OUT_DIR"/*.proof 2>/dev/null | head -1)"
    fi
fi
if [ -n "$GO_PROOF" ]; then
    echo "Go .proof: $GO_PROOF"
else
    echo "Go publisher unavailable or did not produce a .proof; build-only mode." >&2
fi

# ---- 2. Build the C++ consumer ----
OUT_BIN="$(mktemp -t cross_lang_consumer.XXXXXX)"
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
    "$CPP/verifier/load_all_proofs.cpp" \
    "$CPP/verifier/enumerate_callsites.cpp" \
    "$CPP/verifier/resolve_target.cpp" \
    "$CPP/verifier/instantiate.cpp" \
    "$CPP/verifier/smt_emitter.cpp" \
    "$CPP/verifier/solve_obligation.cpp" \
    "$CPP/verifier/report.cpp" \
    "$CPP/verifier/runner.cpp" \
    "$WORKSPACE/implementations/cpp/provekit-ir-symbolic/example/cross_lang_consumer.cpp" \
    -lcrypto -lblake3 \
    -o "$OUT_BIN"

# ---- 3. Run (only when a .proof from Go is available) ----
if [ -n "$GO_PROOF" ]; then
    "$OUT_BIN" "$GO_PROOF"
else
    echo "Built $OUT_BIN; skipping run pending Go-side v1.1.0 port."
fi
