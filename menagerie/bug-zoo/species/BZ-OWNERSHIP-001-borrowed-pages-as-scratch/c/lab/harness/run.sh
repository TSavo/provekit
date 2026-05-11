#!/usr/bin/env bash
# BZ-OWNERSHIP-001: borrowed-pages-as-scratch — lab harness
#
# Compiles the buggy and fixed implementations, then runs the lab scenario
# to confirm:
#   - buggy: process_borrowed_buf clobbers the caller's buffer
#   - fixed: process_buf_to_dst leaves src intact
#
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
lib_dir="$(cd "$script_dir/../library/src" && pwd)"
out_dir="$script_dir/.build"
rm -rf "$out_dir"
mkdir -p "$out_dir"

# ---------------------------------------------------------------------------
# Compile
# ---------------------------------------------------------------------------
cc -std=c11 -Wall -Wextra -o "$out_dir/lab_buggy" \
    "$lib_dir/borrow_contract.c" \
    "$script_dir/harness_main.c"

cc -std=c11 -Wall -Wextra -o "$out_dir/lab_fixed" \
    "$(cd "$script_dir/../.." && pwd)/fixed/c-assertions/harness/src/borrow_contract_fixed.c" \
    "$script_dir/harness_fixed_main.c"

# ---------------------------------------------------------------------------
# Run buggy: caller's buffer should be clobbered (exit 0 means compile/run OK)
# ---------------------------------------------------------------------------
"$out_dir/lab_buggy"

# ---------------------------------------------------------------------------
# Run fixed: caller's buffer is preserved
# ---------------------------------------------------------------------------
"$out_dir/lab_fixed"
