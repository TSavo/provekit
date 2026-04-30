#!/usr/bin/env bash
# Cross-language equivalence regression harness.
#
# For each fixture in fixtures.txt, runs the TS / Rust / Go / C++ runners
# and asserts they emit byte-identical compact JSON. Also asserts the
# SHA256 matches the golden value in goldens.txt (catches any kit drifting
# in a way that all kits drift to — i.e. a canonical-form change).
#
# Exit 0: all fixtures pass.
# Exit 1: at least one fixture diverges across kits OR drifts from golden.
# Exit 2: build failure or runner crash.

set -uo pipefail

# Re-exec under bash 4+ if available (macOS ships bash 3.2 which lacks
# associative arrays). Falls back to grep-based golden lookup either way.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Locate cargo (prefer system path, fall back to per-user install).
if ! command -v cargo >/dev/null 2>&1; then
  if [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  else
    echo "[harness] cargo not found — install rust toolchain to run this gate" >&2
    exit 2
  fi
fi

# Build runners once (cached on subsequent invocations).
echo "[harness] building rust runner..."
(cd rust-runner && cargo build --release --quiet) || { echo "rust build failed"; exit 2; }
RUST_BIN="$SCRIPT_DIR/rust-runner/target/release/cross-lang-runner"

echo "[harness] building cpp runner..."
clang++ -std=c++17 -O2 \
  -I "$SCRIPT_DIR/../../kits/cpp/provekit-ir-symbolic/include" \
  "$SCRIPT_DIR/cpp-runner/main.cpp" \
  -o "$SCRIPT_DIR/cpp-runner/cross-lang-runner" \
  || { echo "cpp build failed"; exit 2; }
CPP_BIN="$SCRIPT_DIR/cpp-runner/cross-lang-runner"

lookup_golden() {
  local needle="$1"
  [ -f "$SCRIPT_DIR/goldens.txt" ] || return 1
  awk -F'\t' -v fx="$needle" '$1 == fx { print $2; exit }' "$SCRIPT_DIR/goldens.txt"
}

# Iterate fixtures.
fail_count=0
pass_count=0
while IFS= read -r fixture; do
  [ -z "$fixture" ] && continue
  [ "${fixture#\#}" != "$fixture" ] && continue

  ts_out=$(npx tsx "$SCRIPT_DIR/ts-runner.ts" "$fixture") || { echo "[$fixture] TS crashed"; fail_count=$((fail_count+1)); continue; }
  rust_out=$("$RUST_BIN" "$fixture") || { echo "[$fixture] Rust crashed"; fail_count=$((fail_count+1)); continue; }
  go_out=$(cd "$SCRIPT_DIR/go-runner" && go run main.go "$fixture") || { echo "[$fixture] Go crashed"; fail_count=$((fail_count+1)); continue; }
  cpp_out=$("$CPP_BIN" "$fixture") || { echo "[$fixture] C++ crashed"; fail_count=$((fail_count+1)); continue; }

  if [ "$ts_out" != "$rust_out" ] || [ "$ts_out" != "$go_out" ] || [ "$ts_out" != "$cpp_out" ]; then
    echo "[$fixture] DIVERGE — outputs differ across kits"
    echo "  ts:   $ts_out"
    echo "  rust: $rust_out"
    echo "  go:   $go_out"
    echo "  cpp:  $cpp_out"
    fail_count=$((fail_count+1))
    continue
  fi

  sha=$(printf '%s' "$ts_out" | shasum -a 256 | awk '{print $1}')
  golden=$(lookup_golden "$fixture" || true)
  if [ -n "$golden" ] && [ "$sha" != "$golden" ]; then
    echo "[$fixture] DRIFT — kits agree but diverged from golden"
    echo "  expected: $golden"
    echo "  actual:   $sha"
    fail_count=$((fail_count+1))
    continue
  fi

  if [ -z "$golden" ]; then
    echo "[$fixture] PASS sha256=$sha (no golden — add to goldens.txt to lock)"
  else
    echo "[$fixture] PASS sha256=$sha"
  fi
  pass_count=$((pass_count+1))
done < "$SCRIPT_DIR/fixtures.txt"

echo
echo "[harness] $pass_count passed, $fail_count failed"
[ "$fail_count" -eq 0 ] || exit 1
exit 0
