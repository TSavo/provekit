/* SPDX-License-Identifier: Apache-2.0
 *
 * provekit-lift-c — C kit lifter via libclang (stub implementation).
 *
 * TODO(#380): Wire libclang bindings.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

#include "provekit/lift.h"

/* kernel annotations: enabled via CLI flag, not default */
static int g_kernel_annotations_enabled = 0;

/* last error buffer */
static char g_last_error[512] = {0};

void pk_enable_kernel_annotations(int enabled) {
    g_kernel_annotations_enabled = enabled;
}

const char *pk_last_error(void) {
    return g_last_error[0] ? g_last_error : NULL;
}

static void set_error(const char *msg) {
    strncpy(g_last_error, msg, sizeof(g_last_error) - 1);
}

pk_lift_result *pk_lift_file(const char *path) {
    if (!path) {
        set_error("null path");
        return NULL;
    }

    /* stub: reject until libclang wired */
    set_error("pk_lift_file: libclang integration TODO (#380)");
    return NULL;
}

pk_lift_result *pk_lift_source(const char *source) {
    (void)source;

    /* stub: reject until libclang wired */
    set_error("pk_lift_source: libclang integration TODO (#380)");
    return NULL;
}

void pk_lift_result_free(pk_lift_result *r) {
    if (!r) return;
    free(r->cid);
    free(r->proof_ir_bundle);
    if (r->errors) {
        for (size_t i = 0; i < r->n_errors; i++) {
            free(r->errors[i]);
        }
        free(r->errors);
    }
    free(r);
}

/* ----------------------------------------------------------------------- */
/* stub main for testing                                               */
/* ----------------------------------------------------------------------- */

#ifdef PROVEKIT_LIFT_C_STUB

int main(int argc, char **argv) {
    int kernel_mode = 0;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--kernel") == 0) {
            kernel_mode = 1;
        } else {
            fprintf(stderr, "Usage: %s [--kernel] <source.c>\n", argv[0]);
            return 1;
        }
    }

    pk_enable_kernel_annotations(kernel_mode);

    pk_lift_result *r = pk_lift_source(
        "void foo(int *x) {\n"
        "    BUG_ON(!x);\n"
        "}\n"
    );

    if (r) {
        printf("CID: %s\n", r->cid ?: "(null)");
        printf("Bundle: %s\n", r->proof_ir_bundle ?: "(null)");
        pk_lift_result_free(r);
    } else {
        printf("Error: %s\n", pk_last_error() ?: "(unknown)");
    }

    return r ? 0 : 1;
}

#endif /* PROVEKIT_LIFT_C_STUB */