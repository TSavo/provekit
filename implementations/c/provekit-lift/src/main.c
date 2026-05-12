/* SPDX-License-Identifier: Apache-2.0
 *
 * provekit-lift-c compatibility facade.
 *
 * The generic `c` surface is no longer a semantic lifter. New C work belongs
 * to the C-family lifters over provekit-lift-core: c-sparse, c-kernel-doc,
 * c-assertions, and future siblings. This binary stays only to give existing
 * manifests and users a clear migration error instead of silently minting
 * legacy marker contracts.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} Buf;

static const char *COMPAT_ERROR =
    "generic C surface is a compatibility facade; use one of the C-family "
    "surfaces: c-sparse, c-kernel-doc, c-assertions";

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
        "\"deprecated\":true,\"emits_signed_mementos\":false,"
        "\"ir_version\":\"v1.1.0\","
        "\"replacement_surfaces\":[\"c-sparse\",\"c-kernel-doc\",\"c-assertions\"]},"
        "\"name\":\"c-lift-compat\",\"protocol_version\":\"pep/1.7.0\","
        "\"version\":\"0.1.0\"}");
}

static void handle_lift(const char *id) {
    send_error(id, -32602, COMPAT_ERROR);
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
            handle_lift(safe_id);
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
    fprintf(stderr, "%s\n", COMPAT_ERROR);
    return 1;
}

int main(int argc, char **argv) {
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--rpc") == 0) return run_rpc();
        if (strcmp(argv[i], "--self-test") == 0) return self_test();
    }
    fprintf(stderr, "Usage: provekit-lift-c --rpc | --self-test\n");
    return 1;
}
