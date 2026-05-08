/* SPDX-License-Identifier: Apache-2.0
 *
 * provekit-lift-c generic lift-plugin facade.
 *
 * C follows the same shape as Java: the language has multiple lift surfaces.
 * `c-self-contracts` mints the kit's own proof envelope; this generic `c`
 * surface is an RPC facade over extractors. The first extractor is the
 * source-contract marker path used by Bridgeworks. Future extractors can add
 * libclang, kernel annotations, ACSL, or other C contract families behind the
 * same lift-plugin protocol without changing the Rust CLI mint boundary.
 */

#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#include "provekit/lift.h"

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} Buf;

static void buf_init(Buf *b) {
    b->cap = 256;
    b->len = 0;
    b->data = (char *)malloc(b->cap);
    if (b->data) b->data[0] = '\0';
}

static void buf_free(Buf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int buf_grow(Buf *b, size_t need) {
    if (b->len + need + 1 <= b->cap) return 0;
    size_t next = b->cap ? b->cap : 256;
    while (next < b->len + need + 1) next *= 2;
    char *data = (char *)realloc(b->data, next);
    if (!data) return -1;
    b->data = data;
    b->cap = next;
    return 0;
}

static int buf_append_n(Buf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) return -1;
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int buf_append(Buf *b, const char *s) {
    return buf_append_n(b, s, strlen(s));
}

static int buf_append_char(Buf *b, char c) {
    return buf_append_n(b, &c, 1);
}

static void json_escape_str(Buf *out, const char *s) {
    buf_append_char(out, '"');
    for (const unsigned char *p = (const unsigned char *)s; p && *p; p++) {
        switch (*p) {
            case '"':
                buf_append(out, "\\\"");
                break;
            case '\\':
                buf_append(out, "\\\\");
                break;
            case '\b':
                buf_append(out, "\\b");
                break;
            case '\f':
                buf_append(out, "\\f");
                break;
            case '\n':
                buf_append(out, "\\n");
                break;
            case '\r':
                buf_append(out, "\\r");
                break;
            case '\t':
                buf_append(out, "\\t");
                break;
            default:
                if (*p < 0x20) {
                    char esc[7];
                    snprintf(esc, sizeof(esc), "\\u00%02x", *p);
                    buf_append(out, esc);
                } else {
                    buf_append_char(out, (char)*p);
                }
                break;
        }
    }
    buf_append_char(out, '"');
}

static char *json_extract_str(const char *json, const char *field) {
    char needle[128];
    snprintf(needle, sizeof(needle), "\"%s\"", field);
    const char *p = strstr(json, needle);
    if (!p) return NULL;
    p += strlen(needle);
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (*p != '"') return NULL;
    p++;

    Buf b;
    buf_init(&b);
    while (*p && *p != '"') {
        if (*p == '\\' && p[1]) {
            p++;
            switch (*p) {
                case '"':
                    buf_append_char(&b, '"');
                    break;
                case '\\':
                    buf_append_char(&b, '\\');
                    break;
                case 'n':
                    buf_append_char(&b, '\n');
                    break;
                case 'r':
                    buf_append_char(&b, '\r');
                    break;
                case 't':
                    buf_append_char(&b, '\t');
                    break;
                default:
                    buf_append_char(&b, *p);
                    break;
            }
        } else {
            buf_append_char(&b, *p);
        }
        p++;
    }

    char *out = b.data;
    b.data = NULL;
    buf_free(&b);
    return out;
}

static char *json_extract_id(const char *json) {
    const char *p = strstr(json, "\"id\"");
    if (!p) return NULL;
    p += 4;
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (!*p || *p == '}') return NULL;

    Buf b;
    buf_init(&b);
    while (*p && *p != ',' && *p != '}') {
        buf_append_char(&b, *p);
        p++;
    }
    while (b.len > 0 && (b.data[b.len - 1] == ' ' || b.data[b.len - 1] == '\t')) {
        b.data[--b.len] = '\0';
    }
    char *out = b.data;
    b.data = NULL;
    buf_free(&b);
    return out;
}

static char *json_extract_method(const char *json) {
    return json_extract_str(json, "method");
}

static int has_suffix(const char *s, const char *suffix) {
    size_t sl = strlen(s);
    size_t tl = strlen(suffix);
    return sl >= tl && strcmp(s + sl - tl, suffix) == 0;
}

static char *join_path(const char *a, const char *b) {
    size_t al = strlen(a);
    size_t bl = strlen(b);
    int needs_slash = al > 0 && a[al - 1] != '/';
    char *out = (char *)malloc(al + (needs_slash ? 1 : 0) + bl + 1);
    if (!out) return NULL;
    memcpy(out, a, al);
    size_t pos = al;
    if (needs_slash) out[pos++] = '/';
    memcpy(out + pos, b, bl);
    out[pos + bl] = '\0';
    return out;
}

typedef struct {
    Buf ir_items;
    Buf diagnostics;
    size_t ir_count;
    size_t diag_count;
    int hard_error;
    char error[512];
} LiftAccumulator;

static void acc_init(LiftAccumulator *acc) {
    memset(acc, 0, sizeof(*acc));
    buf_init(&acc->ir_items);
    buf_init(&acc->diagnostics);
}

static void acc_free(LiftAccumulator *acc) {
    buf_free(&acc->ir_items);
    buf_free(&acc->diagnostics);
}

static void add_diagnostic(LiftAccumulator *acc, const char *message) {
    if (acc->diag_count > 0) buf_append_char(&acc->diagnostics, ',');
    json_escape_str(&acc->diagnostics, message);
    acc->diag_count++;
}

static int append_ir_from_bundle(LiftAccumulator *acc, const char *bundle) {
    const char *start = strstr(bundle, "\"ir\":[");
    if (!start) return -1;
    start += strlen("\"ir\":[");
    const char *end = strstr(start, "],\"diagnostics\"");
    if (!end) return -1;
    if (end == start) return 0;

    if (acc->ir_count > 0) buf_append_char(&acc->ir_items, ',');
    if (buf_append_n(&acc->ir_items, start, (size_t)(end - start)) != 0) return -1;
    acc->ir_count++;
    return 0;
}

static int lift_one_file(const char *path, LiftAccumulator *acc) {
    pk_lift_result *r = pk_lift_file(path);
    if (r) {
        int ok = append_ir_from_bundle(acc, r->proof_ir_bundle);
        pk_lift_result_free(r);
        if (ok != 0) {
            snprintf(acc->error, sizeof(acc->error), "invalid lift bundle from %s", path);
            acc->hard_error = 1;
            return -1;
        }
        return 0;
    }

    const char *err = pk_last_error();
    if (err && strstr(err, "libclang integration TODO")) {
        Buf diag;
        buf_init(&diag);
        buf_append(&diag, path);
        buf_append(&diag, ": ");
        buf_append(&diag, err);
        add_diagnostic(acc, diag.data ? diag.data : err);
        buf_free(&diag);
        return 0;
    }

    snprintf(acc->error, sizeof(acc->error), "%s: %s", path, err ? err : "lift failed");
    acc->hard_error = 1;
    return -1;
}

static int walk_path(const char *path, LiftAccumulator *acc) {
    struct stat st;
    if (stat(path, &st) != 0) {
        snprintf(acc->error, sizeof(acc->error), "%s: stat failed: %s", path, strerror(errno));
        acc->hard_error = 1;
        return -1;
    }

    if (S_ISREG(st.st_mode)) {
        if (has_suffix(path, ".c")) return lift_one_file(path, acc);
        return 0;
    }

    if (!S_ISDIR(st.st_mode)) return 0;

    DIR *dir = opendir(path);
    if (!dir) {
        snprintf(acc->error, sizeof(acc->error), "%s: opendir failed: %s", path, strerror(errno));
        acc->hard_error = 1;
        return -1;
    }

    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) continue;
        char *child = join_path(path, entry->d_name);
        if (!child) {
            closedir(dir);
            snprintf(acc->error, sizeof(acc->error), "out of memory walking %s", path);
            acc->hard_error = 1;
            return -1;
        }
        int rc = walk_path(child, acc);
        free(child);
        if (rc != 0) {
            closedir(dir);
            return rc;
        }
    }

    closedir(dir);
    return 0;
}

static void send_response(const char *id, const char *result_json) {
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":%s}\n", id ? id : "null", result_json);
    fflush(stdout);
}

static void send_error(const char *id, int code, const char *message) {
    Buf b;
    buf_init(&b);
    buf_append(&b, "{\"code\":");
    char code_buf[32];
    snprintf(code_buf, sizeof(code_buf), "%d", code);
    buf_append(&b, code_buf);
    buf_append(&b, ",\"message\":");
    json_escape_str(&b, message);
    buf_append_char(&b, '}');
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":%s}\n", id ? id : "null", b.data);
    fflush(stdout);
    buf_free(&b);
}

static void handle_initialize(const char *id) {
    send_response(id,
        "{\"capabilities\":{\"authoring_surfaces\":[\"c\"],"
        "\"emits_signed_mementos\":false,\"ir_version\":\"v1.1.0\"},"
        "\"name\":\"c-lift\",\"protocol_version\":\"provekit-lift/1\","
        "\"version\":\"0.1.0\"}");
}

static void handle_lift(const char *id, const char *line) {
    char *workspace = json_extract_str(line, "workspace_root");
    if (!workspace || !*workspace) {
        free(workspace);
        workspace = (char *)malloc(2);
        if (!workspace) {
            send_error(id, -32603, "out of memory");
            return;
        }
        strcpy(workspace, ".");
    }

    LiftAccumulator acc;
    acc_init(&acc);
    if (walk_path(workspace, &acc) != 0 || acc.hard_error) {
        send_error(id, 1005, acc.error[0] ? acc.error : "C lift failed");
        acc_free(&acc);
        free(workspace);
        return;
    }

    Buf result;
    buf_init(&result);
    buf_append(&result, "{\"diagnostics\":[");
    buf_append(&result, acc.diagnostics.data ? acc.diagnostics.data : "");
    buf_append(&result, "],\"ir\":[");
    buf_append(&result, acc.ir_items.data ? acc.ir_items.data : "");
    buf_append(&result, "],\"kind\":\"ir-document\"}");

    send_response(id, result.data);

    buf_free(&result);
    acc_free(&acc);
    free(workspace);
}

static int run_rpc(void) {
    char line[65536];
    while (fgets(line, sizeof(line), stdin)) {
        size_t n = strlen(line);
        while (n > 0 && (line[n - 1] == '\n' || line[n - 1] == '\r')) {
            line[--n] = '\0';
        }
        if (n == 0) continue;

        char *method = json_extract_method(line);
        char *id = json_extract_id(line);
        const char *safe_id = (id && *id) ? id : "null";

        if (!method) {
            send_error(safe_id, -32700, "parse error: missing method");
        } else if (strcmp(method, "initialize") == 0) {
            handle_initialize(safe_id);
        } else if (strcmp(method, "lift") == 0) {
            handle_lift(safe_id, line);
        } else if (strcmp(method, "shutdown") == 0) {
            send_response(safe_id, "null");
            free(method);
            free(id);
            break;
        } else {
            send_error(safe_id, -32601, "unknown method");
        }

        free(method);
        free(id);
    }
    return 0;
}

static int self_test(void) {
    const char *source =
        "#include <stdbool.h>\n"
        "#include <stdint.h>\n"
        "typedef struct { bool overflow; uint8_t value; } checked_add_u8_result;\n"
        "/* provekit:contract checked_add_u8.postcondition */\n"
        "checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {\n"
        "    uint16_t wide = (uint16_t)a + (uint16_t)b;\n"
        "    if (wide >= 256) return (checked_add_u8_result){ .overflow = true, .value = 0 };\n"
        "    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };\n"
        "}\n";
    pk_lift_result *r = pk_lift_source(source);
    if (!r) {
        fprintf(stderr, "%s\n", pk_last_error() ? pk_last_error() : "self-test failed");
        return 1;
    }
    printf("%s\n", r->proof_ir_bundle);
    pk_lift_result_free(r);
    return 0;
}

int main(int argc, char **argv) {
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--rpc") == 0) return run_rpc();
        if (strcmp(argv[i], "--self-test") == 0) return self_test();
    }
    fprintf(stderr, "Usage: provekit-lift-c --rpc | --self-test\n");
    return 1;
}
