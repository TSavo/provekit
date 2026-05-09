#!/usr/bin/env bash
# BZ-COMPOSITION-001 cross-language equivalence runner.
#
# Lifts the Rust chain via the appropriate Rust lifter, lifts the C chain
# via the appropriate C lifter, calls the canonical compose_chain_contracts
# (CCP section 5) for each side, and asserts that the resulting
# ComposedFunctionContract CIDs are byte-identical (CCP section 7).
#
# Exit codes:
#   0  EQUAL        both sides composed and CIDs match
#   1  DIVERGENT    both sides composed and CIDs differ
#   2  PENDING      one side is not yet wired (acceptable v0 state)
#   3  ERROR        a tool invocation or environment precondition failed

set -u
set -o pipefail

SPECIES_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_DIR="${SPECIES_DIR}/lab/rust"
C_DIR="${SPECIES_DIR}/lab/c"

REPO_ROOT="$(cd "${SPECIES_DIR}/../../../.." && pwd)"

PROVEKIT_BIN="${PROVEKIT_BIN:-${REPO_ROOT}/target/release/provekit}"
RUST_LIFTER="${RUST_LIFTER:-provekit-walk}"
C_LIFTER="${C_LIFTER:-provekit-lift-c-kernel-doc}"

emit_pending() {
    local reason="$1"
    printf 'rust composed cid: %s\n' "${RUST_CID:-<unavailable>}"
    printf 'c    composed cid: %s\n' "${C_CID:-<unavailable>}"
    printf 'verdict: PENDING\n'
    printf 'reason: %s\n' "${reason}"
    exit 2
}

emit_error() {
    local reason="$1"
    printf 'verdict: ERROR\n' >&2
    printf 'reason: %s\n' "${reason}" >&2
    exit 3
}

# 1. Lift the Rust source.
#
# The intended call shape (subject to provekit-walk CLI evolution):
#
#   ${PROVEKIT_BIN} lift --lifter "${RUST_LIFTER}" \
#       --source "${RUST_DIR}/src/lib.rs" \
#       --emit-composed vec_double_then_filter_positive_then_sum \
#       --out "${SPECIES_DIR}/.out/rust-composed.json"
#
# The composed CID is the top-level "cid" field of the emitted document.

lift_rust() {
    if [[ ! -x "${PROVEKIT_BIN}" ]]; then
        return 10
    fi
    if ! "${PROVEKIT_BIN}" --help 2>/dev/null | grep -q '^[[:space:]]*lift'; then
        return 11
    fi
    mkdir -p "${SPECIES_DIR}/.out"
    "${PROVEKIT_BIN}" lift \
        --lifter "${RUST_LIFTER}" \
        --source "${RUST_DIR}/src/lib.rs" \
        --emit-composed vec_double_then_filter_positive_then_sum \
        --out "${SPECIES_DIR}/.out/rust-composed.json" \
        >/dev/null 2>&1 || return 12
    RUST_CID="$(grep -o '"cid"[[:space:]]*:[[:space:]]*"[^"]*"' \
        "${SPECIES_DIR}/.out/rust-composed.json" | head -n 1 \
        | sed 's/.*"\(blake3-[^"]*\)".*/\1/')"
    [[ -n "${RUST_CID}" ]] || return 13
    return 0
}

# 2. Lift the C source.
#
# CCP section 6.2 / 6.3: the C lifter family must be able to emit composed
# contracts via the FFI binding (libprovekit-c) or the JSON-RPC subprocess
# binding. Until either landing path is wired, the C side stubs out and the
# runner emits PENDING.

lift_c() {
    if [[ ! -x "${PROVEKIT_BIN}" ]]; then
        return 20
    fi
    if ! "${PROVEKIT_BIN}" lift --help 2>/dev/null | grep -q -- "--lifter[[:space:]]*${C_LIFTER}\\|^[[:space:]]*${C_LIFTER}"; then
        return 21
    fi
    mkdir -p "${SPECIES_DIR}/.out"
    "${PROVEKIT_BIN}" lift \
        --lifter "${C_LIFTER}" \
        --source "${C_DIR}/chain.c" \
        --header "${C_DIR}/chain.h" \
        --emit-composed bz_vec_double_then_filter_positive_then_sum \
        --out "${SPECIES_DIR}/.out/c-composed.json" \
        >/dev/null 2>&1 || return 22
    C_CID="$(grep -o '"cid"[[:space:]]*:[[:space:]]*"[^"]*"' \
        "${SPECIES_DIR}/.out/c-composed.json" | head -n 1 \
        | sed 's/.*"\(blake3-[^"]*\)".*/\1/')"
    [[ -n "${C_CID}" ]] || return 23
    return 0
}

RUST_CID=""
C_CID=""

lift_rust
RUST_RC=$?
case "${RUST_RC}" in
    0) ;;
    10) emit_pending "Rust lift skipped: provekit binary not built at ${PROVEKIT_BIN}" ;;
    11) emit_pending "Rust lift skipped: provekit binary lacks 'lift' subcommand" ;;
    12) emit_error  "Rust lift failed during '${PROVEKIT_BIN} lift'" ;;
    13) emit_error  "Rust lift produced no composed CID" ;;
    *)  emit_error  "Rust lift returned unexpected status ${RUST_RC}" ;;
esac

lift_c
C_RC=$?
case "${C_RC}" in
    0) ;;
    20) emit_pending "C ABI FFI not yet wired: provekit binary not built at ${PROVEKIT_BIN}" ;;
    21) emit_pending "C ABI FFI not yet wired: lifter '${C_LIFTER}' unknown to provekit lift" ;;
    22) emit_error   "C lift failed during '${PROVEKIT_BIN} lift --lifter ${C_LIFTER}'" ;;
    23) emit_error   "C lift produced no composed CID" ;;
    *)  emit_error   "C lift returned unexpected status ${C_RC}" ;;
esac

printf 'rust composed cid: %s\n' "${RUST_CID}"
printf 'c    composed cid: %s\n' "${C_CID}"

if [[ "${RUST_CID}" == "${C_CID}" ]]; then
    printf 'verdict: EQUAL\n'
    exit 0
else
    printf 'verdict: DIVERGENT\n'
    exit 1
fi
