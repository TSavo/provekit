#!/bin/sh
# Build + run the C++ peer self-contracts orchestrator.
#
# Mirrors the Rust orchestrator (mint-self-contracts): lifts native C++
# assertion surfaces with provekit-lift-cpp, mints the lifted contracts
# under the foundation key, bundles into a deterministic .proof, and
# asserts byte-determinism by minting twice.
#
# Prereqs:
#   - openssl@3 (libcrypto)
#       - macOS Intel:    /usr/local/opt/openssl@3 (brew install openssl@3)
#       - macOS Silicon:  /opt/homebrew/opt/openssl@3
#       - Ubuntu/Debian:  apt install libssl-dev (headers under /usr/include,
#                          lib under /usr/lib/x86_64-linux-gnu)
#       - Override with OPENSSL_PREFIX=...
#   - nlohmann/json header-only
#       - macOS Intel:    /usr/local/include (brew install nlohmann-json)
#       - macOS Silicon:  /opt/homebrew/include
#       - Ubuntu/Debian:  apt install nlohmann-json3-dev (/usr/include)
#       - Override with NLOHMANN_INC=...
#   - BLAKE3: vendored at tools/blake3-vendored/ (portable C, no system
#     blake3 install required). No override knob; the script always uses
#     the vendored copy so the build is hermetic.
#
# Usage:
#   tools/build-cpp-self-contracts.sh             # build + run
#   tools/build-cpp-self-contracts.sh --build-only # build only, leave bin
#   tools/build-cpp-self-contracts.sh [out_dir]
#
# Default out_dir is /tmp/provekit-cpp-self-out (cleaned and recreated).

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
CPP="$WORKSPACE/implementations/cpp/provekit"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"
B3_DIR="$WORKSPACE/tools/blake3-vendored"

# --- Resolve OpenSSL prefix --------------------------------------------------
# Try in order: explicit env, brew Intel, brew Silicon, system Linux paths.
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
    echo "  macOS:  brew install openssl@3" >&2
    echo "  Ubuntu: sudo apt install libssl-dev" >&2
    echo "  override with: OPENSSL_PREFIX=/path/to/openssl" >&2
    exit 1
fi

# --- Resolve nlohmann/json include path --------------------------------------
detect_nlohmann_inc() {
    if [ -n "${NLOHMANN_INC:-}" ] && [ -f "$NLOHMANN_INC/nlohmann/json.hpp" ]; then
        echo "$NLOHMANN_INC"
        return
    fi
    for cand in /usr/local/include /opt/homebrew/include /usr/include; do
        if [ -f "$cand/nlohmann/json.hpp" ]; then
            echo "$cand"
            return
        fi
    done
    echo ""
}
NLOHMANN_INC="$(detect_nlohmann_inc)"
if [ -z "$NLOHMANN_INC" ]; then
    echo "error: nlohmann/json.hpp not found." >&2
    echo "  macOS:  brew install nlohmann-json" >&2
    echo "  Ubuntu: sudo apt install nlohmann-json3-dev" >&2
    echo "  override with: NLOHMANN_INC=/path/containing/nlohmann/" >&2
    exit 1
fi

# --- Output binary path ------------------------------------------------------
BUILD_ONLY=0
if [ "${1:-}" = "--build-only" ]; then
    BUILD_ONLY=1
    shift
fi

OUT_DIR="${1:-/tmp/provekit-cpp-self-out}"
if [ "$BUILD_ONLY" = "0" ]; then
    rm -rf "$OUT_DIR"
    mkdir -p "$OUT_DIR"
fi

CLEANUP_PATHS=""
cleanup() {
    [ -n "$CLEANUP_PATHS" ] && rm -rf $CLEANUP_PATHS
}
trap cleanup EXIT INT TERM

if [ "$BUILD_ONLY" = "1" ]; then
    STABLE_BIN="$WORKSPACE/implementations/cpp/target/mint_cpp_self_contracts"
    mkdir -p "$(dirname "$STABLE_BIN")"
    OUT_BIN="$STABLE_BIN"
else
    OUT_BIN="$(mktemp -t mint_cpp_self_contracts.XXXXXX)"
    CLEANUP_PATHS="$CLEANUP_PATHS $OUT_BIN"
fi

# --- Compile -----------------------------------------------------------------
# BLAKE3: vendored portable C only. Disabling all SIMD paths makes the
# dispatcher select the portable code unconditionally; no asm files needed,
# builds clean on x86_64, arm64, anywhere clang runs.
B3_FLAGS="-DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0"

CC="${CC:-clang}"
CXX="${CXX:-clang++}"

# The self-contract orchestrator shells out to the existing native C++ lifter
# at runtime. Build it first so direct script invocations have the same path
# the orchestrator uses.
"$WORKSPACE/tools/build-cpp-lift.sh" >/dev/null

# Compile vendored BLAKE3 .c sources to objects first; clang++ refuses to
# compile C with -std=c++17. Place objects in a tempdir; cleaned on exit.
B3_OBJ_DIR="$(mktemp -d -t b3-obj.XXXXXX)"
CLEANUP_PATHS="$CLEANUP_PATHS $B3_OBJ_DIR"

for src in blake3.c blake3_dispatch.c blake3_portable.c; do
    "$CC" -O2 -Wall $B3_FLAGS -c "$B3_DIR/$src" \
        -o "$B3_OBJ_DIR/${src%.c}.o"
done

"$CXX" -std=c++17 -O2 -Wall -Wextra -Werror \
    -I"$OPENSSL_PREFIX/include" \
    -I"$B3_DIR" \
    -I"$KIT_INC" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$NLOHMANN_INC" \
    -L"$OPENSSL_PREFIX/lib" \
    "$B3_OBJ_DIR/blake3.o" \
    "$B3_OBJ_DIR/blake3_dispatch.o" \
    "$B3_OBJ_DIR/blake3_portable.o" \
    "$CPP/canonicalizer/hash.cpp" \
    "$CPP/canonicalizer/jcs.cpp" \
    "$CPP/canonicalizer/property_hash.cpp" \
    "$CPP/proof-envelope/cbor.cpp" \
    "$CPP/proof-envelope/cbor_decoder.cpp" \
    "$CPP/proof-envelope/sign_ed25519.cpp" \
    "$CPP/proof-envelope/proof_envelope.cpp" \
    "$CPP/claim-envelope/mint.cpp" \
    "$CPP/claim-envelope/value_from_kit.cpp" \
    "$WORKSPACE/implementations/cpp/provekit-self-contracts/cross_kit_bridges.cpp" \
    "$WORKSPACE/implementations/cpp/provekit-self-contracts/mint_cpp_self_contracts.cpp" \
    -lcrypto \
    -o "$OUT_BIN"

if [ "$BUILD_ONLY" = "1" ]; then
    echo "built: $OUT_BIN"
    exit 0
fi

PROVEKIT_WORKSPACE_ROOT="$WORKSPACE" "$OUT_BIN" "$OUT_DIR"
