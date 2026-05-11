#define _POSIX_C_SOURCE 200809L

#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#include "provekit/c_lift_core.h"

pk_c_lift_result *pk_c_walk_lift_source(const char *path, const char *source);
pk_c_lift_result *pk_c_walk_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options);

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} Buf;

typedef struct {
    char **items;
    size_t len;
} StringArray;

static void string_array_free(StringArray *arr);

typedef struct {
    pk_c_parse_options options;
    StringArray clang_args;
    pk_c_compile_context *compile_context;
    char *parse_backend;
    char *compile_context_kind;
    char *workspace_root;
    char *compile_command;
    char *target_triple;
    int resolve_kernel_context;
} ParseRequestOptions;

typedef struct {
    Buf declarations;
    Buf ir;
    Buf call_edges;
    Buf diagnostics;
    Buf opacity_report;
    Buf refusals;
    size_t declaration_count;
    size_t ir_count;
    size_t call_edge_count;
    size_t diagnostic_count;
    size_t opacity_count;
    size_t refusal_count;
    char error[512];
} LiftAccumulator;

static void buf_init(Buf *b) {
    b->len = 0;
    b->cap = 256;
    b->data = malloc(b->cap);
    if (b->data) {
        b->data[0] = '\0';
    } else {
        b->cap = 0;
    }
}

static void buf_free(Buf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int buf_grow(Buf *b, size_t need) {
    size_t next = b->cap ? b->cap : 256;
    char *data;

    while (next < b->len + need + 1) {
        if (next > ((size_t)-1) / 2) {
            return -1;
        }
        next *= 2;
    }

    data = realloc(b->data, next);
    if (!data) {
        return -1;
    }

    b->data = data;
    b->cap = next;
    return 0;
}

static int buf_append_n(Buf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) {
        return -1;
    }
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
    (void)buf_append_char(out, '"');
    for (const unsigned char *p = (const unsigned char *)s; p && *p; p++) {
        switch (*p) {
        case '"':
            (void)buf_append(out, "\\\"");
            break;
        case '\\':
            (void)buf_append(out, "\\\\");
            break;
        case '\n':
            (void)buf_append(out, "\\n");
            break;
        case '\r':
            (void)buf_append(out, "\\r");
            break;
        case '\t':
            (void)buf_append(out, "\\t");
            break;
        default:
            if (*p < 0x20) {
                char esc[7];
                (void)snprintf(esc, sizeof(esc), "\\u00%02x", *p);
                (void)buf_append(out, esc);
            } else {
                (void)buf_append_char(out, (char)*p);
            }
            break;
        }
    }
    (void)buf_append_char(out, '"');
}

static char *str_dup(const char *s) {
    size_t n = strlen(s);
    char *out = malloc(n + 1);
    if (!out) {
        return NULL;
    }
    memcpy(out, s, n + 1);
    return out;
}

static void json_skip_ws(const char **p) {
    while (**p == ' ' || **p == '\t' || **p == '\r' || **p == '\n') {
        (*p)++;
    }
}

static int json_parse_value(const char **p);

static int json_hex_value(char c) {
    if (c >= '0' && c <= '9') {
        return c - '0';
    }
    if (c >= 'a' && c <= 'f') {
        return c - 'a' + 10;
    }
    if (c >= 'A' && c <= 'F') {
        return c - 'A' + 10;
    }
    return -1;
}

static int json_decode_hex4(const char *p, unsigned *out) {
    unsigned value = 0;

    for (int i = 0; i < 4; i++) {
        int digit = json_hex_value(p[i]);
        if (digit < 0) {
            return -1;
        }
        value = (value << 4) | (unsigned)digit;
    }
    *out = value;
    return 0;
}

static int json_append_utf8(char *out, size_t cap, size_t *len, unsigned codepoint) {
    if (codepoint == 0 || codepoint > 0x10ffffu ||
        (codepoint >= 0xd800u && codepoint <= 0xdfffu)) {
        return -1;
    }
    if (codepoint < 0x80u) {
        if (*len + 1 >= cap) {
            return -1;
        }
        out[(*len)++] = (char)codepoint;
    } else if (codepoint < 0x800u) {
        if (*len + 2 >= cap) {
            return -1;
        }
        out[(*len)++] = (char)(0xc0u | (codepoint >> 6));
        out[(*len)++] = (char)(0x80u | (codepoint & 0x3fu));
    } else if (codepoint < 0x10000u) {
        if (*len + 3 >= cap) {
            return -1;
        }
        out[(*len)++] = (char)(0xe0u | (codepoint >> 12));
        out[(*len)++] = (char)(0x80u | ((codepoint >> 6) & 0x3fu));
        out[(*len)++] = (char)(0x80u | (codepoint & 0x3fu));
    } else {
        if (*len + 4 >= cap) {
            return -1;
        }
        out[(*len)++] = (char)(0xf0u | (codepoint >> 18));
        out[(*len)++] = (char)(0x80u | ((codepoint >> 12) & 0x3fu));
        out[(*len)++] = (char)(0x80u | ((codepoint >> 6) & 0x3fu));
        out[(*len)++] = (char)(0x80u | (codepoint & 0x3fu));
    }
    return 0;
}

static char *decode_json_string(const char *start, const char **end_out) {
    size_t cap = strlen(start) + 1;
    char *out = malloc(cap);
    size_t len = 0;
    const char *p = start;

    if (!out) {
        return NULL;
    }

    while (*p && *p != '"') {
        if (*p == '\\') {
            p++;
            switch (*p) {
            case 'b':
                out[len++] = '\b';
                p++;
                break;
            case 'f':
                out[len++] = '\f';
                p++;
                break;
            case 'n':
                out[len++] = '\n';
                p++;
                break;
            case 'r':
                out[len++] = '\r';
                p++;
                break;
            case 't':
                out[len++] = '\t';
                p++;
                break;
            case '"':
                out[len++] = '"';
                p++;
                break;
            case '\\':
                out[len++] = '\\';
                p++;
                break;
            case '/':
                out[len++] = '/';
                p++;
                break;
            case 'u': {
                unsigned codepoint;

                p++;
                if (json_decode_hex4(p, &codepoint) != 0) {
                    free(out);
                    return NULL;
                }
                p += 4;
                if (codepoint >= 0xd800u && codepoint <= 0xdbffu) {
                    unsigned low;

                    if (p[0] != '\\' || p[1] != 'u' ||
                        json_decode_hex4(p + 2, &low) != 0 ||
                        low < 0xdc00u || low > 0xdfffu) {
                        free(out);
                        return NULL;
                    }
                    p += 6;
                    codepoint = 0x10000u + (((codepoint - 0xd800u) << 10) |
                        (low - 0xdc00u));
                }
                if (json_append_utf8(out, cap, &len, codepoint) != 0) {
                    free(out);
                    return NULL;
                }
                break;
            }
            case '\0':
                out[len] = '\0';
                if (end_out) {
                    *end_out = p;
                }
                return out;
            default:
                free(out);
                return NULL;
            }
        } else {
            out[len++] = *p++;
        }
    }

    out[len] = '\0';
    if (end_out) {
        *end_out = p;
    }
    return out;
}

static const char *json_find_object_value(const char *json, const char *field) {
    const char *p = json;

    json_skip_ws(&p);
    if (*p != '{') {
        return NULL;
    }
    p++;
    json_skip_ws(&p);
    if (*p == '}') {
        return NULL;
    }
    for (;;) {
        const char *end = NULL;
        char *key;
        int matched;

        if (*p != '"') {
            return NULL;
        }
        key = decode_json_string(p + 1, &end);
        if (key == NULL || end == NULL || *end != '"') {
            free(key);
            return NULL;
        }
        p = end + 1;
        json_skip_ws(&p);
        if (*p != ':') {
            free(key);
            return NULL;
        }
        p++;
        json_skip_ws(&p);
        matched = strcmp(key, field) == 0;
        free(key);
        if (matched) {
            return p;
        }
        if (!json_parse_value(&p)) {
            return NULL;
        }
        json_skip_ws(&p);
        if (*p == '}') {
            return NULL;
        }
        if (*p != ',') {
            return NULL;
        }
        p++;
        json_skip_ws(&p);
    }
}

static const char *json_find_params_value(const char *json, const char *field) {
    const char *params = json_find_object_value(json, "params");

    if (params == NULL) {
        return NULL;
    }
    return json_find_object_value(params, field);
}

static char *json_extract_str(const char *json, const char *field) {
    const char *p = json_find_object_value(json, field);

    if (p == NULL || *p != '"') {
        return NULL;
    }
    return decode_json_string(p + 1, NULL);
}

static char *json_extract_param_str(const char *json, const char *field) {
    const char *p = json_find_params_value(json, field);

    if (p == NULL || *p != '"') {
        return NULL;
    }
    return decode_json_string(p + 1, NULL);
}

static int json_has_param_field(const char *json, const char *field) {
    return json_find_params_value(json, field) != NULL;
}

static char *json_extract_id(const char *json) {
    const char *p = json_find_object_value(json, "id");
    Buf b;
    char *out;

    if (!p) {
        return str_dup("null");
    }
    if (!*p) {
        return str_dup("null");
    }

    buf_init(&b);
    if (!b.data) {
        return NULL;
    }

    if (*p == '"') {
        const char *end = NULL;
        char *decoded = decode_json_string(p + 1, &end);
        if (!decoded) {
            buf_free(&b);
            return NULL;
        }
        json_escape_str(&b, decoded);
        free(decoded);
    } else {
        while (*p && *p != ',' && *p != '}') {
            if (buf_append_char(&b, *p) != 0) {
                buf_free(&b);
                return NULL;
            }
            p++;
        }
        while (b.len > 0 &&
               (b.data[b.len - 1] == ' ' || b.data[b.len - 1] == '\t' ||
                b.data[b.len - 1] == '\r' || b.data[b.len - 1] == '\n')) {
            b.data[--b.len] = '\0';
        }
    }

    out = b.data;
    b.data = NULL;
    buf_free(&b);
    return out;
}

static int json_parse_string_value(const char **p) {
    if (**p != '"') {
        return 0;
    }
    (*p)++;

    while (**p) {
        unsigned char c = (unsigned char)**p;

        if (c == '"') {
            (*p)++;
            return 1;
        }
        if (c < 0x20) {
            return 0;
        }
        if (c == '\\') {
            (*p)++;
            switch (**p) {
            case '"':
            case '\\':
            case '/':
            case 'b':
            case 'f':
            case 'n':
            case 'r':
            case 't':
                (*p)++;
                break;
            case 'u':
                (*p)++;
                for (int i = 0; i < 4; i++) {
                    char h = **p;
                    if (!((h >= '0' && h <= '9') || (h >= 'a' && h <= 'f') ||
                          (h >= 'A' && h <= 'F'))) {
                        return 0;
                    }
                    (*p)++;
                }
                break;
            default:
                return 0;
            }
        } else {
            (*p)++;
        }
    }

    return 0;
}

static int json_parse_literal(const char **p, const char *literal) {
    size_t n = strlen(literal);
    if (strncmp(*p, literal, n) != 0) {
        return 0;
    }
    *p += n;
    return 1;
}

static int json_parse_number(const char **p) {
    const char *s = *p;

    if (*s == '-') {
        s++;
    }
    if (*s == '0') {
        s++;
    } else if (*s >= '1' && *s <= '9') {
        do {
            s++;
        } while (*s >= '0' && *s <= '9');
    } else {
        return 0;
    }

    if (*s == '.') {
        s++;
        if (!(*s >= '0' && *s <= '9')) {
            return 0;
        }
        do {
            s++;
        } while (*s >= '0' && *s <= '9');
    }

    if (*s == 'e' || *s == 'E') {
        s++;
        if (*s == '+' || *s == '-') {
            s++;
        }
        if (!(*s >= '0' && *s <= '9')) {
            return 0;
        }
        do {
            s++;
        } while (*s >= '0' && *s <= '9');
    }

    *p = s;
    return 1;
}

static int json_parse_array(const char **p) {
    if (**p != '[') {
        return 0;
    }
    (*p)++;
    json_skip_ws(p);

    if (**p == ']') {
        (*p)++;
        return 1;
    }

    for (;;) {
        if (!json_parse_value(p)) {
            return 0;
        }
        json_skip_ws(p);
        if (**p == ']') {
            (*p)++;
            return 1;
        }
        if (**p != ',') {
            return 0;
        }
        (*p)++;
        json_skip_ws(p);
        if (**p == ']') {
            return 0;
        }
    }
}

static int json_parse_object(const char **p) {
    if (**p != '{') {
        return 0;
    }
    (*p)++;
    json_skip_ws(p);

    if (**p == '}') {
        (*p)++;
        return 1;
    }

    for (;;) {
        if (!json_parse_string_value(p)) {
            return 0;
        }
        json_skip_ws(p);
        if (**p != ':') {
            return 0;
        }
        (*p)++;
        json_skip_ws(p);
        if (!json_parse_value(p)) {
            return 0;
        }
        json_skip_ws(p);
        if (**p == '}') {
            (*p)++;
            return 1;
        }
        if (**p != ',') {
            return 0;
        }
        (*p)++;
        json_skip_ws(p);
        if (**p == '}') {
            return 0;
        }
    }
}

static int json_parse_value(const char **p) {
    json_skip_ws(p);

    switch (**p) {
    case '{':
        return json_parse_object(p);
    case '[':
        return json_parse_array(p);
    case '"':
        return json_parse_string_value(p);
    case 't':
        return json_parse_literal(p, "true");
    case 'f':
        return json_parse_literal(p, "false");
    case 'n':
        return json_parse_literal(p, "null");
    default:
        if (**p == '-' || (**p >= '0' && **p <= '9')) {
            return json_parse_number(p);
        }
        return 0;
    }
}

static int validate_json_request(const char *json) {
    const char *p = json;

    json_skip_ws(&p);
    if (!json_parse_object(&p)) {
        return 0;
    }
    json_skip_ws(&p);
    return *p == '\0';
}

static int json_extract_str_array(const char *json, const char *field, StringArray *out) {
    const char *p;

    memset(out, 0, sizeof(*out));
    p = json_find_object_value(json, field);
    if (!p) {
        return 0;
    }

    if (*p != '[') {
        string_array_free(out);
        return -1;
    }
    p++;

    while (*p) {
        char *item;
        char **next;

        while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n' || *p == ',') {
            p++;
        }
        if (*p == ']') {
            return 0;
        }
        if (*p != '"') {
            string_array_free(out);
            return -1;
        }

        item = decode_json_string(p + 1, &p);
        if (!item || *p != '"') {
            free(item);
            string_array_free(out);
            return -1;
        }
        p++;

        next = realloc(out->items, sizeof(char *) * (out->len + 1));
        if (!next) {
            free(item);
            string_array_free(out);
            return -1;
        }
        out->items = next;
        out->items[out->len++] = item;
    }

    string_array_free(out);
    return -1;
}

static int json_extract_param_str_array(const char *json, const char *field, StringArray *out) {
    const char *params = json_find_object_value(json, "params");

    if (params == NULL) {
        memset(out, 0, sizeof(*out));
        return 0;
    }
    return json_extract_str_array(params, field, out);
}

static void string_array_free(StringArray *arr) {
    if (!arr) {
        return;
    }
    for (size_t i = 0; i < arr->len; i++) {
        free(arr->items[i]);
    }
    free(arr->items);
    arr->items = NULL;
    arr->len = 0;
}

static void parse_request_options_free(ParseRequestOptions *config) {
    if (!config) {
        return;
    }
    string_array_free(&config->clang_args);
    pk_c_compile_context_free(config->compile_context);
    free(config->parse_backend);
    free(config->compile_context_kind);
    free(config->workspace_root);
    free(config->compile_command);
    free(config->target_triple);
    memset(config, 0, sizeof(*config));
}

static int parse_backend_from_name(const char *name, pk_c_parse_backend *backend) {
    if (!backend) {
        return -1;
    }
    *backend = PK_C_PARSE_BACKEND_AUTO;
    if (!name || !*name || strcmp(name, "auto") == 0) {
        return 0;
    }
    if (strcmp(name, "regex") == 0) {
        *backend = PK_C_PARSE_BACKEND_REGEX;
        return 0;
    }
    if (strcmp(name, "clang_ast") == 0 || strcmp(name, "libclang") == 0) {
        *backend = PK_C_PARSE_BACKEND_CLANG_AST;
        return 0;
    }
    return -1;
}

static int parse_request_options_init(
    ParseRequestOptions *config,
    const char *line,
    const char *path,
    char *error,
    size_t error_len
) {
    pk_c_parse_backend backend = PK_C_PARSE_BACKEND_AUTO;

    memset(config, 0, sizeof(*config));
    config->parse_backend = json_extract_param_str(line, "parse_backend");
    if (!config->parse_backend) {
        config->parse_backend = json_extract_param_str(line, "parser_backend");
    }
    if (parse_backend_from_name(config->parse_backend, &backend) != 0) {
        (void)snprintf(error, error_len, "parse_backend must be auto, regex, or clang_ast");
        return -1;
    }
    if (json_extract_param_str_array(line, "clang_args", &config->clang_args) != 0) {
        (void)snprintf(error, error_len, "clang_args must be an array of strings");
        return -1;
    }
    config->compile_context_kind = json_extract_param_str(line, "compile_context");
    config->workspace_root = json_extract_param_str(line, "workspace_root");
    if (config->compile_context_kind != NULL &&
        config->compile_context_kind[0] != '\0' &&
        strcmp(config->compile_context_kind, "none") != 0 &&
        strcmp(config->compile_context_kind, "kernel") != 0) {
        (void)snprintf(error, error_len, "compile_context must be none or kernel");
        return -1;
    }
    config->resolve_kernel_context = config->compile_context_kind != NULL &&
        strcmp(config->compile_context_kind, "kernel") == 0;
    config->compile_command = json_extract_param_str(line, "compile_command");
    config->target_triple = json_extract_param_str(line, "target_triple");

    if (config->compile_command) {
        config->compile_context = pk_c_compile_context_from_command(path, config->compile_command);
        if (!config->compile_context) {
            (void)snprintf(error, error_len, "compile_command could not be parsed");
            return -1;
        }
        pk_c_compile_context_configure_parse_options(
            config->compile_context,
            backend,
            &config->options);
    } else if (config->resolve_kernel_context && path != NULL) {
        config->compile_context = pk_c_compile_context_resolve_kernel(config->workspace_root, path);
        if (!config->compile_context) {
            (void)snprintf(error, error_len, "kernel compile context could not be resolved");
            return -1;
        }
        pk_c_compile_context_configure_parse_options(
            config->compile_context,
            backend,
            &config->options);
    } else {
        memset(&config->options, 0, sizeof(config->options));
        config->options.backend = backend;
    }

    if (config->clang_args.len > 0) {
        config->options.clang_args = (const char *const *)config->clang_args.items;
        config->options.n_clang_args = config->clang_args.len;
    }
    if (config->target_triple) {
        config->options.target_triple = config->target_triple;
    }
    return 0;
}

static int append_request_option_opacity(
    pk_c_lift_result *result,
    const ParseRequestOptions *config
) {
    if (!config || !config->compile_context ||
        !config->compile_context->extraction_result) {
        return 0;
    }
    return pk_c_lift_result_extend(result, config->compile_context->extraction_result);
}

static void parse_request_options_apply_overrides(
    const ParseRequestOptions *config,
    pk_c_parse_options *options
) {
    if (!config || !options) {
        return;
    }
    if (config->clang_args.len > 0) {
        options->clang_args = (const char *const *)config->clang_args.items;
        options->n_clang_args = config->clang_args.len;
    }
    if (config->target_triple) {
        options->target_triple = config->target_triple;
    }
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
    char *out = malloc(al + (needs_slash ? 1u : 0u) + bl + 1u);
    size_t pos = al;

    if (!out) {
        return NULL;
    }

    memcpy(out, a, al);
    if (needs_slash) {
        out[pos++] = '/';
    }
    memcpy(out + pos, b, bl);
    out[pos + bl] = '\0';
    return out;
}

static char *resolve_source_path(const char *workspace, const char *source_path) {
    if (!source_path || !*source_path || strcmp(source_path, ".") == 0) {
        return str_dup(workspace);
    }
    if (source_path[0] == '/') {
        return str_dup(source_path);
    }
    return join_path(workspace, source_path);
}

static char *read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    long len;
    char *data;

    if (!f) {
        return NULL;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    len = ftell(f);
    if (len < 0) {
        fclose(f);
        return NULL;
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return NULL;
    }

    data = malloc((size_t)len + 1u);
    if (!data) {
        fclose(f);
        return NULL;
    }
    if (fread(data, 1, (size_t)len, f) != (size_t)len) {
        free(data);
        fclose(f);
        return NULL;
    }
    data[len] = '\0';
    fclose(f);
    return data;
}

static void acc_init(LiftAccumulator *acc) {
    memset(acc, 0, sizeof(*acc));
    buf_init(&acc->declarations);
    buf_init(&acc->ir);
    buf_init(&acc->call_edges);
    buf_init(&acc->diagnostics);
    buf_init(&acc->opacity_report);
    buf_init(&acc->refusals);
}

static void acc_free(LiftAccumulator *acc) {
    buf_free(&acc->declarations);
    buf_free(&acc->ir);
    buf_free(&acc->call_edges);
    buf_free(&acc->diagnostics);
    buf_free(&acc->opacity_report);
    buf_free(&acc->refusals);
}

static int acc_append_json_item(Buf *buf, size_t *count, const char *json) {
    if (*count > 0 && buf_append_char(buf, ',') != 0) {
        return -1;
    }
    if (buf_append(buf, json) != 0) {
        return -1;
    }
    (*count)++;
    return 0;
}

static int acc_append_json_array(Buf *buf, size_t *count, const pk_c_json_array *items) {
    for (size_t i = 0; i < items->len; i++) {
        if (acc_append_json_item(buf, count, items->items[i]) != 0) {
            return -1;
        }
    }
    return 0;
}

static int is_wp_walk_chain_declaration(const char *json) {
    return json != NULL &&
        strstr(json, "\"kind\":\"function-contract\"") != NULL &&
        strstr(json, "\"kind\":\"wp-walk-chain\"") != NULL;
}

static int acc_append_declarations(LiftAccumulator *acc, const pk_c_json_array *items) {
    for (size_t i = 0; i < items->len; i++) {
        const char *json = items->items[i];

        if (acc_append_json_item(&acc->ir, &acc->ir_count, json) != 0) {
            return -1;
        }
        /* `ir` is the canonical lift-plugin document. Keep wp-walk-chain
         * mementos out of the legacy declarations mirror so callers that
         * ingest both surfaces do not mint the same callsite twice. */
        if (!is_wp_walk_chain_declaration(json) &&
            acc_append_json_item(&acc->declarations, &acc->declaration_count, json) != 0) {
            return -1;
        }
    }
    return 0;
}

static int acc_append_result(LiftAccumulator *acc, pk_c_lift_result *result) {
    return acc_append_declarations(acc, &result->declarations) == 0 &&
        acc_append_json_array(&acc->call_edges, &acc->call_edge_count, &result->call_edges) == 0 &&
        acc_append_json_array(&acc->diagnostics, &acc->diagnostic_count, &result->diagnostics) == 0 &&
        acc_append_json_array(&acc->opacity_report, &acc->opacity_count, &result->opacity_report) == 0 &&
        acc_append_json_array(&acc->refusals, &acc->refusal_count, &result->refusals) == 0
        ? 0
        : -1;
}

static int lift_one_file(
    const char *path,
    LiftAccumulator *acc,
    const ParseRequestOptions *config
) {
    char *source = read_file(path);
    pk_c_lift_result *result;
    pk_c_compile_context *file_context = NULL;
    pk_c_parse_options file_options;
    const pk_c_parse_options *options = config == NULL ? NULL : &config->options;

    if (!source) {
        (void)snprintf(acc->error,
            sizeof(acc->error),
            "%s: read failed: %s",
            path,
            strerror(errno));
        return -1;
    }

    if (config != NULL && config->resolve_kernel_context && config->compile_context == NULL) {
        file_context = pk_c_compile_context_resolve_kernel(config->workspace_root, path);
        if (file_context == NULL) {
            free(source);
            (void)snprintf(acc->error, sizeof(acc->error), "%s: kernel compile context failed", path);
            return -1;
        }
        pk_c_compile_context_configure_parse_options(
            file_context,
            config->options.backend,
            &file_options);
        parse_request_options_apply_overrides(config, &file_options);
        options = &file_options;
    }

    result = pk_c_walk_lift_source_with_options(path, source, options);
    free(source);
    if (!result) {
        pk_c_compile_context_free(file_context);
        (void)snprintf(acc->error, sizeof(acc->error), "%s: lift failed", path);
        return -1;
    }
    if (file_context != NULL && file_context->extraction_result != NULL &&
        pk_c_lift_result_extend(result, file_context->extraction_result) != 0) {
        pk_c_lift_result_free(result);
        pk_c_compile_context_free(file_context);
        (void)snprintf(acc->error, sizeof(acc->error), "%s: context opacity failed", path);
        return -1;
    }

    if (acc_append_result(acc, result) != 0) {
        pk_c_lift_result_free(result);
        pk_c_compile_context_free(file_context);
        (void)snprintf(acc->error, sizeof(acc->error), "out of memory aggregating %s", path);
        return -1;
    }

    pk_c_lift_result_free(result);
    pk_c_compile_context_free(file_context);
    return 0;
}

static int walk_path(
    const char *path,
    LiftAccumulator *acc,
    const ParseRequestOptions *config
) {
    struct stat st;
    DIR *dir;
    struct dirent *entry;

    if (lstat(path, &st) != 0) {
        (void)snprintf(acc->error,
            sizeof(acc->error),
            "%s: stat failed: %s",
            path,
            strerror(errno));
        return -1;
    }

    if (S_ISLNK(st.st_mode)) {
        return 0;
    }

    if (S_ISREG(st.st_mode)) {
        return has_suffix(path, ".c") ? lift_one_file(path, acc, config) : 0;
    }

    if (!S_ISDIR(st.st_mode)) {
        return 0;
    }

    dir = opendir(path);
    if (!dir) {
        (void)snprintf(acc->error,
            sizeof(acc->error),
            "%s: opendir failed: %s",
            path,
            strerror(errno));
        return -1;
    }

    while ((entry = readdir(dir)) != NULL) {
        char *child;
        int rc;

        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }

        child = join_path(path, entry->d_name);
        if (!child) {
            closedir(dir);
            (void)snprintf(acc->error, sizeof(acc->error), "out of memory walking %s", path);
            return -1;
        }

        rc = walk_path(child, acc, config);
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
    char code_buf[32];

    buf_init(&b);
    if (!b.data) {
        printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":{\"code\":-32603,\"message\":\"internal error\"}}\n",
            id ? id : "null");
        fflush(stdout);
        return;
    }

    (void)snprintf(code_buf, sizeof(code_buf), "%d", code);
    (void)buf_append(&b, "{\"code\":");
    (void)buf_append(&b, code_buf);
    (void)buf_append(&b, ",\"message\":");
    json_escape_str(&b, message);
    (void)buf_append_char(&b, '}');

    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":%s}\n", id ? id : "null", b.data);
    fflush(stdout);
    buf_free(&b);
}

static void handle_initialize(const char *id) {
    send_response(id,
        "{\"capabilities\":{\"authoring_surfaces\":[\"c-walk\"],"
        "\"emits_signed_mementos\":false,\"ir_version\":\"v1.1.0\"},"
        "\"name\":\"c-walk\",\"protocol_version\":\"provekit-lift/1\","
        "\"version\":\"0.1.0\"}");
}

static void handle_parse(const char *id, const char *line) {
    char *path = json_extract_param_str(line, "path");
    char *source = json_extract_param_str(line, "source");
    ParseRequestOptions parse_config;
    char option_error[256];
    pk_c_lift_result *result;
    char *json;

    if (!source) {
        free(path);
        send_error(id, -32602, "missing source");
        return;
    }

    if (parse_request_options_init(
        &parse_config,
        line,
        path ? path : "source.c",
        option_error,
        sizeof(option_error)) != 0) {
        parse_request_options_free(&parse_config);
        free(path);
        free(source);
        send_error(id, -32602, option_error);
        return;
    }

    result = pk_c_walk_lift_source_with_options(
        path ? path : "source.c",
        source,
        &parse_config.options);
    if (!result) {
        parse_request_options_free(&parse_config);
        free(path);
        free(source);
        send_error(id, -32603, "internal error");
        return;
    }
    if (append_request_option_opacity(result, &parse_config) != 0) {
        pk_c_lift_result_free(result);
        parse_request_options_free(&parse_config);
        free(path);
        free(source);
        send_error(id, -32603, "internal error");
        return;
    }

    json = pk_c_lift_result_to_json(result);
    if (!json) {
        pk_c_lift_result_free(result);
        parse_request_options_free(&parse_config);
        free(path);
        free(source);
        send_error(id, -32603, "internal error");
        return;
    }

    send_response(id, json);

    free(json);
    pk_c_lift_result_free(result);
    parse_request_options_free(&parse_config);
    free(path);
    free(source);
}

static void handle_lift(const char *id, const char *line) {
    char *workspace = json_extract_param_str(line, "workspace_root");
    char *surface = json_extract_param_str(line, "surface");
    StringArray source_paths;
    ParseRequestOptions parse_config;
    char option_error[256];
    LiftAccumulator acc;
    Buf result;

    if (!surface) {
        free(workspace);
        send_error(id, -32602, "surface must be a string");
        return;
    }
    if (strcmp(surface, "c-walk") != 0) {
        free(surface);
        free(workspace);
        send_error(id, -32602, "unsupported surface");
        return;
    }

    if (!workspace || !*workspace) {
        free(workspace);
        workspace = str_dup(".");
        if (!workspace) {
            free(surface);
            send_error(id, -32603, "out of memory");
            return;
        }
    }

    if (!json_has_param_field(line, "source_paths")) {
        free(surface);
        free(workspace);
        send_error(id, -32602, "source_paths must be a non-empty array of strings");
        return;
    }

    if (json_extract_param_str_array(line, "source_paths", &source_paths) != 0) {
        string_array_free(&source_paths);
        free(surface);
        free(workspace);
        send_error(id, -32602, "source_paths must be an array of strings");
        return;
    }
    if (source_paths.len == 0) {
        string_array_free(&source_paths);
        free(surface);
        free(workspace);
        send_error(id, -32602, "source_paths must be a non-empty array of strings");
        return;
    }
    for (size_t i = 0; i < source_paths.len; i++) {
        if (!source_paths.items[i] || !*source_paths.items[i]) {
            string_array_free(&source_paths);
            free(surface);
            free(workspace);
            send_error(id, -32602, "source_paths entries must be non-empty strings");
            return;
        }
    }

    if (parse_request_options_init(
        &parse_config,
        line,
        NULL,
        option_error,
        sizeof(option_error)) != 0) {
        parse_request_options_free(&parse_config);
        string_array_free(&source_paths);
        free(surface);
        free(workspace);
        send_error(id, -32602, option_error);
        return;
    }

    acc_init(&acc);
    if (!acc.declarations.data || !acc.ir.data || !acc.call_edges.data || !acc.diagnostics.data ||
        !acc.opacity_report.data || !acc.refusals.data) {
        acc_free(&acc);
        parse_request_options_free(&parse_config);
        string_array_free(&source_paths);
        free(surface);
        free(workspace);
        send_error(id, -32603, "out of memory");
        return;
    }

    for (size_t i = 0; i < source_paths.len; i++) {
        char *resolved = resolve_source_path(workspace, source_paths.items[i]);
        int rc;

        if (!resolved) {
            acc_free(&acc);
            parse_request_options_free(&parse_config);
            string_array_free(&source_paths);
            free(surface);
            free(workspace);
            send_error(id, -32603, "out of memory");
            return;
        }

        rc = walk_path(resolved, &acc, &parse_config);
        free(resolved);
        if (rc != 0) {
            send_error(id, -32603, acc.error[0] ? acc.error : "lift failed");
            acc_free(&acc);
            parse_request_options_free(&parse_config);
            string_array_free(&source_paths);
            free(surface);
            free(workspace);
            return;
        }
    }

    if (parse_config.compile_context != NULL &&
        parse_config.compile_context->extraction_result != NULL &&
        acc_append_result(&acc, parse_config.compile_context->extraction_result) != 0) {
        send_error(id, -32603, "out of memory aggregating compile context opacity");
        acc_free(&acc);
        parse_request_options_free(&parse_config);
        string_array_free(&source_paths);
        free(surface);
        free(workspace);
        return;
    }

    buf_init(&result);
    if (!result.data ||
        buf_append(&result, "{\"declarations\":[") != 0 ||
        buf_append(&result, acc.declarations.data ? acc.declarations.data : "") != 0 ||
        buf_append(&result, "],\"callEdges\":[") != 0 ||
        buf_append(&result, acc.call_edges.data ? acc.call_edges.data : "") != 0 ||
        buf_append(&result, "],\"diagnostics\":[") != 0 ||
        buf_append(&result, acc.diagnostics.data ? acc.diagnostics.data : "") != 0 ||
        buf_append(&result, "],\"opacityReport\":[") != 0 ||
        buf_append(&result, acc.opacity_report.data ? acc.opacity_report.data : "") != 0 ||
        buf_append(&result, "],\"refusals\":[") != 0 ||
        buf_append(&result, acc.refusals.data ? acc.refusals.data : "") != 0 ||
        buf_append(&result, "],\"ir\":[") != 0 ||
        buf_append(&result, acc.ir.data ? acc.ir.data : "") != 0 ||
        buf_append(&result, "],\"kind\":\"ir-document\"}") != 0) {
        buf_free(&result);
        acc_free(&acc);
        parse_request_options_free(&parse_config);
        string_array_free(&source_paths);
        free(surface);
        free(workspace);
        send_error(id, -32603, "out of memory");
        return;
    }

    send_response(id, result.data);

    buf_free(&result);
    acc_free(&acc);
    parse_request_options_free(&parse_config);
    string_array_free(&source_paths);
    free(surface);
    free(workspace);
}

int main(int argc, char **argv) {
    char *line = NULL;
    size_t line_cap = 0;

    if (argc != 2 || strcmp(argv[1], "--rpc") != 0) {
        fprintf(stderr, "usage: %s --rpc\n", argv[0]);
        return 1;
    }

    while (getline(&line, &line_cap, stdin) != -1) {
        if (!validate_json_request(line)) {
            send_error("null", -32700, "parse error");
            continue;
        }

        char *id = json_extract_id(line);
        char *method = json_extract_str(line, "method");

        if (!id) {
            id = str_dup("null");
        }

        if (!method) {
            send_error(id, -32700, "parse error: missing method");
        } else if (strcmp(method, "initialize") == 0) {
            handle_initialize(id);
        } else if (strcmp(method, "parse") == 0) {
            handle_parse(id, line);
        } else if (strcmp(method, "lift") == 0) {
            handle_lift(id, line);
        } else if (strcmp(method, "shutdown") == 0) {
            send_response(id, "null");
            free(method);
            free(id);
            break;
        } else {
            send_error(id, -32601, "unknown method");
        }

        free(method);
        free(id);
    }

    free(line);
    return 0;
}
