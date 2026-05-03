#!/bin/sh
# Build the C++ LSP plugin binary (provekit-lsp-cpp).
#
# Prereqs: same as build-cpp-self-contracts.sh — no extra deps needed beyond
# the provekit/ir.hpp header-only library (already in the repo).
#
# Usage:
#   tools/build-cpp-lsp.sh             # build to implementations/cpp/target/
#   tools/build-cpp-lsp.sh --out /path # override output path

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
KIT_INC="$WORKSPACE/implementations/cpp/provekit-ir-symbolic/include"
SRC="$WORKSPACE/implementations/cpp/provekit-lsp-cpp/main.cpp"

OUT_DIR="$WORKSPACE/implementations/cpp/target"
mkdir -p "$OUT_DIR"
OUT_BIN="$OUT_DIR/provekit-lsp-cpp"

if [ "${1:-}" = "--out" ] && [ -n "${2:-}" ]; then
    OUT_BIN="$2"
fi

CXX="${CXX:-clang++}"

"$CXX" -std=c++17 -O2 -Wall -Wextra -Werror \
    -I"$KIT_INC" \
    -I"$WORKSPACE/implementations/cpp" \
    "$SRC" \
    -o "$OUT_BIN"

echo "built: $OUT_BIN"
