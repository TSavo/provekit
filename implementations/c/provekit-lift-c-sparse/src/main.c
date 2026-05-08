#define _POSIX_C_SOURCE 200809L

#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#include "provekit/c_lift_core.h"

pk_c_lift_result *pk_c_sparse_lift_source(const char *path, const char *source);

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
    Buf ir;
    Buf diagnostics;
    size_t ir_count;
    size_t diagnostic_count;
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
            case 'n':
                out[len++] = '\n';
                break;
            case 'r':
                out[len++] = '\r';
                break;
            case 't':
                out[len++] = '\t';
                break;
            case '"':
                out[len++] = '"';
                break;
            case '\\':
                out[len++] = '\\';
                break;
            case '\0':
                out[len] = '\0';
                if (end_out) {
                    *end_out = p;
                }
                return out;
            default:
                out[len++] = *p;
                break;
            }
            p++;
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

static char *json_extract_str(const char *json, const char *field) {
    char needle[128];
    const char *p;

    (void)snprintf(needle, sizeof(needle), "\"%s\"", field);
    p = strstr(json, needle);
    if (!p) {
        return NULL;
    }

    p += strlen(needle);
    while (*p == ':' || *p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
        p++;
    }
    if (*p != '"') {
        return NULL;
    }

    return decode_json_string(p + 1, NULL);
}

static char *json_extract_id(const char *json) {
    const char *p = strstr(json, "\"id\"");
    Buf b;
    char *out;

    if (!p) {
        return str_dup("null");
    }
    p += strlen("\"id\"");
    while (*p == ':' || *p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
        p++;
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

static int validate_json_request(const char *json) {
    int object_depth = 0;
    int array_depth = 0;
    int in_string = 0;
    int saw_nonspace = 0;

    for (const char *p = json; *p; p++) {
        unsigned char c = (unsigned char)*p;

        if (!in_string && (c == ' ' || c == '\t' || c == '\r' || c == '\n')) {
            continue;
        }

        if (!saw_nonspace) {
            if (c != '{') {
                return 0;
            }
            saw_nonspace = 1;
        }

        if (in_string) {
            if (c == '\\') {
                if (p[1] == '\0') {
                    return 0;
                }
                p++;
                continue;
            }
            if (c == '"') {
                in_string = 0;
            }
            continue;
        }

        switch (c) {
        case '"':
            in_string = 1;
            break;
        case '{':
            object_depth++;
            break;
        case '}':
            if (object_depth == 0) {
                return 0;
            }
            object_depth--;
            break;
        case '[':
            array_depth++;
            break;
        case ']':
            if (array_depth == 0) {
                return 0;
            }
            array_depth--;
            break;
        default:
            break;
        }
    }

    return saw_nonspace && !in_string && object_depth == 0 && array_depth == 0;
}

static int json_extract_str_array(const char *json, const char *field, StringArray *out) {
    char needle[128];
    const char *p;

    memset(out, 0, sizeof(*out));
    (void)snprintf(needle, sizeof(needle), "\"%s\"", field);
    p = strstr(json, needle);
    if (!p) {
        return 0;
    }

    p += strlen(needle);
    while (*p == ':' || *p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
        p++;
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
    buf_init(&acc->ir);
    buf_init(&acc->diagnostics);
}

static void acc_free(LiftAccumulator *acc) {
    buf_free(&acc->ir);
    buf_free(&acc->diagnostics);
}

static int acc_append_result(LiftAccumulator *acc, pk_c_lift_result *result) {
    for (size_t i = 0; i < result->declarations.len; i++) {
        if (acc->ir_count > 0 && buf_append_char(&acc->ir, ',') != 0) {
            return -1;
        }
        if (buf_append(&acc->ir, result->declarations.items[i]) != 0) {
            return -1;
        }
        acc->ir_count++;
    }

    for (size_t i = 0; i < result->diagnostics.len; i++) {
        if (acc->diagnostic_count > 0 && buf_append_char(&acc->diagnostics, ',') != 0) {
            return -1;
        }
        json_escape_str(&acc->diagnostics, result->diagnostics.items[i]);
        acc->diagnostic_count++;
    }

    return 0;
}

static int lift_one_file(const char *path, LiftAccumulator *acc) {
    char *source = read_file(path);
    pk_c_lift_result *result;

    if (!source) {
        (void)snprintf(acc->error,
            sizeof(acc->error),
            "%s: read failed: %s",
            path,
            strerror(errno));
        return -1;
    }

    result = pk_c_sparse_lift_source(path, source);
    free(source);
    if (!result) {
        (void)snprintf(acc->error, sizeof(acc->error), "%s: lift failed", path);
        return -1;
    }

    if (acc_append_result(acc, result) != 0) {
        pk_c_lift_result_free(result);
        (void)snprintf(acc->error, sizeof(acc->error), "out of memory aggregating %s", path);
        return -1;
    }

    pk_c_lift_result_free(result);
    return 0;
}

static int walk_path(const char *path, LiftAccumulator *acc) {
    struct stat st;
    DIR *dir;
    struct dirent *entry;

    if (stat(path, &st) != 0) {
        (void)snprintf(acc->error,
            sizeof(acc->error),
            "%s: stat failed: %s",
            path,
            strerror(errno));
        return -1;
    }

    if (S_ISREG(st.st_mode)) {
        return has_suffix(path, ".c") ? lift_one_file(path, acc) : 0;
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

        rc = walk_path(child, acc);
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
        "{\"capabilities\":{\"authoring_surfaces\":[\"c-sparse\"],"
        "\"emits_signed_mementos\":false,\"ir_version\":\"v1.1.0\"},"
        "\"name\":\"c-sparse\",\"protocol_version\":\"provekit-lift/1\","
        "\"version\":\"0.1.0\"}");
}

static void handle_parse(const char *id, const char *line) {
    char *path = json_extract_str(line, "path");
    char *source = json_extract_str(line, "source");
    pk_c_lift_result *result;
    char *json;

    if (!source) {
        free(path);
        send_error(id, -32602, "missing source");
        return;
    }

    result = pk_c_sparse_lift_source(path ? path : "source.c", source);
    if (!result) {
        free(path);
        free(source);
        send_error(id, -32603, "internal error");
        return;
    }

    json = pk_c_lift_result_to_json(result);
    if (!json) {
        pk_c_lift_result_free(result);
        free(path);
        free(source);
        send_error(id, -32603, "internal error");
        return;
    }

    send_response(id, json);

    free(json);
    pk_c_lift_result_free(result);
    free(path);
    free(source);
}

static void handle_lift(const char *id, const char *line) {
    char *workspace = json_extract_str(line, "workspace_root");
    StringArray source_paths;
    LiftAccumulator acc;
    Buf result;

    if (!workspace || !*workspace) {
        free(workspace);
        workspace = str_dup(".");
        if (!workspace) {
            send_error(id, -32603, "out of memory");
            return;
        }
    }

    if (json_extract_str_array(line, "source_paths", &source_paths) != 0) {
        string_array_free(&source_paths);
        free(workspace);
        send_error(id, -32602, "source_paths must be an array of strings");
        return;
    }
    if (source_paths.len == 0) {
        source_paths.items = malloc(sizeof(char *));
        if (!source_paths.items) {
            free(workspace);
            send_error(id, -32603, "out of memory");
            return;
        }
        source_paths.items[0] = str_dup(".");
        if (!source_paths.items[0]) {
            string_array_free(&source_paths);
            free(workspace);
            send_error(id, -32603, "out of memory");
            return;
        }
        source_paths.len = 1;
    }

    acc_init(&acc);
    if (!acc.ir.data || !acc.diagnostics.data) {
        acc_free(&acc);
        string_array_free(&source_paths);
        free(workspace);
        send_error(id, -32603, "out of memory");
        return;
    }

    for (size_t i = 0; i < source_paths.len; i++) {
        char *resolved = resolve_source_path(workspace, source_paths.items[i]);
        int rc;

        if (!resolved) {
            acc_free(&acc);
            string_array_free(&source_paths);
            free(workspace);
            send_error(id, -32603, "out of memory");
            return;
        }

        rc = walk_path(resolved, &acc);
        free(resolved);
        if (rc != 0) {
            send_error(id, -32603, acc.error[0] ? acc.error : "lift failed");
            acc_free(&acc);
            string_array_free(&source_paths);
            free(workspace);
            return;
        }
    }

    buf_init(&result);
    if (!result.data ||
        buf_append(&result, "{\"diagnostics\":[") != 0 ||
        buf_append(&result, acc.diagnostics.data ? acc.diagnostics.data : "") != 0 ||
        buf_append(&result, "],\"ir\":[") != 0 ||
        buf_append(&result, acc.ir.data ? acc.ir.data : "") != 0 ||
        buf_append(&result, "],\"kind\":\"ir-document\"}") != 0) {
        buf_free(&result);
        acc_free(&acc);
        string_array_free(&source_paths);
        free(workspace);
        send_error(id, -32603, "out of memory");
        return;
    }

    send_response(id, result.data);

    buf_free(&result);
    acc_free(&acc);
    string_array_free(&source_paths);
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
