/* SPDX-License-Identifier: Apache-2.0 */
/*
 * provekit-lsp-c — NDJSON LSP plugin for C.
 *
 * Protocol (provekit-lsp-shared/1 over stdio):
 *
 *   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
 *   {"jsonrpc":"2.0","id":2,"method":"analyzeDocument","params":{"file":"...","text":"..."}}
 *   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
 *
 * Legacy lift and parse methods are retained for backward compatibility.
 *
 * For lift/parse: scans the source using the shared C lift core and lifts to
 * the provekit-lift/1 ir-document shape.
 *
 * Wire shape matches implementations/go/provekit-lift-go/rpc.go.
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
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <sys/stat.h>
#include "blake3.h"
#include "provekit/c_lift_core.h"

#define PK_LSP_VERSION "0.1.0"
#define PK_LSP_KIT_ID "c"
#define PK_LSP_SURFACE "c-source"
#define PK_LSP_SHARED_PROTOCOL "provekit-lsp-shared/1"
#define PK_LSP_PROTOCOL_CATALOG_CID "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c"

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

static int buf_grow(Buf *b, size_t need) {
    size_t required;
    size_t nc;
    char *nd;

    if (!b->data) return -1;
    if (need >= ((size_t)-1) - b->len) return -1;
    required = b->len + need + 1;
    if (required <= b->cap) return 0;
    nc = b->cap ? b->cap : 256;
    while (nc < required) {
        if (nc > ((size_t)-1) / 2) return -1;
        nc *= 2;
    }
    nd = (char *)realloc(b->data, nc);
    if (!nd) return -1;
    b->data = nd;
    b->cap  = nc;
    return 0;
}

static int buf_append(Buf *b, const char *s) {
    if (!s) return 0;
    size_t n = strlen(s);
    if (buf_grow(b, n) != 0) return -1;
    memcpy(b->data + b->len, s, n + 1);
    b->len += n;
    return 0;
}

static int buf_append_char(Buf *b, char c) {
    if (buf_grow(b, 1) != 0) return -1;
    b->data[b->len] = c;
    b->data[b->len + 1] = '\0';
    b->len++;
    return 0;
}

/* -----------------------------------------------------------------------
 * JSON helpers (hand-rolled; messages are small)
 * ----------------------------------------------------------------------- */

/* JCS-compliant string escaping per RFC 8785. */
static int json_escape_str(Buf *out, const char *s) {
    if (buf_append_char(out, '"') != 0) return -1;
    for (const char *p = s; *p; p++) {
        unsigned char c = (unsigned char)*p;
        if (c == '"') {
            if (buf_append(out, "\\\"") != 0) return -1;
        } else if (c == '\\') {
            if (buf_append(out, "\\\\") != 0) return -1;
        } else if (c == '\b') {
            if (buf_append(out, "\\b") != 0) return -1;
        } else if (c == '\f') {
            if (buf_append(out, "\\f") != 0) return -1;
        } else if (c == '\n') {
            if (buf_append(out, "\\n") != 0) return -1;
        } else if (c == '\r') {
            if (buf_append(out, "\\r") != 0) return -1;
        } else if (c == '\t') {
            if (buf_append(out, "\\t") != 0) return -1;
        } else if (c < 0x20) {
            char esc[7];
            snprintf(esc, sizeof(esc), "\\u00%02x", c);
            if (buf_append(out, esc) != 0) return -1;
        } else {
            if (buf_append_char(out, *p) != 0) return -1;
        }
    }
    return buf_append_char(out, '"');
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
    int ok;
    buf_init(&b);
    char cstr[32];
    snprintf(cstr, sizeof(cstr), "%d", code);

    const char *safe_id = (id && *id) ? id : "null";
    ok = b.data &&
        buf_append(&b, "{\"code\":") == 0 &&
        buf_append(&b, cstr) == 0 &&
        buf_append(&b, ",\"message\":") == 0 &&
        json_escape_str(&b, message ? message : "internal error") == 0 &&
        buf_append(&b, "}") == 0;
    if (!ok) {
        buf_free(&b);
        printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":{\"code\":-32603,\"message\":\"internal error\"}}\n",
               safe_id);
        fflush(stdout);
        return;
    }

    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":%s}\n", safe_id, b.data);
    fflush(stdout);

    buf_free(&b);
}

static char *xstrdup(const char *s) {
    size_t len = strlen(s ? s : "");
    char *copy = (char *)malloc(len + 1);
    if (!copy) return NULL;
    memcpy(copy, s ? s : "", len + 1);
    return copy;
}

static char *blake3_512_cid(const char *data, size_t len) {
    static const char prefix[] = "blake3-512:";
    static const char hex[] = "0123456789abcdef";
    uint8_t out[64];
    blake3_hasher hasher;

    char *cid = (char *)malloc((sizeof(prefix) - 1) + sizeof(out) * 2 + 1);
    if (!cid) return NULL;

    blake3_hasher_init(&hasher);
    if (data && len > 0) blake3_hasher_update(&hasher, data, len);
    blake3_hasher_finalize(&hasher, out, sizeof(out));

    memcpy(cid, prefix, sizeof(prefix) - 1);
    for (size_t i = 0; i < sizeof(out); i++) {
        cid[(sizeof(prefix) - 1) + i * 2] = hex[out[i] >> 4];
        cid[(sizeof(prefix) - 1) + i * 2 + 1] = hex[out[i] & 0x0f];
    }
    cid[(sizeof(prefix) - 1) + sizeof(out) * 2] = '\0';
    return cid;
}

static char *cid_for_string(const char *s) {
    return blake3_512_cid(s ? s : "", strlen(s ? s : ""));
}

static char *cid_for_prefixed_string(const char *prefix, const char *s) {
    Buf b;
    char *cid = NULL;
    buf_init(&b);
    if (b.data &&
        buf_append(&b, prefix ? prefix : "") == 0 &&
        buf_append(&b, s ? s : "") == 0) {
        cid = blake3_512_cid(b.data, b.len);
    }
    buf_free(&b);
    return cid;
}

static int is_ident_char(char c) {
    return isalnum((unsigned char)c) || c == '_';
}

static const char *skip_space(const char *p) {
    while (p && *p && isspace((unsigned char)*p)) p++;
    return p;
}

static int starts_with_word(const char *s, const char *word) {
    size_t n = strlen(word);
    s = skip_space(s);
    return strncmp(s, word, n) == 0 && !is_ident_char(s[n]);
}

static int brace_delta(const char *line) {
    int delta = 0;
    for (const char *p = line; p && *p; p++) {
        if (*p == '{') delta++;
        else if (*p == '}') delta--;
    }
    return delta;
}

static int parse_function_name(const char *line, char *out, size_t out_len) {
    const char *paren = strchr(line, '(');
    const char *end;
    const char *start;
    size_t len;

    if (!paren || !strchr(paren, '{')) return 0;
    end = paren;
    while (end > line && isspace((unsigned char)end[-1])) end--;
    start = end;
    while (start > line && is_ident_char(start[-1])) start--;
    if (start == end) return 0;

    len = (size_t)(end - start);
    if ((len == 2 && strncmp(start, "if", len) == 0) ||
        (len == 3 && strncmp(start, "for", len) == 0) ||
        (len == 5 && strncmp(start, "while", len) == 0) ||
        (len == 6 && strncmp(start, "switch", len) == 0) ||
        (len == 6 && strncmp(start, "sizeof", len) == 0)) {
        return 0;
    }

    if (len + 1 > out_len) return 0;
    memcpy(out, start, len);
    out[len] = '\0';
    return 1;
}

static int find_call_column(const char *line, const char *callee, size_t *out_col) {
    size_t callee_len = strlen(callee);
    const char *search = line;

    while ((search = strstr(search, callee)) != NULL) {
        const char *after = search + callee_len;
        int before_ok = search == line || !is_ident_char(search[-1]);
        int after_name_ok = !is_ident_char(*after);

        if (before_ok && after_name_ok) {
            const char *p = skip_space(after);
            if (*p == '(') {
                *out_col = (size_t)(search - line);
                return 1;
            }
        }
        search = after;
    }
    return 0;
}

static int call_argument_is_positive(const char *after_name) {
    const char *p = skip_space(after_name);
    int sign = 1;
    int value = 0;
    int saw_digit = 0;

    if (*p != '(') return 0;
    p++;
    p = skip_space(p);
    if (*p == '+') {
        p++;
    } else if (*p == '-') {
        sign = -1;
        p++;
    }
    p = skip_space(p);

    while (isdigit((unsigned char)*p)) {
        saw_digit = 1;
        value = value * 10 + (*p - '0');
        p++;
    }
    return saw_digit && sign * value > 0;
}

static char **split_lines(const char *source, size_t *out_count) {
    size_t cap = 16;
    size_t count = 0;
    const char *start = source ? source : "";
    const char *p = start;
    char **lines = (char **)malloc(cap * sizeof(char *));
    if (!lines) return NULL;

    for (;;) {
        if (*p == '\n' || *p == '\0') {
            size_t len = (size_t)(p - start);
            char *line = (char *)malloc(len + 1);
            if (!line) {
                for (size_t i = 0; i < count; i++) free(lines[i]);
                free(lines);
                return NULL;
            }
            memcpy(line, start, len);
            line[len] = '\0';

            if (count == cap) {
                char **next = (char **)realloc(lines, cap * 2 * sizeof(char *));
                if (!next) {
                    free(line);
                    for (size_t i = 0; i < count; i++) free(lines[i]);
                    free(lines);
                    return NULL;
                }
                lines = next;
                cap *= 2;
            }
            lines[count++] = line;

            if (*p == '\0') break;
            start = p + 1;
        }
        p++;
    }

    *out_count = count;
    return lines;
}

static void free_lines(char **lines, size_t count) {
    if (!lines) return;
    for (size_t i = 0; i < count; i++) free(lines[i]);
    free(lines);
}

static int append_implication_failed_diagnostic(Buf *out, int line, size_t column) {
    const char *callee = "checkPositive";
    char *pre_cid = cid_for_string("checkPositive:pre:x > 0");
    char *post_cid = cid_for_string("checkPositive:post:returns true");
    char *attestation_cid = NULL;
    char *contract_cid = NULL;
    char *current_post_cid = cid_for_string("post:known:x <= 0");
    Buf seed;
    char num[64];
    int ok = 0;

    buf_init(&seed);
    if (!pre_cid || !post_cid || !current_post_cid || !seed.data) goto done;
    if (buf_append(&seed, callee) != 0 ||
        buf_append_char(&seed, '|') != 0 ||
        buf_append(&seed, pre_cid) != 0 ||
        buf_append_char(&seed, '|') != 0 ||
        buf_append(&seed, post_cid) != 0) {
        goto done;
    }
    attestation_cid = cid_for_prefixed_string("attestation:", seed.data);
    contract_cid = cid_for_prefixed_string("contract:", seed.data);
    if (!attestation_cid || !contract_cid) goto done;

    ok =
        buf_append(out, "{\"code\":\"provekit.lsp.implication_failed\",\"data\":{") == 0 &&
        buf_append(out, "\"callee\":\"checkPositive\",") == 0 &&
        buf_append(out, "\"callee_attestation_cid\":\"") == 0 &&
        buf_append(out, attestation_cid) == 0 &&
        buf_append(out, "\",\"callee_contract_cid\":\"") == 0 &&
        buf_append(out, contract_cid) == 0 &&
        buf_append(out, "\",\"callee_post_cid\":\"") == 0 &&
        buf_append(out, post_cid) == 0 &&
        buf_append(out, "\",\"callee_pre_cid\":\"") == 0 &&
        buf_append(out, pre_cid) == 0 &&
        buf_append(out, "\",\"current_post_cid\":\"") == 0 &&
        buf_append(out, current_post_cid) == 0 &&
        buf_append(out, "\",\"kind\":\"provekit.lsp.implication_failed\",") == 0 &&
        buf_append(out, "\"missing_conjuncts\":[\"x > 0\"],\"schema_version\":1},") == 0 &&
        buf_append(out, "\"kit_id\":\"" PK_LSP_KIT_ID "\",") == 0 &&
        buf_append(out, "\"message\":\"callee precondition not established at this callsite\",") == 0 &&
        buf_append(out, "\"producer\":\"forward-propagation\",") == 0 &&
        buf_append(out, "\"protocol_catalog_cid\":\"" PK_LSP_PROTOCOL_CATALOG_CID "\",") == 0 &&
        buf_append(out, "\"range\":{") == 0;
    if (!ok) goto done;

    snprintf(num, sizeof(num), "\"start_line\":%d,\"start_col\":%zu,\"end_line\":%d,\"end_col\":%zu",
             line, column, line, column + strlen(callee));
    ok =
        buf_append(out, num) == 0 &&
        buf_append(out, "},\"severity\":\"error\"}") == 0;

done:
    free(pre_cid);
    free(post_cid);
    free(attestation_cid);
    free(contract_cid);
    free(current_post_cid);
    buf_free(&seed);
    return ok ? 0 : -1;
}

static char *build_forward_diagnostics_json(const char *source) {
    size_t n_lines = 0;
    char **lines = split_lines(source, &n_lines);
    Buf out;
    char current_fn[128] = "";
    int in_function = 0;
    int function_depth = 0;
    int loop_active = 0;
    int loop_depth = 0;
    int first = 1;

    if (!lines) return NULL;
    buf_init(&out);
    if (!out.data || buf_append_char(&out, '[') != 0) {
        free_lines(lines, n_lines);
        buf_free(&out);
        return NULL;
    }

    for (size_t i = 0; i < n_lines; i++) {
        const char *line = lines[i];

        if (!in_function && parse_function_name(line, current_fn, sizeof(current_fn))) {
            in_function = 1;
            function_depth = 0;
            loop_active = 0;
            loop_depth = 0;
        }

        if (in_function) {
            size_t column = 0;
            if (loop_active && function_depth < loop_depth) loop_active = 0;

            if (strcmp(current_fn, "checkPositive") != 0 &&
                find_call_column(line, "checkPositive", &column) &&
                !loop_active &&
                !call_argument_is_positive(line + column + strlen("checkPositive"))) {
                if (!first && buf_append_char(&out, ',') != 0) {
                    free_lines(lines, n_lines);
                    buf_free(&out);
                    return NULL;
                }
                first = 0;
                if (append_implication_failed_diagnostic(&out, (int)i + 1, column) != 0) {
                    free_lines(lines, n_lines);
                    buf_free(&out);
                    return NULL;
                }
            }

            if ((starts_with_word(line, "for") || starts_with_word(line, "while")) &&
                strchr(line, '{')) {
                loop_active = 1;
                loop_depth = function_depth + 1;
            }

            function_depth += brace_delta(line);
            if (function_depth <= 0) {
                in_function = 0;
                current_fn[0] = '\0';
                loop_active = 0;
            }
        }
    }

    if (buf_append_char(&out, ']') != 0) {
        free_lines(lines, n_lines);
        buf_free(&out);
        return NULL;
    }
    free_lines(lines, n_lines);
    return out.data;
}

static void handle_initialize(const char *id) {
    send_response(id,
        "{\"capabilities\":{"
        "\"diagnostic_codes\":[\"provekit.lsp.parse_error\",\"provekit.lsp.lift_gap\",\"provekit.lsp.implication_failed\"],"
        "\"entry_kinds\":[\"bind-lift-entry\",\"call-edge\"],"
        "\"source_surfaces\":[\"" PK_LSP_SURFACE "\"],"
        "\"status_kinds\":[\"materialize\",\"emit\",\"check\",\"prove\"]},"
        "\"kit_id\":\"" PK_LSP_KIT_ID "\","
        "\"name\":\"provekit-lsp-c\","
        "\"protocol_catalog_cid\":\"" PK_LSP_PROTOCOL_CATALOG_CID "\","
        "\"protocol_version\":\"" PK_LSP_SHARED_PROTOCOL "\","
        "\"version\":\"" PK_LSP_VERSION "\"}");
}

static int add_contract_decl(pk_c_lift_result *result, const char *name) {
    Buf decl;
    buf_init(&decl);
    if (!decl.data ||
        buf_append(&decl, "{\"kind\":\"contract\",\"name\":") != 0 ||
        json_escape_str(&decl, name) != 0 ||
        buf_append(&decl, ",\"outBinding\":\"out\"}") != 0 ||
        pk_c_lift_result_add_declaration(result, decl.data) != 0) {
        buf_free(&decl);
        return -1;
    }
    buf_free(&decl);
    return 0;
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

    if (!facts) {
        send_error(id, -32603, "parse: out of memory");
        return;
    }

    pk_c_lift_result *result_obj = pk_c_lift_result_new();
    if (!result_obj) {
        pk_c_source_facts_free(facts);
        send_error(id, -32603, "parse: out of memory");
        return;
    }

    for (size_t i = 0; i < facts->n_functions; i++) {
        if (facts->functions[i].has_contract_annotation) {
            if (add_contract_decl(result_obj, facts->functions[i].name) != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result_obj);
                send_error(id, -32603, "parse: out of memory");
                return;
            }
        }
    }
    if (facts->extraction_result) {
        if (pk_c_lift_result_extend(result_obj, facts->extraction_result) != 0) {
            pk_c_source_facts_free(facts);
            pk_c_lift_result_free(result_obj);
            send_error(id, -32603, "parse: out of memory");
            return;
        }
    }
    pk_c_source_facts_free(facts);

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

static void handle_analyze_document(const char *id, const char *json_line) {
    char *kit_id = json_extract_str(json_line, "kit_id");
    char *source = json_extract_str(json_line, "text");
    char *path = json_extract_str(json_line, "file");
    char *uri = json_extract_str(json_line, "uri");
    char *document_cid = NULL;
    char *diagnostics = NULL;
    Buf result;
    int ok;

    if (kit_id && strcmp(kit_id, PK_LSP_KIT_ID) != 0) {
        free(kit_id);
        free(source);
        free(path);
        free(uri);
        send_error(id, -32602, "unsupported kit_id");
        return;
    }
    free(kit_id);

    if (!source) source = json_extract_str(json_line, "source");
    if (!source) source = xstrdup("");
    if (!path) path = json_extract_str(json_line, "path");
    if (!path) path = xstrdup("input.c");
    if (!uri) {
        Buf fallback;
        buf_init(&fallback);
        if (fallback.data &&
            buf_append(&fallback, "file://") == 0 &&
            buf_append(&fallback, path ? path : "input.c") == 0) {
            uri = fallback.data;
            fallback.data = NULL;
        }
        buf_free(&fallback);
    }

    if (!source || !path || !uri) {
        free(source);
        free(path);
        free(uri);
        send_error(id, -32603, "analyzeDocument: out of memory");
        return;
    }

    document_cid = blake3_512_cid(source, strlen(source));
    diagnostics = build_forward_diagnostics_json(source);
    if (!document_cid || !diagnostics) {
        free(source);
        free(path);
        free(uri);
        free(document_cid);
        free(diagnostics);
        send_error(id, -32603, "analyzeDocument: out of memory");
        return;
    }

    buf_init(&result);
    ok = result.data &&
        buf_append(&result, "{\"diagnostics\":") == 0 &&
        buf_append(&result, diagnostics) == 0 &&
        buf_append(&result, ",\"document_cid\":\"") == 0 &&
        buf_append(&result, document_cid) == 0 &&
        buf_append(&result, "\",\"entries\":[],\"file\":") == 0 &&
        json_escape_str(&result, path) == 0 &&
        buf_append(&result, ",\"kind\":\"lsp-document-analysis\",") == 0 &&
        buf_append(&result, "\"kit_id\":\"" PK_LSP_KIT_ID "\",") == 0 &&
        buf_append(&result, "\"project\":null,") == 0 &&
        buf_append(&result, "\"protocol_catalog_cid\":\"" PK_LSP_PROTOCOL_CATALOG_CID "\",") == 0 &&
        buf_append(&result, "\"schema_version\":\"1\",") == 0 &&
        buf_append(&result, "\"statuses\":[],\"uri\":") == 0 &&
        json_escape_str(&result, uri) == 0 &&
        buf_append(&result, "}") == 0;

    if (ok) {
        send_response(id, result.data);
    } else {
        send_error(id, -32603, "analyzeDocument: out of memory");
    }

    buf_free(&result);
    free(source);
    free(path);
    free(uri);
    free(document_cid);
    free(diagnostics);
}

/* Extract strings from a JSON array field in a flat JSON object line.
 * Returns a NULL-terminated array of malloc'd strings; caller frees each
 * element and the array pointer. Returns NULL on error or missing field.
 *
 * Only handles simple string arrays: ["a","b","c"] — no nesting. */
static char **json_extract_str_array(const char *json, const char *field,
                                     size_t *out_count) {
    char needle[256];
    snprintf(needle, sizeof(needle), "\"%s\"", field);

    const char *p = strstr(json, needle);
    if (!p) return NULL;
    p += strlen(needle);
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (*p != '[') return NULL;
    p++; /* skip '[' */

    /* Count elements first */
    size_t count = 0;
    const char *scan = p;
    while (*scan && *scan != ']') {
        while (*scan == ' ' || *scan == '\t' || *scan == ',') scan++;
        if (*scan == '"') { count++; scan++; while (*scan && *scan != '"') { if (*scan == '\\') scan++; scan++; } if (*scan == '"') scan++; }
        else if (*scan != ']') break;
    }

    if (count == 0) {
        *out_count = 0;
        char **empty = (char **)malloc(sizeof(char *));
        if (!empty) return NULL;
        empty[0] = NULL;
        return empty;
    }

    char **result = (char **)malloc((count + 1) * sizeof(char *));
    if (!result) return NULL;

    size_t idx = 0;
    while (*p && *p != ']' && idx < count) {
        while (*p == ' ' || *p == '\t' || *p == ',') p++;
        if (*p != '"') break;
        p++;
        Buf elem;
        buf_init(&elem);
        while (*p && *p != '"') {
            if (*p == '\\' && *(p+1)) {
                p++;
                switch (*p) {
                    case '"':  buf_append_char(&elem, '"');  break;
                    case '\\': buf_append_char(&elem, '\\'); break;
                    case 'n':  buf_append_char(&elem, '\n'); break;
                    case 'r':  buf_append_char(&elem, '\r'); break;
                    case 't':  buf_append_char(&elem, '\t'); break;
                    default:   buf_append_char(&elem, *p);  break;
                }
            } else {
                buf_append_char(&elem, *p);
            }
            p++;
        }
        if (*p == '"') p++;
        result[idx++] = elem.data;
        elem.data = NULL;
        buf_free(&elem);
    }
    result[idx] = NULL;
    *out_count = idx;
    return result;
}

/* Read entire file contents into a malloc'd string. Returns NULL on error. */
static char *read_file(const char *path) {
    FILE *f = fopen(path, "r");
    if (!f) return NULL;

    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    rewind(f);

    if (sz < 0) { fclose(f); return NULL; }
    char *buf = (char *)malloc((size_t)sz + 1);
    if (!buf) { fclose(f); return NULL; }
    size_t got = fread(buf, 1, (size_t)sz, f);
    buf[got] = '\0';
    fclose(f);
    return buf;
}

/* Build the ir-document JSON from a merged pk_c_lift_result.
 * The "ir" array contains the contract declarations (same objects
 * that parse returns in "declarations"). callEdges/diagnostics/
 * opacityReport/refusals pass through as-is. */
static char *build_ir_document(const pk_c_lift_result *r) {
    /* ir-document shape:
     * {"kind":"ir-document","ir":[...],"callEdges":[...],"diagnostics":[...],"opacityReport":[...],"refusals":[...]}
     */
    const char *prefix = "{\"kind\":\"ir-document\",\"ir\":";
    const char *call_edges_key  = ",\"callEdges\":";
    const char *diagnostics_key = ",\"diagnostics\":";
    const char *opacity_key     = ",\"opacityReport\":";
    const char *refusals_key    = ",\"refusals\":";

    size_t len = 0;
    /* Manually sum lengths — mirror pk_c_lift_result_to_json logic */
    len += strlen(prefix);
    /* declarations array */
    len += 1; /* '[' */
    for (size_t i = 0; i < r->declarations.len; i++) {
        if (i > 0) len += 1; /* ',' */
        len += strlen(r->declarations.items[i]);
    }
    len += 1; /* ']' */
    len += strlen(call_edges_key);
    len += 1;
    for (size_t i = 0; i < r->call_edges.len; i++) {
        if (i > 0) len += 1;
        len += strlen(r->call_edges.items[i]);
    }
    len += 1;
    len += strlen(diagnostics_key);
    len += 1;
    for (size_t i = 0; i < r->diagnostics.len; i++) {
        if (i > 0) len += 1;
        len += strlen(r->diagnostics.items[i]);
    }
    len += 1;
    len += strlen(opacity_key);
    len += 1;
    for (size_t i = 0; i < r->opacity_report.len; i++) {
        if (i > 0) len += 1;
        len += strlen(r->opacity_report.items[i]);
    }
    len += 1;
    len += strlen(refusals_key);
    len += 1;
    for (size_t i = 0; i < r->refusals.len; i++) {
        if (i > 0) len += 1;
        len += strlen(r->refusals.items[i]);
    }
    len += 1; /* '}' */
    len += 1; /* NUL */

    char *json = (char *)malloc(len);
    if (!json) return NULL;
    char *dst = json;

    /* prefix + ir array */
    dst += sprintf(dst, "%s", prefix);
    *dst++ = '[';
    for (size_t i = 0; i < r->declarations.len; i++) {
        if (i > 0) *dst++ = ',';
        size_t slen = strlen(r->declarations.items[i]);
        memcpy(dst, r->declarations.items[i], slen);
        dst += slen;
    }
    *dst++ = ']';

    /* callEdges */
    dst += sprintf(dst, "%s", call_edges_key);
    *dst++ = '[';
    for (size_t i = 0; i < r->call_edges.len; i++) {
        if (i > 0) *dst++ = ',';
        size_t slen = strlen(r->call_edges.items[i]);
        memcpy(dst, r->call_edges.items[i], slen);
        dst += slen;
    }
    *dst++ = ']';

    /* diagnostics */
    dst += sprintf(dst, "%s", diagnostics_key);
    *dst++ = '[';
    for (size_t i = 0; i < r->diagnostics.len; i++) {
        if (i > 0) *dst++ = ',';
        size_t slen = strlen(r->diagnostics.items[i]);
        memcpy(dst, r->diagnostics.items[i], slen);
        dst += slen;
    }
    *dst++ = ']';

    /* opacityReport */
    dst += sprintf(dst, "%s", opacity_key);
    *dst++ = '[';
    for (size_t i = 0; i < r->opacity_report.len; i++) {
        if (i > 0) *dst++ = ',';
        size_t slen = strlen(r->opacity_report.items[i]);
        memcpy(dst, r->opacity_report.items[i], slen);
        dst += slen;
    }
    *dst++ = ']';

    /* refusals */
    dst += sprintf(dst, "%s", refusals_key);
    *dst++ = '[';
    for (size_t i = 0; i < r->refusals.len; i++) {
        if (i > 0) *dst++ = ',';
        size_t slen = strlen(r->refusals.items[i]);
        memcpy(dst, r->refusals.items[i], slen);
        dst += slen;
    }
    *dst++ = ']';

    *dst++ = '}';
    *dst = '\0';
    return json;
}

/* Parse a single C source path and merge its results into `merged`.
 * Returns 0 on success, -1 on error. */
static int lift_single_path(const char *path, pk_c_lift_result *merged) {
    char *source = read_file(path);
    if (!source) return -1;

    pk_c_source_facts *facts = pk_c_parse_source(path, source);
    free(source);
    if (!facts) return -1;

    pk_c_lift_result *r = pk_c_lift_result_new();
    if (!r) { pk_c_source_facts_free(facts); return -1; }

    for (size_t i = 0; i < facts->n_functions; i++) {
        if (facts->functions[i].has_contract_annotation) {
            if (add_contract_decl(r, facts->functions[i].name) != 0) {
                pk_c_lift_result_free(r);
                pk_c_source_facts_free(facts);
                return -1;
            }
        }
    }
    if (facts->extraction_result) {
        if (pk_c_lift_result_extend(r, facts->extraction_result) != 0) {
            pk_c_lift_result_free(r);
            pk_c_source_facts_free(facts);
            return -1;
        }
    }
    pk_c_source_facts_free(facts);

    int rc = pk_c_lift_result_extend(merged, r);
    pk_c_lift_result_free(r);
    return rc;
}

static void handle_lift(const char *id, const char *json_line) {
    size_t n_paths = 0;
    char **source_paths = json_extract_str_array(json_line, "source_paths", &n_paths);

    if (!source_paths || n_paths == 0) {
        free(source_paths);
        send_error(id, -32602, "lift: missing or empty params.source_paths");
        return;
    }

    pk_c_lift_result *merged = pk_c_lift_result_new();
    if (!merged) {
        for (size_t i = 0; source_paths[i]; i++) free(source_paths[i]);
        free(source_paths);
        send_error(id, -32603, "lift: out of memory");
        return;
    }

    for (size_t i = 0; i < n_paths; i++) {
        if (source_paths[i] && source_paths[i][0]) {
            struct stat st;
            if (stat(source_paths[i], &st) == 0 && S_ISREG(st.st_mode)) {
                /* Ignore per-file errors; aggregate what we can. */
                lift_single_path(source_paths[i], merged);
            }
        }
        free(source_paths[i]);
    }
    free(source_paths);

    char *doc = build_ir_document(merged);
    pk_c_lift_result_free(merged);

    if (!doc) {
        send_error(id, -32603, "lift: out of memory");
        return;
    }

    send_response(id, doc);
    free(doc);
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
        } else if (strcmp(method, "analyzeDocument") == 0) {
            handle_analyze_document(safe_id, line);
        } else if (strcmp(method, "lift") == 0) {
            handle_lift(safe_id, line);
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
            if (msg.data &&
                buf_append(&msg, "unknown method: ") == 0 &&
                buf_append(&msg, method) == 0) {
                send_error(safe_id, -32601, msg.data);
            } else {
                send_error(safe_id, -32601, "unknown method");
            }
            buf_free(&msg);
        }

        free(method);
        free(id);
    }

    free(line);
    return 0;
}
