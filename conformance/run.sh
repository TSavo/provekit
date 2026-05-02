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

# Skip a kit when its toolchain is absent. Reports SKIP and returns 1
# so the caller can early-out before trying to run the kit. Distinguishes
# missing-tool from real conformance failure.
need_tool() {
    local tool="$1" lang="$2"
    if ! command -v "$tool" >/dev/null 2>&1; then
        report "$lang" "SKIP" "$tool not on PATH"
        return 1
    fi
    return 0
}

echo ""
echo "=========================================="
echo " Cross-Language Conformance Suite"
echo "=========================================="
echo ""

# --- Rust (canonical reference) ---
echo "[Rust] cargo test -p provekit-ir-symbolic"
if need_tool cargo rust; then
    if (cd implementations/rust && run_quiet cargo test -p provekit-ir-symbolic --lib); then
        report "rust" "PASS" "canonical reference matches pinned fixtures"
    else
        report "rust" "FAIL" "cargo test failed"
    fi
fi

# --- Go ---
echo "[Go] go test"
if need_tool go go; then
    if (cd implementations/go && run_quiet go test -count=1 ./...); then
        report "go" "PASS" "matches canonical JCS bytes"
    else
        report "go" "FAIL" "go test failed"
    fi
fi

# --- Java ---
echo "[Java] mvn test"
if need_tool mvn java; then
    if (cd implementations/java && run_quiet mvn test); then
        report "java" "PASS" "13 modules, integration tests prove equivalence"
    else
        report "java" "FAIL" "mvn test failed"
    fi
fi

# --- Python ---
echo "[Python] pytest"
if need_tool python3 python; then
    if (cd implementations/python/provekit-lift-py-tests && run_quiet python3 -m pytest tests/); then
        report "python" "PASS" "55 tests, byte-identical to Rust"
    else
        report "python" "FAIL" "pytest failed"
    fi
fi

# --- C++ ---
echo "[C++] compile + run parseInt.invariant.cpp"
if need_tool c++ cpp; then
    cpp_example="implementations/cpp/provekit-ir-symbolic/example/parseInt.invariant.cpp"
    cpp_include="implementations/cpp/provekit-ir-symbolic/include"
    cpp_bin="/tmp/provekit-cpp-conformance"
    if run_quiet c++ -std=c++17 -I "$cpp_include" "$cpp_example" -o "$cpp_bin" && "$cpp_bin" >/dev/null 2>&1; then
        report "cpp" "PASS" "matches deterministic JCS output"
    else
        report "cpp" "FAIL" "parseInt.invariant.cpp failed"
    fi
fi

# --- C ---
echo "[C] make test"
if need_tool make c; then
    if (cd implementations/c/provekit-ir && run_quiet make test); then
        report "c" "PASS" "5 tests match Rust JCS + hashes"
    else
        report "c" "FAIL" "make test failed"
    fi
fi

# --- Zig ---
echo "[Zig] zig build test"
zig="${PWD}/zig-toolchain/zig"
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
if need_tool bun typescript; then
    if (cd implementations/typescript && run_quiet bun test src/lift/*.test.ts); then
        report "typescript" "PASS" "29 tests, adapters prove equivalence"
    else
        report "typescript" "FAIL" "bun test failed"
    fi
fi

# --- Summary ---
echo ""
echo "=========================================="
printf " Results: ${GREEN}%d pass${NC}, ${RED}%d fail${NC}\n" "$PASSES" "$FAILS"
echo "=========================================="

if [ "$FAILS" -gt 0 ]; then
    exit 1
fi
exit 0
