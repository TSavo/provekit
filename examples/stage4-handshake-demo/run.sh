#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Stage 4 cross-language handshake demo.
#
# Four scripted runs that put numbers on the trojan-horse pitch:
#
#   Run A: Tier 1 hit (post == pre by hash equality).
#   Run B: Tier 1 miss, Tier 3 fires once (Z3 unsat -> mints memento),
#          then Tier 2 cache hit on warm replay.
#   Run C: Tier 1 miss, Tier 2 miss (different antecedent hash),
#          Tier 3 fires (Z3 sat -> violation flagged). Demonstrates
#          content-addressed cache "invalidation".
#   Run D: Tier 2 cache hit again once Go restores the antecedent
#          shape that Run B's memento covers.
#
# All publishers cross-language: Go publishes the post, Rust publishes
# the pre, Rust verifier reconciles. The protocol bytes are the only
# thing crossing the language boundary.
#
# Usage:
#   bash examples/stage4-handshake-demo/run.sh
#
# Optional env:
#   PROVEKIT_Z3 — path to the z3 binary (default: z3 on $PATH).
#   STAGE4_KEEP — set to 1 to keep the per-run project_dirs.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEMO_DIR="${REPO_ROOT}/examples/stage4-handshake-demo"
RUST_DIR="${REPO_ROOT}/implementations/rust"
GO_DIR="${DEMO_DIR}/go-validate-kit"

WORK_DIR="${TMPDIR:-/tmp}/provekit-stage4-$(date +%s)"
mkdir -p "${WORK_DIR}"

# A single project_dir reused across all four runs so the cache
# directory persists. Each run gets a fresh subdirectory for proofs
# but shares .provekit/cache/.
PROJECT_DIR="${WORK_DIR}/project"
mkdir -p "${PROJECT_DIR}"
mkdir -p "${PROJECT_DIR}/.provekit/cache"

# Per-run scratch dirs for Go publisher output.
mkdir -p "${WORK_DIR}/go-A" "${WORK_DIR}/go-B" "${WORK_DIR}/go-C" "${WORK_DIR}/go-D"

export PATH="${HOME}/.cargo/bin:${PATH}"
export PROVEKIT_Z3="${PROVEKIT_Z3:-z3}"

if ! command -v "${PROVEKIT_Z3}" >/dev/null 2>&1; then
    echo "ERROR: z3 not found at PROVEKIT_Z3=${PROVEKIT_Z3}" >&2
    exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: cargo not found on PATH" >&2
    exit 1
fi
if ! command -v go >/dev/null 2>&1; then
    echo "ERROR: go not found on PATH" >&2
    exit 1
fi

# -------------------- Build phase --------------------

echo
echo "=========================================================="
echo "  Stage 4 handshake demo"
echo "=========================================================="
echo "  work dir: ${WORK_DIR}"
echo "  project dir: ${PROJECT_DIR}"
echo "  z3: $(${PROVEKIT_Z3} --version 2>/dev/null | head -1)"
echo

echo "[build] cargo build --example stage4_driver --release"
( cd "${RUST_DIR}" && cargo build --example stage4_driver --release >/dev/null 2>&1 )
DRIVER="${RUST_DIR}/target/release/examples/stage4_driver"
if [[ ! -x "${DRIVER}" ]]; then
    echo "ERROR: stage4_driver not built at ${DRIVER}" >&2
    exit 1
fi

echo "[build] go build for go-validate-kit"
( cd "${GO_DIR}" && go build -o "${WORK_DIR}/go-validate-kit-bin" . )
GO_BIN="${WORK_DIR}/go-validate-kit-bin"

# -------------------- Helper: extract STAGE4_SUMMARY --------------------

# Runs the driver. Streams full output to stderr (so the user sees
# it inline) and the file `logfile`, then writes the matching
# STAGE4_SUMMARY line to stdout (consumed by $(...) capture).
run_driver() {
    local label="$1"
    local go_proof="$2"
    local proj="$3"
    local logfile="$4"

    "${DRIVER}" \
        --label "${label}" \
        --go-proof "${go_proof}" \
        --project-dir "${proj}" \
        --print-cids \
        >"${logfile}" 2>&1
    cat "${logfile}" >&2
    grep '^STAGE4_SUMMARY ' "${logfile}" | tail -n1
}

# -------------------- Run A: Tier 1 hash equality --------------------
echo
echo "=========================================================="
echo "  Run A: Tier 1 (hash equality, no Z3)"
echo "=========================================================="

PROJECT_A="${WORK_DIR}/proj-A"
mkdir -p "${PROJECT_A}"
ln -sfn "${PROJECT_DIR}/.provekit" "${PROJECT_A}/.provekit"

"${GO_BIN}" --shape gt0 --out "${WORK_DIR}/go-A" >/dev/null
GO_PROOF_A="$(ls "${WORK_DIR}/go-A"/*.proof | head -1)"
SUMMARY_A=$(run_driver "Run A: hash equality" "${GO_PROOF_A}" "${PROJECT_A}" "${WORK_DIR}/log-A.txt")

# -------------------- Run B (cold): Tier 3 mint --------------------
echo
echo "=========================================================="
echo "  Run B (cold): Tier 1 miss, Tier 2 miss, Tier 3 mints"
echo "=========================================================="

PROJECT_B="${WORK_DIR}/proj-B"
mkdir -p "${PROJECT_B}"
ln -sfn "${PROJECT_DIR}/.provekit" "${PROJECT_B}/.provekit"

"${GO_BIN}" --shape gte1 --out "${WORK_DIR}/go-B" >/dev/null
GO_PROOF_B="$(ls "${WORK_DIR}/go-B"/*.proof | head -1)"
SUMMARY_B_COLD=$(run_driver "Run B (cold)" "${GO_PROOF_B}" "${PROJECT_B}" "${WORK_DIR}/log-B-cold.txt")

# -------------------- Run B (warm): Tier 2 cache hit --------------------
echo
echo "=========================================================="
echo "  Run B (warm): same publisher, Tier 2 cache hit"
echo "=========================================================="

PROJECT_B2="${WORK_DIR}/proj-B-warm"
mkdir -p "${PROJECT_B2}"
ln -sfn "${PROJECT_DIR}/.provekit" "${PROJECT_B2}/.provekit"

# Re-run with same Go publisher; Tier 2 should hit because .provekit/cache/
# already contains the implication memento minted in the cold run.
SUMMARY_B_WARM=$(run_driver "Run B (warm)" "${GO_PROOF_B}" "${PROJECT_B2}" "${WORK_DIR}/log-B-warm.txt")

# -------------------- Run C: cache invalidation by content addressing --------------------
echo
echo "=========================================================="
echo "  Run C: Go's post is now 'n >= 0'. Hash changed; cache miss."
echo "=========================================================="

PROJECT_C="${WORK_DIR}/proj-C"
mkdir -p "${PROJECT_C}"
ln -sfn "${PROJECT_DIR}/.provekit" "${PROJECT_C}/.provekit"

"${GO_BIN}" --shape gte0 --out "${WORK_DIR}/go-C" >/dev/null
GO_PROOF_C="$(ls "${WORK_DIR}/go-C"/*.proof | head -1)"
SUMMARY_C=$(run_driver "Run C: violation flagged" "${GO_PROOF_C}" "${PROJECT_C}" "${WORK_DIR}/log-C.txt")

# -------------------- Run D: re-cache after fix --------------------
echo
echo "=========================================================="
echo "  Run D: Go restores 'n >= 1'; old memento re-indexes"
echo "=========================================================="

PROJECT_D="${WORK_DIR}/proj-D"
mkdir -p "${PROJECT_D}"
ln -sfn "${PROJECT_DIR}/.provekit" "${PROJECT_D}/.provekit"

"${GO_BIN}" --shape gte1 --out "${WORK_DIR}/go-D" >/dev/null
GO_PROOF_D="$(ls "${WORK_DIR}/go-D"/*.proof | head -1)"
SUMMARY_D=$(run_driver "Run D: re-cache" "${GO_PROOF_D}" "${PROJECT_D}" "${WORK_DIR}/log-D.txt")

# -------------------- Headline summary --------------------
echo
echo "=========================================================="
echo "  HEADLINE METRICS"
echo "=========================================================="
echo "  format: hash=M cache=K z3+mint=L residue=J violations=V z3_invocations=Z"
echo
echo "  Run A (hash equality):  ${SUMMARY_A##STAGE4_SUMMARY }"
echo "  Run B (cold, mint):     ${SUMMARY_B_COLD##STAGE4_SUMMARY }"
echo "  Run B (warm, cached):   ${SUMMARY_B_WARM##STAGE4_SUMMARY }"
echo "  Run C (violation):      ${SUMMARY_C##STAGE4_SUMMARY }"
echo "  Run D (re-cached):      ${SUMMARY_D##STAGE4_SUMMARY }"
echo
echo "  cache contents (.provekit/cache):"
ls -la "${PROJECT_DIR}/.provekit/cache/" 2>/dev/null | awk 'NR>1 {print "    " $0}'
echo
echo "  artifacts kept under: ${WORK_DIR}"
if [[ "${STAGE4_KEEP:-0}" != "1" ]]; then
    echo "  (set STAGE4_KEEP=1 to retain on disk)"
fi
