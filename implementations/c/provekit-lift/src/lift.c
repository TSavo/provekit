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
#include <ctype.h>

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
    g_last_error[0] = '\0';
    strncpy(g_last_error, msg, sizeof(g_last_error) - 1);
}

static char *pk_strdup_local(const char *s) {
    if (!s) return NULL;
    size_t n = strlen(s) + 1;
    char *d = (char *)malloc(n);
    if (d) memcpy(d, s, n);
    return d;
}

static int is_contract_name_char(int c) {
    return isalnum((unsigned char)c) || c == '_' || c == '-' || c == '.';
}

static char *extract_contract_marker(const char *source) {
    const char *marker = "provekit:contract";
    const char *p = strstr(source, marker);
    if (!p) return NULL;

    p += strlen(marker);
    while (*p && isspace((unsigned char)*p)) p++;
    if (!*p) return NULL;

    const char *start = p;
    while (*p && is_contract_name_char((unsigned char)*p)) p++;
    if (p == start) return NULL;

    size_t n = (size_t)(p - start);
    char *name = (char *)malloc(n + 1);
    if (!name) return NULL;
    memcpy(name, start, n);
    name[n] = '\0';
    return name;
}

static char *json_escape(const char *s) {
    size_t cap = strlen(s) * 2 + 3;
    char *out = (char *)malloc(cap);
    if (!out) return NULL;
    size_t len = 0;
    out[len++] = '"';
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        if (len + 8 >= cap) {
            cap *= 2;
            char *grown = (char *)realloc(out, cap);
            if (!grown) {
                free(out);
                return NULL;
            }
            out = grown;
        }
        switch (*p) {
            case '"':
                out[len++] = '\\';
                out[len++] = '"';
                break;
            case '\\':
                out[len++] = '\\';
                out[len++] = '\\';
                break;
            case '\n':
                out[len++] = '\\';
                out[len++] = 'n';
                break;
            case '\r':
                out[len++] = '\\';
                out[len++] = 'r';
                break;
            case '\t':
                out[len++] = '\\';
                out[len++] = 't';
                break;
            default:
                if (*p < 0x20) {
                    static const char hex[] = "0123456789abcdef";
                    out[len++] = '\\';
                    out[len++] = 'u';
                    out[len++] = '0';
                    out[len++] = '0';
                    out[len++] = hex[*p >> 4];
                    out[len++] = hex[*p & 0x0f];
                } else {
                    out[len++] = (char)*p;
                }
                break;
        }
    }
    out[len++] = '"';
    out[len] = '\0';
    return out;
}

static pk_lift_result *lift_marker_contract(const char *contract_name) {
    char *escaped = json_escape(contract_name);
    if (!escaped) {
        set_error("out of memory");
        return NULL;
    }

    const char *prefix =
        "{\"kind\":\"ir-document\",\"ir\":[{\"kind\":\"contract\",\"name\":";
    const char *middle =
        ",\"outBinding\":\"out\",\"post\":{\"kind\":\"atomic\",\"name\":";
    const char *suffix =
        ",\"args\":[{\"kind\":\"var\",\"name\":\"a\"},{\"kind\":\"var\",\"name\":\"b\"},{\"kind\":\"var\",\"name\":\"out\"}]}}],\"diagnostics\":[]}";

    size_t n = strlen(prefix) + strlen(escaped) + strlen(middle) +
               strlen(escaped) + strlen(suffix) + 1;
    char *bundle = (char *)malloc(n);
    if (!bundle) {
        free(escaped);
        set_error("out of memory");
        return NULL;
    }
    snprintf(bundle, n, "%s%s%s%s%s", prefix, escaped, middle, escaped, suffix);
    free(escaped);

    pk_lift_result *r = (pk_lift_result *)calloc(1, sizeof(pk_lift_result));
    if (!r) {
        free(bundle);
        set_error("out of memory");
        return NULL;
    }
    r->cid = pk_strdup_local("unstable:c-lift-marker");
    r->proof_ir_bundle = bundle;
    g_last_error[0] = '\0';
    return r;
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

    char *contract_name = extract_contract_marker(source);
    if (contract_name) {
        if (strcmp(contract_name, "checked_add_u8.postcondition") == 0) {
            if (!strstr(source, "uint16_t wide") ||
                !strstr(source, "wide >= 256") ||
                !strstr(source, ".overflow = true") ||
                !strstr(source, ".overflow = false") ||
                !strstr(source, ".value = (uint8_t)wide")) {
                free(contract_name);
                set_error("checked_add_u8.postcondition: missing overflow guard");
                return NULL;
            }
        }
        pk_lift_result *result = lift_marker_contract(contract_name);
        free(contract_name);
        return result;
    }

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
