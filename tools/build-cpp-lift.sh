#!/bin/sh
# Build the C++ lift plugin binary (provekit-lift-cpp).

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"
SRC="$WORKSPACE/implementations/cpp/provekit-lift-cpp/main.cpp"

OUT_DIR="$WORKSPACE/implementations/cpp/target"
mkdir -p "$OUT_DIR"
OUT_BIN="$OUT_DIR/provekit-lift-cpp"

if [ "${1:-}" = "--out" ] && [ -n "${2:-}" ]; then
    OUT_BIN="$2"
    mkdir -p "$(dirname "$OUT_BIN")"
fi

CXX="${CXX:-clang++}"

"$CXX" -std=c++17 -O2 -Wall -Wextra -Werror \
    -I"$KIT_INC" \
    -I"$WORKSPACE/implementations/cpp" \
    "$SRC" \
    -o "$OUT_BIN"

echo "built: $OUT_BIN"
