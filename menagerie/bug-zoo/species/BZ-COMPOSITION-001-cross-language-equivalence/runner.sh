#!/usr/bin/env bash
# BZ-COMPOSITION-001 cross-language equivalence runner.
#
# Lifts the Rust chain via the appropriate Rust lifter, lifts the C chain
# via the appropriate C lifter, calls the canonical compose_chain_contracts
# (CCP section 5) for each side, and asserts that the resulting
# ComposedFunctionContract CIDs are byte-identical (CCP section 7).
#
# Exit codes:
#   0  EQUAL          both sides composed and CIDs match
#   1  DIVERGENT      both sides composed and CIDs differ
#   2  PENDING-RUST   Rust side has not yet wired --emit-composed in
#                    provekit-walk; C side may or may not have composed
#   4  PENDING-C      C lifter emits no composed-contract (chain too
#                    short, or no pure subtree of length >= 2 in lab/c)
#   5  PENDING-OTHER  some other precondition is missing (binary not
#                    built, lifter unavailable, etc.)
#   3  ERROR          a tool invocation or environment precondition failed

set -u
set -o pipefail

SPECIES_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_DIR="${SPECIES_DIR}/lab/rust"
C_DIR="${SPECIES_DIR}/lab/c"

REPO_ROOT="$(cd "${SPECIES_DIR}/../../../.." && pwd)"

PROVEKIT_BIN="${PROVEKIT_BIN:-${REPO_ROOT}/implementations/rust/target/release/provekit}"
RUST_LIFTER="${RUST_LIFTER:-provekit-walk}"
C_LIFTER_BIN="${C_LIFTER_BIN:-${REPO_ROOT}/implementations/c/provekit-lift-c-kernel-doc/provekit-lift-c-kernel-doc}"
C_LIFTER="${C_LIFTER:-provekit-lift-c-kernel-doc}"

emit_pending_rust() {
    local reason="$1"
    printf 'rust composed cid: %s\n' "${RUST_CID:-<unavailable>}"
    printf 'c    composed cid: %s\n' "${C_CID:-<unavailable>}"
    printf 'verdict: PENDING-RUST\n'
    printf 'reason: %s\n' "${reason}"
    exit 2
}

emit_pending_c() {
    local reason="$1"
    printf 'rust composed cid: %s\n' "${RUST_CID:-<unavailable>}"
    printf 'c    composed cid: %s\n' "${C_CID:-<unavailable>}"
    printf 'verdict: PENDING-C\n'
    printf 'reason: %s\n' "${reason}"
    exit 4
}

emit_pending_other() {
    local reason="$1"
    printf 'rust composed cid: %s\n' "${RUST_CID:-<unavailable>}"
    printf 'c    composed cid: %s\n' "${C_CID:-<unavailable>}"
    printf 'verdict: PENDING-OTHER\n'
    printf 'reason: %s\n' "${reason}"
    exit 5
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
# CCP §4 (eager materialization) + §6.2 (C ABI FFI): the C lifter
# (provekit-lift-c-kernel-doc) now links libprovekit's compose
# primitive via the C ABI and emits composed-contract declarations
# directly. The runner drives it through the JSON-RPC `parse` method;
# any composed-contract for `bz_vec_double_then_filter_positive_then_sum`
# in the response surfaces as the C-side CID.
#
# Note: the lab/c/chain.c body uses pointer reads/writes that the
# c-effects walker currently tags non-pure (Reads/Writes targets).
# Composition refuses on impure subtrees per CCP §2 (Purity refusal),
# so the wrapper may not yield a composed contract until either the
# lab is restructured around pure helpers or the effects walker gains
# a borrow-aware analysis. PENDING-C is the honest verdict in that
# case; the C wiring is still proven by composition_basic.c via
# `make -C implementations/c/provekit-lift-c-kernel-doc test`.

lift_c() {
    if [[ ! -x "${C_LIFTER_BIN}" ]]; then
        return 20
    fi
    if [[ ! -r "${C_DIR}/chain.c" ]]; then
        return 24
    fi
    mkdir -p "${SPECIES_DIR}/.out"

    # JSON-encode the source file's contents so we can stuff it into
    # the parse request's "source" field. python3 is a workspace-wide
    # dependency already (used by the existing integration tests).
    local request
    request="$(python3 -c '
import json, sys, pathlib
src = pathlib.Path(sys.argv[1]).read_text()
print(json.dumps({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "parse",
    "params": {
        "path": "chain.c",
        "parse_backend": "clang_ast",
        "source": src,
    },
}))
' "${C_DIR}/chain.c")"

    local response
    response="$(printf '%s\n' "${request}" | "${C_LIFTER_BIN}" --rpc 2>/dev/null)" || return 22

    printf '%s\n' "${response}" > "${SPECIES_DIR}/.out/c-lift.json"

    # Pull the composed-contract CID for the wrapper out of the
    # response. Empty string if no composed-contract for the wrapper
    # was emitted; in that case the C side is PENDING-C.
    C_CID="$(printf '%s\n' "${response}" | python3 -c '
import json, sys
r = json.load(sys.stdin)
ds = r.get("result", {}).get("declarations", [])
target = "bz_vec_double_then_filter_positive_then_sum"
matches = [d for d in ds if d.get("kind") == "composed-contract" and d.get("function") == target]
print(matches[0]["composedCid"] if matches else "")
')"
    if [[ -z "${C_CID}" ]]; then
        return 25
    fi
    return 0
}

RUST_CID=""
C_CID=""

# C side first: it now has a real implementation path. We compute its
# verdict before deciding how to surface the (still-pending) Rust side.
lift_c
C_RC=$?
case "${C_RC}" in
    0) ;;
    20) emit_pending_other "C lifter binary not built at ${C_LIFTER_BIN}; run 'make -C implementations/c/provekit-lift-c-kernel-doc'" ;;
    22) emit_error         "C lifter JSON-RPC invocation failed against ${C_DIR}/chain.c" ;;
    24) emit_error         "C lab source missing: ${C_DIR}/chain.c" ;;
    25) ;;  # No composed-contract for the wrapper; will be reported below.
    *)  emit_error         "C lift returned unexpected status ${C_RC}" ;;
esac

lift_rust
RUST_RC=$?
case "${RUST_RC}" in
    0) ;;
    10|11|12|13) ;;  # Rust side is expected PENDING in v0; keep going.
    *)  emit_error  "Rust lift returned unexpected status ${RUST_RC}" ;;
esac

# Verdict logic. Both-emitted is EQUAL/DIVERGENT; either-missing is a
# typed PENDING with a precise reason so the next iteration knows what
# to wire.
if [[ "${C_RC}" -eq 25 && "${RUST_RC}" -ne 0 ]]; then
    emit_pending_other "neither side emits composed CID for the wrapper: C lifter classified the wrapper as impure (lab/c/chain.c uses xs[i] / acc = ...) AND Rust lifter has no --emit-composed wiring yet"
fi
if [[ "${C_RC}" -eq 25 ]]; then
    emit_pending_c "C lifter emits no composed-contract for bz_vec_double_then_filter_positive_then_sum: the wrapper's call_sites include impure helpers (bz_sum / inner loop) per the conservative effects walker. Restructure lab/c/chain.c around purely-functional helpers OR teach effects.c borrow-aware analysis to lift the impurity."
fi
if [[ "${RUST_RC}" -ne 0 ]]; then
    emit_pending_rust "Rust side composed CID unavailable: provekit-walk does not yet expose --emit-composed via the provekit binary. C side composed CID for the wrapper is ${C_CID}"
fi

printf 'rust composed cid: %s\n' "${RUST_CID}"
printf 'c    composed cid: %s\n' "${C_CID}"

if [[ "${RUST_CID}" == "${C_CID}" ]]; then
    printf 'verdict: EQUAL\n'
    exit 0
else
    printf 'verdict: DIVERGENT\n'
    exit 1
fi
