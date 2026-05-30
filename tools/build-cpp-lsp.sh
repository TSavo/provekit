#!/bin/sh
# Build the C++ LSP plugin binary (provekit-lsp-cpp).
#
# Prereqs: no system BLAKE3 dependency; this uses the vendored portable C
# implementation under tools/blake3-vendored.
#
# Usage:
#   tools/build-cpp-lsp.sh             # build to implementations/cpp/target/
#   tools/build-cpp-lsp.sh --out /path # override output path

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"
SRC="$WORKSPACE/implementations/cpp/provekit-lsp-cpp/main.cpp"
B3_DIR="$WORKSPACE/tools/blake3-vendored"
B3_FLAGS="-DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0"

OUT_DIR="$WORKSPACE/implementations/cpp/target"
mkdir -p "$OUT_DIR"
OUT_BIN="$OUT_DIR/provekit-lsp-cpp"

if [ "${1:-}" = "--out" ] && [ -n "${2:-}" ]; then
    OUT_BIN="$2"
    mkdir -p "$(dirname "$OUT_BIN")"
fi

CC="${CC:-clang}"
CXX="${CXX:-clang++}"

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
    -I"$KIT_INC" \
    -I"$WORKSPACE/implementations/cpp" \
    -I"$B3_DIR" \
    "$B3_OBJ_DIR/blake3.o" \
    "$B3_OBJ_DIR/blake3_dispatch.o" \
    "$B3_OBJ_DIR/blake3_portable.o" \
    "$WORKSPACE/implementations/cpp/provekit/canonicalizer/hash.cpp" \
    "$SRC" \
    -o "$OUT_BIN"

echo "built: $OUT_BIN"
