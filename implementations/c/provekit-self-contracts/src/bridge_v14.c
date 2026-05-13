/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Bridge v1.4 mint — layered {envelope, header, metadata} with tagged-union target.
 *
 * Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.
 * Canonical reference: rust/provekit-claim-envelope/src/lib.rs fn mint_bridge_v14.
 */

#include "provekit/self_contracts.h"

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

/* ---- result lifecycle ---- */

void pksc_bridge_v14_result_free(pksc_bridge_v14_result *r) {
    if (r) {
        free(r->cid);
        free(r->canonical_bytes);
        r->cid = NULL;
        r->canonical_bytes = NULL;
        r->canonical_len = 0;
    }
}

/* ---- main mint ---- */

int pksc_mint_bridge_v14(pksc_bridge_v14_result *out,
                         const uint8_t seed[PKSC_ED25519_SEED_LEN],
                         const char *declared_at,
                         const char *name,
                         const char *source_symbol,
                         const char *source_layer,
                         const char *source_contract_cid,
                         const char *target_kind,
                         const char *target_cid,
                         const char **metadata_keys,
                         const char **metadata_vals,
                         int metadata_count)
{
    if (!out || !name || !target_kind || !target_cid) return -1;
    memset(out, 0, sizeof(*out));

    /* 1. Build target := {kind, cid} */
    pksc_value *target = pksc_v_obj_new();
    pksc_v_obj_set(target, "kind", pksc_v_str(target_kind));
    pksc_v_obj_set(target, "cid", pksc_v_str(target_cid));

    /* 2. Build header (7 canonical fields per spec §1.R3) */
    pksc_value *header = pksc_v_obj_new();
    pksc_v_obj_set(header, "schemaVersion", pksc_v_str("1"));
    pksc_v_obj_set(header, "kind", pksc_v_str("bridge"));
    pksc_v_obj_set(header, "name", pksc_v_str(name));
    pksc_v_obj_set(header, "sourceSymbol", pksc_v_str(source_symbol));
    pksc_v_obj_set(header, "sourceLayer", pksc_v_str(source_layer));
    pksc_v_obj_set(header, "sourceContractCid", pksc_v_str(source_contract_cid));
    pksc_v_obj_set(header, "target", target);

    /* 3. Build metadata (omit empty entries per §1.R2) */
    pksc_value *meta = pksc_v_obj_new();
    for (int i = 0; i < metadata_count; i++) {
        if (metadata_keys[i] && metadata_vals[i] && metadata_vals[i][0]) {
            pksc_v_obj_set(meta, metadata_keys[i], pksc_v_str(metadata_vals[i]));
        }
    }

    /* 4. Sign payload := JCS({header, metadata}) */
    pksc_value *sig_payload = pksc_v_obj_new();
    pksc_v_obj_set(sig_payload, "header", header);
    pksc_v_obj_set(sig_payload, "metadata", meta);

    char *sig_payload_jcs = pksc_jcs_encode_string(sig_payload);
    if (!sig_payload_jcs) return -1;

    char *sig_str = pksc_ed25519_sign_string(seed,
        (const uint8_t *)sig_payload_jcs, strlen(sig_payload_jcs));
    free(sig_payload_jcs);
    if (!sig_str) return -1;

    /* 5. Build envelope */
    char *pubkey_str = pksc_ed25519_pubkey_string(seed);
    pksc_value *envelope = pksc_v_obj_new();
    pksc_v_obj_set(envelope, "signer", pksc_v_str(pubkey_str));
    pksc_v_obj_set(envelope, "declaredAt", pksc_v_str(declared_at));
    pksc_v_obj_set(envelope, "signature", pksc_v_str(sig_str));
    free(pubkey_str);
    free(sig_str);

    /* 6. Full memento := {envelope, header, metadata} */
    pksc_value *memento = pksc_v_obj_new();
    pksc_v_obj_set(memento, "envelope", envelope);
    pksc_v_obj_set(memento, "header", header);
    pksc_v_obj_set(memento, "metadata", meta);

    char *canonical = pksc_jcs_encode_string(memento);
    if (!canonical) return -1;

    /* 7. CID = BLAKE3-512(canonical) */
    char *cid_str = pksc_blake3_512_cid(
        (const uint8_t *)canonical, strlen(canonical));

    out->cid = cid_str;
    out->canonical_len = strlen(canonical);
    out->canonical_bytes = (uint8_t *)canonical; /* transfer ownership */

    return 0;
}
