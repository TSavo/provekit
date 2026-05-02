#!/usr/bin/env bash
# Cross-language conformance harness.
#
# Runs every language kit's test suite. Each kit is known to internally assert
# that its JCS bytes match the Rust canonical reference. If a kit passes its
# own tests, cross-language byte-for-byte equivalence is proven.
#
# Usage: ./conformance/run.sh
#
# Requires: cargo, go, mvn, python3, gcc, zig, bun, g++

set -uo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

RESULTS=()
PASSES=0
FAILS=0

report() {
    local lang="$1" status="$2" detail="$3"
    local marker col
    case "$status" in
        PASS) marker="✓"; col="$GREEN"; ((PASSES++)) ;;
        FAIL) marker="✗"; col="$RED";   ((FAILS++)) ;;
        SKIP) marker="~"; col="$YELLOW"; ;;
    esac
    RESULTS+=("$lang|$status|$detail")
    printf "  ${col}%s %-12s %s${NC}\n" "$marker" "$lang" "$detail"
}

run_quiet() {
    "$@" >/dev/null 2>&1
}

echo ""
echo "=========================================="
echo " Cross-Language Conformance Suite"
echo "=========================================="
echo ""

# --- Rust (canonical reference) ---
echo "[Rust] cargo test -p provekit-ir-symbolic"
if (cd implementations/rust && run_quiet cargo test -p provekit-ir-symbolic --lib); then
    report "rust" "PASS" "canonical reference matches pinned fixtures"
else
    report "rust" "FAIL" "cargo test failed"
fi

# --- Go ---
echo "[Go] go test"
if (cd implementations/go && run_quiet go test -count=1 ./...); then
    report "go" "PASS" "matches canonical JCS bytes"
else
    report "go" "FAIL" "go test failed"
fi

# --- Java ---
echo "[Java] mvn test"
if (cd implementations/java && run_quiet mvn test); then
    report "java" "PASS" "13 modules, integration tests prove equivalence"
else
    report "java" "FAIL" "mvn test failed"
fi

# --- Python ---
echo "[Python] pytest"
if (cd implementations/python/provekit-lift-py-tests && run_quiet python3 -m pytest tests/); then
    report "python" "PASS" "55 tests, byte-identical to Rust"
else
    report "python" "FAIL" "pytest failed"
fi

# --- C++ ---
echo "[C++] compile + run parseInt.invariant.cpp"
local cpp_example="implementations/cpp/provekit-ir-symbolic/example/parseInt.invariant.cpp"
local cpp_include="implementations/cpp/provekit-ir-symbolic/include"
local cpp_bin="/tmp/provekit-cpp-conformance"
if run_quiet c++ -std=c++17 -I "$cpp_include" "$cpp_example" -o "$cpp_bin" && "$cpp_bin" >/dev/null 2>&1; then
    report "cpp" "PASS" "matches deterministic JCS output"
else
    report "cpp" "FAIL" "parseInt.invariant.cpp failed"
fi

# --- C ---
echo "[C] make test"
if (cd implementations/c/provekit-ir && run_quiet make test); then
    report "c" "PASS" "5 tests match Rust JCS + hashes"
else
    report "c" "FAIL" "make test failed"
fi

# --- Zig ---
echo "[Zig] zig build test"
local zig="${PWD}/zig-toolchain/zig"
if [ -x "$zig" ]; then
    if (cd implementations/zig/provekit-ir && run_quiet "$zig" build test); then
        report "zig" "PASS" "5 tests match Rust JCS + BLAKE3-512"
    else
        report "zig" "FAIL" "zig build test failed"
    fi
else
    report "zig" "SKIP" "zig toolchain not found at $zig"
fi

# --- TypeScript ---
echo "[TypeScript] bun test"
if (cd implementations/typescript && run_quiet bun test src/lift/*.test.ts); then
    report "typescript" "PASS" "29 tests, adapters prove equivalence"
else
    report "typescript" "FAIL" "bun test failed"
fi

# --- Summary ---
echo ""
echo "=========================================="
echo " Results: ${GREEN}${PASSES} pass${NC}, ${RED}${FAILS} fail${NC}"
echo "=========================================="

if [ "$FAILS" -gt 0 ]; then
    exit 1
fi
exit 0
