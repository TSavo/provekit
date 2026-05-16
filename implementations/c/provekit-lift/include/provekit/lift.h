/* SPDX-License-Identifier: Apache-2.0
 *
 * provekit-lift-c - C kit lifter via libclang.
 *
 * Lifts C source to proof.ir bundles. Parses kernel annotations:
 *   - BUG_ON, WARN_ON, assert
 *   - sparse: __user, __rcu, __must_hold, __acquires, __releases
 *
 * Architecture mirrors provekit-walk:
 *   - canonical: JCS + BLAKE3-512
 *   - lift: AST → IR
 *   - walk: WP propagation
 *   - shadow: predicate capture
 *   - emit: proof.ir bundle
 */

#ifndef PROVEKIT_LIFT_C_H
#define PROVEKIT_LIFT_C_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ----------------------------------------------------------------------- */
/* Lift result                                                           */
/* ----------------------------------------------------------------------- */

typedef struct {
    char *cid;
    char *proof_ir_bundle;  /* JCS-canonical JSON */
    size_t n_errors;
    char **errors;
} pk_lift_result;

void pk_lift_result_free(pk_lift_result *r);

/* ----------------------------------------------------------------------- */
/* API                                                                   */
/* ----------------------------------------------------------------------- */

/* Lift a C source file.
 * Returns NULL on error (check pk_last_error).
 */
pk_lift_result *pk_lift_file(const char *path);

/* Lift C source from memory.
 * source: nul-terminated C code
 * Returns NULL on error.
 */
pk_lift_result *pk_lift_source(const char *source);

/* Enable kernel annotation parsing (default: disabled)
 * Parses: BUG_ON, WARN_ON, __user, __rcu, __must_hold, __acquires, __releases
 */
void pk_enable_kernel_annotations(int enabled);

/* Get last error message.
 * Returns NULL if no error.
 */
const char *pk_last_error(void);

#ifdef __cplusplus
}
#endif

#endif /* PROVEKIT_LIFT_C_H */