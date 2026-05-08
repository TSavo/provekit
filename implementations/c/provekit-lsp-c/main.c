/* SPDX-License-Identifier: Apache-2.0 */
/*
 * provekit-lsp-c — NDJSON LSP plugin for C.
 *
 * Protocol (provekit-lsp-plugin/1 over stdio):
 *
 *   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
 *   {"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
 *   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
 *
 * For parse: scans the source using the shared C lift core and lifts to the
 * shared parse result shape.
 *
 * Wire shape matches implementations/go/cmd/provekit-lsp-go/main.go.
 *
 * Build:
 *   make
 */

/* `getline` and `ssize_t` are POSIX extensions; glibc gates them behind
 * feature-test macros. Define _GNU_SOURCE before any system header is
 * pulled in (review feedback: PR #165 / CodeRabbit).
 *
 * Also include <sys/types.h> explicitly for `ssize_t` so the build doesn't
 * rely on transitive inclusion through <stdio.h>. */
#define _GNU_SOURCE
#include <sys/types.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/c_lift_core.h"

/* -----------------------------------------------------------------------
 * Dynamic string buffer
 * ----------------------------------------------------------------------- */

typedef struct {
    char   *data;
    size_t  len;
    size_t  cap;
} Buf;

static void buf_init(Buf *b) {
    b->cap  = 256;
    b->len  = 0;
    b->data = (char *)malloc(b->cap);
    if (b->data) b->data[0] = '\0';
}

static void buf_free(Buf *b) {
    free(b->data);
    b->data = NULL;
    b->len  = 0;
    b->cap  = 0;
}

static void buf_grow(Buf *b, size_t need) {
    if (b->len + need + 1 <= b->cap) return;
    size_t nc = b->cap * 2;
    while (nc < b->len + need + 1) nc *= 2;
    char *nd = (char *)realloc(b->data, nc);
    if (!nd) return;
    b->data = nd;
    b->cap  = nc;
}

static void buf_append(Buf *b, const char *s) {
    if (!s) return;
    size_t n = strlen(s);
    buf_grow(b, n);
    memcpy(b->data + b->len, s, n + 1);
    b->len += n;
}

static void buf_append_char(Buf *b, char c) {
    buf_grow(b, 1);
    b->data[b->len] = c;
    b->data[b->len + 1] = '\0';
    b->len++;
}

/* -----------------------------------------------------------------------
 * JSON helpers (hand-rolled; messages are small)
 * ----------------------------------------------------------------------- */

/* JCS-compliant string escaping per RFC 8785. */
static void json_escape_str(Buf *out, const char *s) {
    buf_append_char(out, '"');
    for (const char *p = s; *p; p++) {
        unsigned char c = (unsigned char)*p;
        if (c == '"') {
            buf_append(out, "\\\"");
        } else if (c == '\\') {
            buf_append(out, "\\\\");
        } else if (c == '\b') {
            buf_append(out, "\\b");
        } else if (c == '\f') {
            buf_append(out, "\\f");
        } else if (c == '\n') {
            buf_append(out, "\\n");
        } else if (c == '\r') {
            buf_append(out, "\\r");
        } else if (c == '\t') {
            buf_append(out, "\\t");
        } else if (c < 0x20) {
            char esc[7];
            snprintf(esc, sizeof(esc), "\\u00%02x", c);
            buf_append(out, esc);
        } else {
            buf_append_char(out, *p);
        }
    }
    buf_append_char(out, '"');
}

/* Extract the string value of the named field in a flat JSON object line.
 * field must be the exact key string (no quotes).
 * Returns a malloc'd string or NULL. */
static char *json_extract_str(const char *json, const char *field) {
    char needle[256];
    snprintf(needle, sizeof(needle), "\"%s\"", field);

    const char *p = strstr(json, needle);
    if (!p) return NULL;

    p += strlen(needle);
    /* skip : and whitespace */
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (*p != '"') return NULL;
    p++; /* skip opening quote */

    Buf result;
    buf_init(&result);

    while (*p && *p != '"') {
        if (*p == '\\' && *(p + 1)) {
            p++;
            switch (*p) {
                case '"':  buf_append_char(&result, '"');  break;
                case '\\': buf_append_char(&result, '\\'); break;
                case 'b':  buf_append_char(&result, '\b'); break;
                case 'f':  buf_append_char(&result, '\f'); break;
                case 'n':  buf_append_char(&result, '\n'); break;
                case 'r':  buf_append_char(&result, '\r'); break;
                case 't':  buf_append_char(&result, '\t'); break;
                case 'u': {
                    /* Minimal 4-hex decode for BMP range */
                    if (*(p+1) && *(p+2) && *(p+3) && *(p+4)) {
                        unsigned int cp = 0;
                        sscanf(p + 1, "%4x", &cp);
                        p += 4;
                        if (cp < 0x80) {
                            buf_append_char(&result, (char)cp);
                        } else if (cp < 0x800) {
                            buf_append_char(&result, (char)(0xC0 | (cp >> 6)));
                            buf_append_char(&result, (char)(0x80 | (cp & 0x3F)));
                        } else {
                            buf_append_char(&result, (char)(0xE0 | (cp >> 12)));
                            buf_append_char(&result, (char)(0x80 | ((cp >> 6) & 0x3F)));
                            buf_append_char(&result, (char)(0x80 | (cp & 0x3F)));
                        }
                    }
                    break;
                }
                default: buf_append_char(&result, *p); break;
            }
        } else {
            buf_append_char(&result, *p);
        }
        p++;
    }

    char *s = result.data;
    result.data = NULL;
    buf_free(&result);
    return s;
}

/* Extract integer id from JSON line (may be string or number). */
static char *json_extract_id(const char *json) {
    const char *p = strstr(json, "\"id\"");
    if (!p) return NULL;
    p += 4;
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (!*p || *p == '}') return NULL;

    Buf b;
    buf_init(&b);

    /* Collect up to next ',' or '}', trimming whitespace */
    while (*p && *p != ',' && *p != '}') {
        buf_append_char(&b, *p);
        p++;
    }
    /* Trim trailing whitespace */
    while (b.len > 0 && (b.data[b.len - 1] == ' ' || b.data[b.len - 1] == '\t'))
        b.data[--b.len] = '\0';

    char *s = b.data;
    b.data = NULL;
    buf_free(&b);
    return s;
}

/* Extract "method" field value (unquoted result string). */
static char *json_extract_method(const char *json) {
    return json_extract_str(json, "method");
}

/* -----------------------------------------------------------------------
 * Response writers
 * ----------------------------------------------------------------------- */

static void send_response(const char *id, const char *result_json) {
    /* Output: {"jsonrpc":"2.0","id":<id>,"result":<result>} */
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":%s}\n", id, result_json);
    fflush(stdout);
}

static void send_error(const char *id, int code, const char *message) {
    Buf b;
    buf_init(&b);
    buf_append(&b, "{\"code\":");
    char cstr[32];
    snprintf(cstr, sizeof(cstr), "%d", code);
    buf_append(&b, cstr);
    buf_append(&b, ",\"message\":");
    json_escape_str(&b, message);
    buf_append(&b, "}");

    const char *safe_id = (id && *id) ? id : "null";
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":%s}\n", safe_id, b.data);
    fflush(stdout);

    buf_free(&b);
}

static void handle_initialize(const char *id) {
    /* Per wire shape: {name, version, capabilities:["parse"]} */
    send_response(id,
        "{\"capabilities\":[\"parse\"],"
        "\"name\":\"provekit-lsp-c\","
        "\"version\":\"0.1.0\"}");
}

static void add_contract_decl(pk_c_lift_result *result, const char *name) {
    Buf decl;
    buf_init(&decl);
    buf_append(&decl, "{\"kind\":\"contract\",\"name\":");
    json_escape_str(&decl, name);
    buf_append(&decl, ",\"outBinding\":\"out\"}");
    pk_c_lift_result_add_declaration(result, decl.data);
    buf_free(&decl);
}

static void handle_parse(const char *id, const char *json_line) {
    char *path = json_extract_str(json_line, "path");
    char *source = json_extract_str(json_line, "source");

    if (!source) {
        free(path);
        send_error(id, -32602, "parse: missing params.source");
        return;
    }

    pk_c_source_facts *facts = pk_c_parse_source(path ? path : "lsp-document.c", source);
    free(source);
    free(path);

    pk_c_lift_result *result_obj = pk_c_lift_result_new();
    if (!result_obj) {
        pk_c_source_facts_free(facts);
        send_error(id, -32603, "parse: out of memory");
        return;
    }

    if (facts) {
        for (size_t i = 0; i < facts->n_functions; i++) {
            if (facts->functions[i].has_contract_annotation) {
                add_contract_decl(result_obj, facts->functions[i].name);
            }
        }
        if (facts->extraction_result) {
            for (size_t i = 0; i < facts->extraction_result->diagnostics.len; i++) {
                pk_c_lift_result_add_diagnostic(
                    result_obj,
                    facts->extraction_result->diagnostics.items[i]);
            }
        }
        pk_c_source_facts_free(facts);
    }

    char *result_json = pk_c_lift_result_to_json(result_obj);
    if (!result_json) {
        pk_c_lift_result_free(result_obj);
        send_error(id, -32603, "parse: out of memory");
        return;
    }
    send_response(id, result_json);
    free(result_json);
    pk_c_lift_result_free(result_obj);
}

static void handle_shutdown(const char *id) {
    send_response(id, "null");
}

/* -----------------------------------------------------------------------
 * Main NDJSON dispatcher
 * ----------------------------------------------------------------------- */

int main(int argc, char **argv) {
    int rpc_mode = 0;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--rpc") == 0) {
            rpc_mode = 1;
        }
    }

    if (!rpc_mode) {
        fprintf(stderr, "Usage: provekit-lsp-c --rpc\n");
        fprintf(stderr, "  Speaks provekit-lsp-plugin/1 NDJSON over stdin/stdout.\n");
        return 1;
    }

    char *line = NULL;
    size_t line_cap = 0;
    ssize_t line_len;

    while ((line_len = getline(&line, &line_cap, stdin)) != -1) {
        /* Strip trailing newline. */
        while (line_len > 0 && (line[line_len - 1] == '\n' || line[line_len - 1] == '\r')) {
            line[--line_len] = '\0';
        }
        if (line_len == 0) continue;

        char *method = json_extract_method(line);
        char *id     = json_extract_id(line);
        const char *safe_id = (id && *id) ? id : "null";

        if (!method) {
            send_error(safe_id, -32700, "parse error: could not extract method");
        } else if (strcmp(method, "initialize") == 0) {
            handle_initialize(safe_id);
        } else if (strcmp(method, "parse") == 0) {
            handle_parse(safe_id, line);
        } else if (strcmp(method, "shutdown") == 0) {
            handle_shutdown(safe_id);
            free(method);
            free(id);
            break;
        } else if (strcmp(method, "exit") == 0) {
            free(method);
            free(id);
            break;
        } else {
            Buf msg;
            buf_init(&msg);
            buf_append(&msg, "unknown method: ");
            buf_append(&msg, method);
            send_error(safe_id, -32601, msg.data);
            buf_free(&msg);
        }

        free(method);
        free(id);
    }

    free(line);
    return 0;
}
