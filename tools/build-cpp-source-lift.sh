#!/bin/sh
# Build the C++ source-language lift plugin binary (sugar-lift-cpp-source).

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
LLVM_CONFIG="${LLVM_CONFIG:-}"
OUT_DIR="$WORKSPACE/implementations/cpp/target"
OUT_BIN="$OUT_DIR/sugar-lift-cpp-source"

if [ "${1:-}" = "--out" ] && [ -n "${2:-}" ]; then
    OUT_BIN="$2"
fi

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
        echo "build-cpp-source-lift: libclang/llvm-config not available; cpp-source lifter build required by PK_CPP_ENABLE_CLANG_AST=1: $reason" >&2
        exit 1
    fi
    echo "build-cpp-source-lift: libclang/llvm-config not available; skipping cpp-source lifter build (set PK_CPP_ENABLE_CLANG_AST=1 to require it): $reason" >&2
    rm -f "$OUT_BIN"
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

B3_DIR="$WORKSPACE/tools/blake3-vendored"
# BLAKE3: vendored portable C only. Disabling all SIMD paths makes the
# dispatcher select the portable code unconditionally; no asm files needed,
# builds clean on x86_64, arm64, anywhere clang runs.
B3_FLAGS="-DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0"
CC="${CC:-clang}"
CXX="${CXX:-$LLVM_BIN_DIR/clang++}"

mkdir -p "$(dirname "$OUT_BIN")"

# Compile vendored BLAKE3 .c sources to objects first; clang++ refuses to
# compile C with -std=c++17. Place objects in a tempdir; cleaned on exit.
B3_OBJ_DIR="$(mktemp -d -t b3-obj.XXXXXX)"
cleanup() {
    rm -rf "$B3_OBJ_DIR"
}
trap cleanup EXIT INT TERM

for src in blake3.c blake3_dispatch.c blake3_portable.c; do
    "$CC" -O2 -Wall $B3_FLAGS -c "$B3_DIR/$src" \
        -o "$B3_OBJ_DIR/${src%.c}.o"
done

"$CXX" -std=c++17 -O2 -Wall -Wextra -Werror \
    -I"$WORKSPACE/implementations/cpp/sugar-lift-cpp-source/include" \
    -I"$WORKSPACE/implementations/cpp/sugar-ir-symbolic/include" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$LLVM_INC" \
    -I"$B3_DIR" \
    "$B3_OBJ_DIR/blake3.o" \
    "$B3_OBJ_DIR/blake3_dispatch.o" \
    "$B3_OBJ_DIR/blake3_portable.o" \
    "$WORKSPACE/implementations/cpp/sugar-lift-cpp-source/src/cpp_source_lifter.cpp" \
    "$WORKSPACE/implementations/cpp/sugar-lift-cpp-source/src/main.cpp" \
    "$WORKSPACE/implementations/cpp/sugar/canonicalizer/jcs.cpp" \
    "$WORKSPACE/implementations/cpp/sugar/canonicalizer/hash.cpp" \
    -L"$LLVM_LIB" -Wl,-rpath,"$LLVM_LIB" -lclang \
    -o "$OUT_BIN"

echo "built: $OUT_BIN"
