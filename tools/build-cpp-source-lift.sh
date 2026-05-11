#!/bin/sh
# Build the C++ source-language lift plugin binary (provekit-lift-cpp-source).

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
LLVM_CONFIG="${LLVM_CONFIG:-}"

if [ -z "$LLVM_CONFIG" ]; then
    for candidate in \
        llvm-config \
        /usr/local/opt/llvm/bin/llvm-config \
        /usr/local/opt/llvm@22/bin/llvm-config \
        /usr/local/opt/llvm@21/bin/llvm-config \
        /opt/homebrew/opt/llvm/bin/llvm-config \
        /opt/homebrew/opt/llvm@22/bin/llvm-config \
        /opt/homebrew/opt/llvm@21/bin/llvm-config
    do
        if command -v "$candidate" >/dev/null 2>&1; then
            LLVM_CONFIG="$candidate"
            break
        fi
        if [ -x "$candidate" ]; then
            LLVM_CONFIG="$candidate"
            break
        fi
    done
fi

if [ -z "$LLVM_CONFIG" ]; then
    echo "build-cpp-source-lift: llvm-config not found" >&2
    exit 1
fi

LLVM_BIN_DIR="$("$LLVM_CONFIG" --bindir)"
LLVM_INC="$("$LLVM_CONFIG" --includedir)"
LLVM_LIB="$("$LLVM_CONFIG" --libdir)"
BLAKE3_INC="${BLAKE3_INC:-/usr/local/opt/blake3/include}"
BLAKE3_LIB="${BLAKE3_LIB:-/usr/local/opt/blake3/lib}"
CXX="${CXX:-$LLVM_BIN_DIR/clang++}"
OUT_DIR="$WORKSPACE/implementations/cpp/target"
OUT_BIN="$OUT_DIR/provekit-lift-cpp-source"

if [ "${1:-}" = "--out" ] && [ -n "${2:-}" ]; then
    OUT_BIN="$2"
fi

mkdir -p "$(dirname "$OUT_BIN")"

"$CXX" -std=c++17 -O2 -Wall -Wextra -Werror \
    -I"$WORKSPACE/implementations/cpp/provekit-lift-cpp-source/include" \
    -I"$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$LLVM_INC" \
    -I"$BLAKE3_INC" \
    "$WORKSPACE/implementations/cpp/provekit-lift-cpp-source/src/cpp_source_lifter.cpp" \
    "$WORKSPACE/implementations/cpp/provekit-lift-cpp-source/src/main.cpp" \
    "$WORKSPACE/implementations/cpp/provekit/canonicalizer/jcs.cpp" \
    "$WORKSPACE/implementations/cpp/provekit/canonicalizer/hash.cpp" \
    -L"$LLVM_LIB" -L"$BLAKE3_LIB" -Wl,-rpath,"$LLVM_LIB" -Wl,-rpath,"$BLAKE3_LIB" -lclang -lblake3 \
    -o "$OUT_BIN"

echo "built: $OUT_BIN"
