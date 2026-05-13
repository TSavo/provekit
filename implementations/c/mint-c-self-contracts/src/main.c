/* SPDX-License-Identifier: Apache-2.0 */
/*
 * mint-c-self-contracts entry point.
 *
 * Two modes:
 *   1. Standalone:  ./mint-c-self-contracts [outDir]
 *                   Mints once, prints catalog CID + contractSetCid,
 *                   asserts byte-determinism by minting twice into
 *                   distinct dirs.
 *
 *   2. --rpc:       ./mint-c-self-contracts --rpc
 *                   Speaks the lift-plugin protocol (pep/1.7.0)
 *                   over NDJSON on stdio. The Rust CLI dispatcher
 *                   (`provekit mint --kit=c`) uses this mode; spec is
 *                   protocol/specs/2026-04-30-lift-plugin-protocol.md.
 *
 * Authoring surface: c-self-contracts. Slabs walked:
 *   - blake3              5 contracts
 *   - cbor                5 contracts
 *   - claim_envelope      5 contracts
 *   - ed25519             5 contracts
 *   - jcs                 5 contracts
 *   - proof_envelope      5 contracts
 */

#include "orchestrator.h"
#include "rpc.h"

#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    /* Detect --rpc anywhere on argv (mirrors swift/go peers). */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--rpc") == 0) {
            return mcsc_run_rpc();
        }
    }
    const char *out_dir = (argc >= 2) ? argv[1] : ".";
    return mcsc_run_cli(out_dir);
}
