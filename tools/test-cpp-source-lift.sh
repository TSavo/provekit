#!/bin/sh
# Build and run the C++ source-language lift kit tests.

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
LLVM_CONFIG="${LLVM_CONFIG:-}"
OUT_DIR="$WORKSPACE/implementations/cpp/target"
OUT="$OUT_DIR/test-cpp-source-lift"

clang_required() {
    [ "${PK_CPP_ENABLE_CLANG_AST:-}" = "1" ]
}

find_llvm_config() {
    if [ -n "$LLVM_CONFIG" ]; then
        if command -v "$LLVM_CONFIG" >/dev/null 2>&1; then
            command -v "$LLVM_CONFIG"
            return 0
        fi
        if [ -x "$LLVM_CONFIG" ]; then
            echo "$LLVM_CONFIG"
            return 0
        fi
        return 1
    fi

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
            command -v "$candidate"
            return 0
        fi
        if [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

skip_or_fail() {
    reason="$1"
    if clang_required; then
        echo "test-cpp-source-lift: libclang/llvm-config not available; cpp-source lifter tests required by PK_CPP_ENABLE_CLANG_AST=1: $reason" >&2
        exit 1
    fi
    echo "test-cpp-source-lift: libclang/llvm-config not available; skipping cpp-source lifter tests (set PK_CPP_ENABLE_CLANG_AST=1 to require it): $reason" >&2
    rm -f "$OUT"
    exit 0
}

LLVM_CONFIG="$(find_llvm_config || true)"
[ -n "$LLVM_CONFIG" ] || skip_or_fail "llvm-config not found"

LLVM_BIN_DIR="$("$LLVM_CONFIG" --bindir 2>/dev/null)" || skip_or_fail "$LLVM_CONFIG --bindir failed"
LLVM_INC="$("$LLVM_CONFIG" --includedir 2>/dev/null)" || skip_or_fail "$LLVM_CONFIG --includedir failed"
LLVM_LIB="$("$LLVM_CONFIG" --libdir 2>/dev/null)" || skip_or_fail "$LLVM_CONFIG --libdir failed"

[ -f "$LLVM_INC/clang-c/Index.h" ] || skip_or_fail "clang-c/Index.h not found under $LLVM_INC"

LIBCLANG_FOUND=0
for candidate in "$LLVM_LIB"/libclang.so "$LLVM_LIB"/libclang.so.* "$LLVM_LIB"/libclang.dylib "$LLVM_LIB"/libclang.*.dylib "$LLVM_LIB"/libclang.a; do
    if [ -e "$candidate" ]; then
        LIBCLANG_FOUND=1
        break
    fi
done
[ "$LIBCLANG_FOUND" = "1" ] || skip_or_fail "libclang library not found under $LLVM_LIB"

BLAKE3_INC="${BLAKE3_INC:-/usr/local/opt/blake3/include}"
BLAKE3_LIB="${BLAKE3_LIB:-/usr/local/opt/blake3/lib}"
CXX="${CXX:-$LLVM_BIN_DIR/clang++}"

mkdir -p "$OUT_DIR"

"$CXX" -std=c++17 -O2 -Wall -Wextra -Werror \
    -I"$WORKSPACE/implementations/cpp/sugar-lift-cpp-source/include" \
    -I"$WORKSPACE/implementations/cpp/sugar-ir-symbolic/include" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$LLVM_INC" \
    -I"$BLAKE3_INC" \
    "$WORKSPACE/implementations/cpp/sugar-lift-cpp-source/src/cpp_source_lifter.cpp" \
    "$WORKSPACE/implementations/cpp/sugar-lift-cpp-source/tests/test_cpp_source_lifter.cpp" \
    "$WORKSPACE/implementations/cpp/sugar/canonicalizer/jcs.cpp" \
    "$WORKSPACE/implementations/cpp/sugar/canonicalizer/hash.cpp" \
    -L"$LLVM_LIB" -L"$BLAKE3_LIB" -Wl,-rpath,"$LLVM_LIB" -Wl,-rpath,"$BLAKE3_LIB" -lclang -lblake3 \
    -o "$OUT"

"$OUT"
