#!/usr/bin/env bash
# Build the Java-native test assertion lifter.
# Requires JDK 21+ (uses com.sun.source compiler tree API).
# Output: out/ directory with JavaTestAssertionsRpc.class
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:-$HERE/out}"

mkdir -p "$OUT"

# com.sun.source is in the jdk.compiler module. We compile without --release
# because --add-exports is incompatible with --release for system modules.
# The code is written to JDK 21 language level; the actual compiler on PATH
# must be JDK 21+ (both battleaxe/21 and Mac/25 qualify).
javac \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED \
  -source 21 -target 21 \
  -d "$OUT" \
  "$HERE/src/JavaTestAssertionsRpc.java"

echo "Built to $OUT"
