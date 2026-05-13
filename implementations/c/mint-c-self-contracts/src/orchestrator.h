/* SPDX-License-Identifier: Apache-2.0 */
#ifndef MCSC_ORCHESTRATOR_H
#define MCSC_ORCHESTRATOR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * One mint run's result. All `char *` fields are owned by the caller and
 * must be freed via mcsc_mint_result_free; `bytes` must be freed via
 * mcsc_mint_result_free as well.
 */
typedef struct mcsc_mint_result {
    char    *cid;             /* "blake3-512:..." catalog filename CID */
    char    *contract_set_cid; /* "blake3-512:..." */
    size_t   contract_count;
    size_t   slab_count;
    uint8_t *bytes;            /* CBOR proof-envelope bytes */
    size_t   bytes_len;
} mcsc_mint_result;

void mcsc_mint_result_init(mcsc_mint_result *r);
void mcsc_mint_result_free(mcsc_mint_result *r);

/*
 * Author every slab in JavaKitInvariants C peer, JCS-encode each
 * contract's {name, outBinding, pre?, post?, inv?} body, hash for
 * contentCid, build the proof-envelope catalog, write
 * <out_dir>/<cid>.proof. On success returns 0 and populates *out;
 * caller owns and must free *out via mcsc_mint_result_free. If
 * `out_dir` is NULL the function does NOT write a file (RPC mode);
 * the caller still receives bytes + cid for the wire response.
 */
int mcsc_mint_one_run(const char *out_dir, mcsc_mint_result *out);

/* CLI entry point: writes to outDir, prints a human-readable report,
 * asserts byte-determinism by minting again into a sibling dir.
 * Returns process exit code. */
int mcsc_run_cli(const char *out_dir);

#ifdef __cplusplus
}
#endif

#endif /* MCSC_ORCHESTRATOR_H */
