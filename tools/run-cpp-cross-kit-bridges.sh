#!/bin/sh
# Build + run the C++ phase-2 cross-kit bridges test.
#
# Asserts the pinned BLAKE3-512 of the JCS-canonical bytes of the 10
# lift-plugin-protocol BridgeDecls returned by
# build_lift_plugin_protocol_bridges() in
# implementations/cpp/provekit-self-contracts/cross_kit_bridges.hpp.
#
# Mirrors tools/build-cpp-self-contracts.sh: vendored-blake3, openssl@3
# from brew or system. No Bazel; clang++ direct invocation is the
# conformance-gate path until rules_foreign_cc ships.
#
# Usage:
#   tools/run-cpp-cross-kit-bridges.sh

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
CPP="$WORKSPACE/implementations/cpp/provekit"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"
B3_DIR="$WORKSPACE/tools/blake3-vendored"
SC_DIR="$WORKSPACE/implementations/cpp/provekit-self-contracts"

# --- Resolve OpenSSL prefix --------------------------------------------------
detect_openssl_prefix() {
    if [ -n "${OPENSSL_PREFIX:-}" ] && [ -d "$OPENSSL_PREFIX/include/openssl" ]; then
        echo "$OPENSSL_PREFIX"
        return
    fi
    for cand in /usr/local/opt/openssl@3 /opt/homebrew/opt/openssl@3 /usr/local /usr; do
        if [ -d "$cand/include/openssl" ]; then
            echo "$cand"
            return
        fi
    done
    echo ""
}
OPENSSL_PREFIX="$(detect_openssl_prefix)"
if [ -z "$OPENSSL_PREFIX" ]; then
    echo "error: openssl headers not found." >&2
    exit 1
fi

# --- Compile vendored BLAKE3 to objects --------------------------------------
B3_FLAGS="-DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0"
B3_OBJ_DIR="$(mktemp -d -t b3-obj.XXXXXX)"
OUT_BIN="$(mktemp -t cross_kit_bridges_test.XXXXXX)"
trap 'rm -rf "$B3_OBJ_DIR" "$OUT_BIN"' EXIT INT TERM

CC="${CC:-clang}"
CXX="${CXX:-clang++}"

for src in blake3.c blake3_dispatch.c blake3_portable.c; do
    "$CC" -O2 -Wall $B3_FLAGS -c "$B3_DIR/$src" \
        -o "$B3_OBJ_DIR/${src%.c}.o"
done

# --- Compile + link the test -------------------------------------------------
"$CXX" -std=c++17 -O2 -Wall -Wextra -Werror \
    -I"$OPENSSL_PREFIX/include" \
    -I"$B3_DIR" \
    -I"$KIT_INC" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$SC_DIR" \
    -L"$OPENSSL_PREFIX/lib" \
    "$B3_OBJ_DIR/blake3.o" \
    "$B3_OBJ_DIR/blake3_dispatch.o" \
    "$B3_OBJ_DIR/blake3_portable.o" \
    "$CPP/canonicalizer/hash.cpp" \
    "$SC_DIR/cross_kit_bridges_test.cpp" \
    -lcrypto \
    -o "$OUT_BIN"

"$OUT_BIN"
