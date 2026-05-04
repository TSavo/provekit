/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Orchestrator. Drives one full author + mint + bundle pass:
 *
 *   1. Author every contract slab in c_kit_invariants.c.
 *   2. For each ContractDecl, JCS-encode the canonical contract body
 *      {name, outBinding, pre?, post?, inv?} and hash to BLAKE3-512 to
 *      get the signer-independent contentCid (member CID).
 *   3. The member bytes are those same JCS bytes; the catalog wraps
 *      them via pksc_proof_build.
 *   4. contractSetCid = BLAKE3-512(JCS(sorted(contentCids))) — byte-
 *      identical to other kits per spec
 *      protocol/specs/2026-05-03-contract-set-extension.md §1.
 *
 * Mirrors the rust contract_cid / compute_contract_set_cid model
 * (implementations/rust/provekit-claim-envelope/src/lib.rs §1).
 */

#include "orchestrator.h"

#include "c_kit_invariants.h"
#include "slab.h"

#include "provekit/self_contracts.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>

#define MCSC_DECLARED_AT     "2026-05-03T18:00:00Z"
#define MCSC_PRODUCED_BY     "provekit-c-self-contracts@1.0"
#define MCSC_CATALOG_NAME    "@provekit/c-self-contracts"
#define MCSC_CATALOG_VERSION "1.0.0"

/* ----------------------------------------------------------------------- */
/* helpers                                                                  */
/* ----------------------------------------------------------------------- */

static char *dup_cstr(const char *s) {
    if (!s) return NULL;
    size_t n = strlen(s);
    char *p = (char *)malloc(n + 1);
    if (!p) return NULL;
    memcpy(p, s, n + 1);
    return p;
}

/* Deep-copy a pksc_value tree. Used because each contract decl's pre/post/inv
 * is owned by the slab; we need an independent tree to assemble into the
 * contract_body object passed to JCS. (We could mutate ownership instead, but
 * that complicates slab teardown; copying is simpler and slabs are small.) */
static pksc_value *value_copy(const pksc_value *v) {
    if (!v) return NULL;
    switch (v->kind) {
        case PKSC_V_NULL: return pksc_v_null();
        case PKSC_V_BOOL: return pksc_v_bool(v->as.b);
        case PKSC_V_INT:  return pksc_v_int(v->as.i);
        case PKSC_V_STR:  return pksc_v_str(v->as.s);
        case PKSC_V_ARR: {
            pksc_value *out = pksc_v_arr_new();
            if (!out) return NULL;
            for (size_t i = 0; i < v->as.arr.n; i++) {
                pksc_value *child = value_copy(v->as.arr.items[i]);
                if (!child || pksc_v_arr_push(out, child) != 0) {
                    pksc_value_free(child);
                    pksc_value_free(out);
                    return NULL;
                }
            }
            return out;
        }
        case PKSC_V_OBJ: {
            pksc_value *out = pksc_v_obj_new();
            if (!out) return NULL;
            for (size_t i = 0; i < v->as.obj.n; i++) {
                pksc_value *child = value_copy(v->as.obj.entries[i].value);
                if (!child ||
                    pksc_v_obj_set(out, v->as.obj.entries[i].key, child) != 0) {
                    pksc_value_free(child);
                    pksc_value_free(out);
                    return NULL;
                }
            }
            return out;
        }
    }
    return NULL;
}

/* Compare for qsort(sort lex on `char *`). */
static int cmp_str_ptr(const void *a, const void *b) {
    const char *sa = *(const char *const *)a;
    const char *sb = *(const char *const *)b;
    return strcmp(sa, sb);
}

/* contractSetCid = BLAKE3-512(JCS(sorted([cids...]))). Returns malloc'd
 * "blake3-512:<hex>" string or NULL on failure. */
static char *compute_contract_set_cid(char **cids, size_t n) {
    /* Sort lexicographically (stable; cids are unique per content). */
    char **sorted = (char **)malloc(n * sizeof(*sorted));
    if (!sorted && n > 0) return NULL;
    for (size_t i = 0; i < n; i++) sorted[i] = cids[i];
    qsort(sorted, n, sizeof(*sorted), cmp_str_ptr);

    pksc_value *arr = pksc_v_arr_new();
    if (!arr) { free(sorted); return NULL; }
    for (size_t i = 0; i < n; i++) {
        pksc_value *s = pksc_v_str(sorted[i]);
        if (!s || pksc_v_arr_push(arr, s) != 0) {
            pksc_value_free(s);
            pksc_value_free(arr);
            free(sorted);
            return NULL;
        }
    }
    free(sorted);

    pksc_bytes jcs;
    pksc_bytes_init(&jcs);
    if (pksc_jcs_encode(&jcs, arr) != 0) {
        pksc_value_free(arr);
        pksc_bytes_free(&jcs);
        return NULL;
    }
    pksc_value_free(arr);
    char *cid = pksc_blake3_512_cid(jcs.data, jcs.len);
    pksc_bytes_free(&jcs);
    return cid;
}

/* Build the canonical contract body JCS-Value:
 *   {"name":<name>,"outBinding":<out>,"pre"?:<pre>,"post"?:<post>,"inv"?:<inv>}
 * Returns malloc'd value tree or NULL. The pre/post/inv args are
 * deep-copied from the slab. */
static pksc_value *build_contract_body(const mcsc_contract *c) {
    pksc_value *o = pksc_v_obj_new();
    if (!o) return NULL;
    pksc_value *name_v = pksc_v_str(c->name);
    pksc_value *out_v = pksc_v_str(c->out_binding);
    if (!name_v || !out_v) {
        pksc_value_free(name_v);
        pksc_value_free(out_v);
        pksc_value_free(o);
        return NULL;
    }
    if (pksc_v_obj_set(o, "name", name_v) != 0) goto fail;
    if (pksc_v_obj_set(o, "outBinding", out_v) != 0) goto fail;
    if (c->pre) {
        pksc_value *cp = value_copy(c->pre);
        if (!cp || pksc_v_obj_set(o, "pre", cp) != 0) {
            pksc_value_free(cp);
            goto fail_o_only;
        }
    }
    if (c->post) {
        pksc_value *cp = value_copy(c->post);
        if (!cp || pksc_v_obj_set(o, "post", cp) != 0) {
            pksc_value_free(cp);
            goto fail_o_only;
        }
    }
    if (c->inv) {
        pksc_value *cp = value_copy(c->inv);
        if (!cp || pksc_v_obj_set(o, "inv", cp) != 0) {
            pksc_value_free(cp);
            goto fail_o_only;
        }
    }
    return o;
fail:
    pksc_value_free(name_v);
    pksc_value_free(out_v);
fail_o_only:
    pksc_value_free(o);
    return NULL;
}

/* Make a directory if it doesn't exist. */
static int ensure_dir(const char *path) {
    if (!path) return 0;
    struct stat st;
    if (stat(path, &st) == 0) {
        return S_ISDIR(st.st_mode) ? 0 : -1;
    }
    if (mkdir(path, 0755) != 0) return -1;
    return 0;
}

/* ----------------------------------------------------------------------- */
/* Public API                                                              */
/* ----------------------------------------------------------------------- */

void mcsc_mint_result_init(mcsc_mint_result *r) {
    if (!r) return;
    r->cid = NULL;
    r->contract_set_cid = NULL;
    r->contract_count = 0;
    r->slab_count = 0;
    r->bytes = NULL;
    r->bytes_len = 0;
}

void mcsc_mint_result_free(mcsc_mint_result *r) {
    if (!r) return;
    free(r->cid);
    free(r->contract_set_cid);
    free(r->bytes);
    mcsc_mint_result_init(r);
}

int mcsc_mint_one_run(const char *out_dir, mcsc_mint_result *out) {
    if (!out) return -1;
    mcsc_mint_result_init(out);

    if (pksc_sodium_init() != 0) return -1;

    if (out_dir && ensure_dir(out_dir) != 0) return -1;

    mcsc_slab_list *slabs = mcsc_author_all();
    if (!slabs) return -1;

    /* Count total contracts upfront. */
    size_t total = 0;
    for (size_t i = 0; i < slabs->n; i++) total += slabs->slabs[i]->n;

    pksc_member *members = (pksc_member *)calloc(total ? total : 1, sizeof(*members));
    char       **content_cids = (char **)calloc(total ? total : 1, sizeof(*content_cids));
    if (!members || !content_cids) {
        free(members);
        free(content_cids);
        mcsc_slab_list_free(slabs);
        return -1;
    }

    size_t idx = 0;
    for (size_t si = 0; si < slabs->n; si++) {
        mcsc_slab *s = slabs->slabs[si];
        for (size_t ci = 0; ci < s->n; ci++) {
            const mcsc_contract *c = s->contracts[ci];
            pksc_value *body = build_contract_body(c);
            if (!body) goto fail;

            pksc_bytes jcs;
            pksc_bytes_init(&jcs);
            if (pksc_jcs_encode(&jcs, body) != 0) {
                pksc_value_free(body);
                pksc_bytes_free(&jcs);
                goto fail;
            }
            pksc_value_free(body);

            char *cid = pksc_blake3_512_cid(jcs.data, jcs.len);
            if (!cid) {
                pksc_bytes_free(&jcs);
                goto fail;
            }

            /* Detect duplicate CIDs across slabs. */
            for (size_t k = 0; k < idx; k++) {
                if (strcmp(members[k].key, cid) == 0) {
                    free(cid);
                    pksc_bytes_free(&jcs);
                    fprintf(stderr,
                            "duplicate contract CID across slabs (contract `%s`)\n",
                            c->name);
                    goto fail;
                }
            }

            members[idx].key = cid; /* takes ownership */
            /* Move ownership of jcs.data into members[idx].bytes. */
            members[idx].bytes = jcs.data;
            members[idx].len = jcs.len;
            jcs.data = NULL;
            jcs.cap = jcs.len = 0;

            content_cids[idx] = dup_cstr(cid);
            if (!content_cids[idx]) goto fail;
            idx++;
        }
    }

    /* Catalog signer CID = BLAKE3-512 of the self-identifying pubkey string.
     * Mirrors rust signer_cid = blake3_512_of(signer_pubkey.as_bytes()). */
    char *pubkey_str = pksc_ed25519_pubkey_string(PKSC_FOUNDATION_V0_SEED);
    if (!pubkey_str) goto fail;
    char *signer_cid = pksc_blake3_512_cid(
        (const uint8_t *)pubkey_str, strlen(pubkey_str));
    free(pubkey_str);
    if (!signer_cid) goto fail;

    pksc_proof_input in;
    memset(&in, 0, sizeof(in));
    in.name = MCSC_CATALOG_NAME;
    in.version = MCSC_CATALOG_VERSION;
    in.binary_cid = NULL;
    in.metadata = NULL;
    in.n_metadata = 0;
    in.members = members;
    in.n_members = total;
    in.signer_cid = signer_cid;
    in.declared_at = MCSC_DECLARED_AT;
    in.signer_seed = PKSC_FOUNDATION_V0_SEED;

    pksc_proof_output po;
    memset(&po, 0, sizeof(po));
    int rc = pksc_proof_build(&in, &po);
    free(signer_cid);
    if (rc != 0) goto fail;

    /* Compute contractSetCid from the per-contract content CIDs. */
    char *cset = compute_contract_set_cid(content_cids, total);
    if (!cset) {
        pksc_proof_output_free(&po);
        goto fail;
    }

    /* Write <cid>.proof if outDir requested. */
    if (out_dir) {
        size_t plen = strlen(out_dir) + 1 + strlen(po.cid) + 6 + 1;
        char *path = (char *)malloc(plen);
        if (!path) {
            free(cset);
            pksc_proof_output_free(&po);
            goto fail;
        }
        snprintf(path, plen, "%s/%s.proof", out_dir, po.cid);
        FILE *fp = fopen(path, "wb");
        if (!fp) {
            free(path);
            free(cset);
            pksc_proof_output_free(&po);
            goto fail;
        }
        if (fwrite(po.bytes, 1, po.len, fp) != po.len) {
            fclose(fp);
            free(path);
            free(cset);
            pksc_proof_output_free(&po);
            goto fail;
        }
        fclose(fp);
        free(path);
    }

    out->cid = po.cid;             /* take ownership */
    out->bytes = po.bytes;         /* take ownership */
    out->bytes_len = po.len;
    out->contract_set_cid = cset;
    out->contract_count = total;
    out->slab_count = slabs->n;
    /* po.cid and po.bytes are now owned by `out`; clear po so its free
     * doesn't double-free. */
    po.cid = NULL;
    po.bytes = NULL;
    pksc_proof_output_free(&po);

    /* Free intermediates: members (key+bytes were moved to po, then to out's
     * bytes via the proof envelope; but pksc_proof_build COPIES the keys
     * and bytes internally — see proof_envelope.c). So we still own the
     * `members[*].key` and `.bytes` we passed in and must free them. */
    for (size_t i = 0; i < total; i++) {
        free(members[i].key);
        free(members[i].bytes);
    }
    free(members);
    for (size_t i = 0; i < total; i++) free(content_cids[i]);
    free(content_cids);
    mcsc_slab_list_free(slabs);
    return 0;

fail:
    for (size_t i = 0; i < idx; i++) {
        free(members[i].key);
        free(members[i].bytes);
        free(content_cids[i]);
    }
    free(members);
    free(content_cids);
    mcsc_slab_list_free(slabs);
    mcsc_mint_result_free(out);
    return -1;
}

int mcsc_run_cli(const char *out_dir) {
    if (!out_dir) out_dir = ".";

    fprintf(stdout, "== ProvekIt C self-contracts orchestrator ==\n\n");
    fprintf(stdout, "output dir: %s\n\n", out_dir);

    fprintf(stdout, "== mint #1 ==\n");
    mcsc_mint_result run1;
    mcsc_mint_result_init(&run1);
    if (mcsc_mint_one_run(out_dir, &run1) != 0) {
        fprintf(stderr, "mint #1 failed\n");
        return 1;
    }
    fprintf(stdout, "  contracts:        %zu across %zu slabs\n",
            run1.contract_count, run1.slab_count);
    fprintf(stdout, "  catalog CID:      %s\n", run1.cid);
    fprintf(stdout, "  contractSetCid:   %s\n", run1.contract_set_cid);
    fprintf(stdout, "  proof bytes:      %zu\n", run1.bytes_len);
    fprintf(stdout, "  .proof file:      %s/%s.proof\n", out_dir, run1.cid);

    /* Determinism check: mint again into a sibling dir. */
    size_t det_len = strlen(out_dir) + strlen("/_determinism_check") + 1;
    char *det_dir = (char *)malloc(det_len);
    if (!det_dir) {
        mcsc_mint_result_free(&run1);
        return 1;
    }
    snprintf(det_dir, det_len, "%s/_determinism_check", out_dir);

    fprintf(stdout, "\n== mint #2 (determinism check) ==\n");
    mcsc_mint_result run2;
    mcsc_mint_result_init(&run2);
    if (mcsc_mint_one_run(det_dir, &run2) != 0) {
        fprintf(stderr, "mint #2 failed\n");
        free(det_dir);
        mcsc_mint_result_free(&run1);
        return 1;
    }
    free(det_dir);

    int ok = (strcmp(run1.cid, run2.cid) == 0)
          && (strcmp(run1.contract_set_cid, run2.contract_set_cid) == 0);
    if (!ok) {
        fprintf(stderr, "DETERMINISM FAILURE:\n");
        fprintf(stderr, "  run 1 cid:              %s\n", run1.cid);
        fprintf(stderr, "  run 2 cid:              %s\n", run2.cid);
        fprintf(stderr, "  run 1 contractSetCid:   %s\n", run1.contract_set_cid);
        fprintf(stderr, "  run 2 contractSetCid:   %s\n", run2.contract_set_cid);
        mcsc_mint_result_free(&run1);
        mcsc_mint_result_free(&run2);
        return 1;
    }
    fprintf(stdout, "  determinism check:  OK (two runs produced identical CIDs)\n\n");
    fprintf(stdout, "== done. C self-application: live (%zu contracts across %zu slabs). ==\n",
            run1.contract_count, run1.slab_count);

    mcsc_mint_result_free(&run1);
    mcsc_mint_result_free(&run2);
    return 0;
}
