#define _POSIX_C_SOURCE 200809L

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/c_lift_core.h"

pk_c_lift_result *pk_c_sparse_lift_source(const char *path, const char *source);

static void emit_error(int code, const char *message) {
    printf("{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":%d,\"message\":\"%s\"}}\n",
        code,
        message);
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

static char *extract_flat_string_field(const char *request, const char *field) {
    size_t field_len = strlen(field);
    const char *p = request;

    while ((p = strstr(p, field)) != NULL) {
        const char *q = p + field_len;
        while (*q == ' ' || *q == '\t' || *q == '\r' || *q == '\n') {
            q++;
        }
        if (*q != ':') {
            p++;
            continue;
        }
        q++;
        while (*q == ' ' || *q == '\t' || *q == '\r' || *q == '\n') {
            q++;
        }
        if (*q != '"') {
            p++;
            continue;
        }
        return decode_json_string(q + 1, NULL);
    }

    return NULL;
}

int main(int argc, char **argv) {
    char *line = NULL;
    size_t line_cap = 0;

    if (argc != 2 || strcmp(argv[1], "--rpc") != 0) {
        fprintf(stderr, "usage: %s --rpc\n", argv[0]);
        return 1;
    }

    while (getline(&line, &line_cap, stdin) != -1) {
        char *path = extract_flat_string_field(line, "\"path\"");
        char *source = extract_flat_string_field(line, "\"source\"");
        pk_c_lift_result *result;
        char *json;

        if (!source) {
            free(path);
            emit_error(-32602, "missing source");
            continue;
        }

        result = pk_c_sparse_lift_source(path ? path : "source.c", source);
        if (!result) {
            free(path);
            free(source);
            emit_error(-32603, "internal error");
            continue;
        }

        json = pk_c_lift_result_to_json(result);
        if (!json) {
            pk_c_lift_result_free(result);
            free(path);
            free(source);
            emit_error(-32603, "internal error");
            continue;
        }

        printf("{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":%s}\n", json);

        free(json);
        pk_c_lift_result_free(result);
        free(path);
        free(source);
    }

    free(line);
    return 0;
}
