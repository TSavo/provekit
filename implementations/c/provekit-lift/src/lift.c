/* SPDX-License-Identifier: Apache-2.0
 *
 * provekit-lift-c compatibility facade.
 *
 * The real C path is the C-family lifter stack over provekit-lift-core.
 * This legacy library surface stays fail-closed so old callers get a clear
 * route to c-sparse, c-kernel-doc, and c-assertions instead of silent marker
 * lifting.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/lift.h"

/* kernel annotations: enabled via CLI flag, not default */
static int g_kernel_annotations_enabled = 0;

/* last error buffer */
static char g_last_error[512] = {0};

static const char *COMPAT_ERROR =
    "generic C surface is a compatibility facade; use one of the C-family "
    "surfaces: c-sparse, c-kernel-doc, c-assertions";

void pk_enable_kernel_annotations(int enabled) {
    g_kernel_annotations_enabled = enabled;
}

const char *pk_last_error(void) {
    return g_last_error[0] ? g_last_error : NULL;
}

static void set_error(const char *msg) {
    g_last_error[0] = '\0';
    strncpy(g_last_error, msg, sizeof(g_last_error) - 1);
}

static char *read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    long len = ftell(f);
    if (len < 0) {
        fclose(f);
        return NULL;
    }
    rewind(f);
    char *buf = (char *)malloc((size_t)len + 1);
    if (!buf) {
        fclose(f);
        return NULL;
    }
    size_t got = fread(buf, 1, (size_t)len, f);
    fclose(f);
    buf[got] = '\0';
    return buf;
}

pk_lift_result *pk_lift_file(const char *path) {
    if (!path) {
        set_error("null path");
        return NULL;
    }

    char *source = read_file(path);
    if (!source) {
        set_error("pk_lift_file: unable to read source file");
        return NULL;
    }
    pk_lift_result *result = pk_lift_source(source);
    free(source);
    return result;
}

pk_lift_result *pk_lift_source(const char *source) {
    if (!source) {
        set_error("null source");
        return NULL;
    }

    set_error(COMPAT_ERROR);
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
