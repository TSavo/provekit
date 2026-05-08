#include "provekit/c_lift_core.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int pk_c_checked_add_size(size_t lhs, size_t rhs, size_t *out) {
    if (SIZE_MAX - lhs < rhs) {
        return -1;
    }
    *out = lhs + rhs;
    return 0;
}

static int pk_c_checked_mul_size(size_t lhs, size_t rhs, size_t *out) {
    if (lhs != 0 && rhs > SIZE_MAX / lhs) {
        return -1;
    }
    *out = lhs * rhs;
    return 0;
}

static char *pk_c_copy_string(const char *src) {
    size_t len;
    char *copy;

    if (src == NULL) {
        return NULL;
    }

    len = strlen(src);
    copy = malloc(len + 1);
    if (copy == NULL) {
        return NULL;
    }

    memcpy(copy, src, len + 1);
    return copy;
}

static int pk_c_json_array_add(pk_c_json_array *array, const char *json) {
    char **items;
    char *copy;
    size_t cap;
    size_t bytes;

    if (array == NULL || json == NULL) {
        return -1;
    }

    if (array->len > array->cap) {
        return -1;
    }

    if (array->len == array->cap) {
        if (array->cap == 0) {
            cap = 4;
        } else if (pk_c_checked_mul_size(array->cap, 2, &cap) != 0) {
            return -1;
        }

        if (pk_c_checked_mul_size(cap, sizeof(*array->items), &bytes) != 0) {
            return -1;
        }

        items = realloc(array->items, bytes);
        if (items == NULL) {
            return -1;
        }
        array->items = items;
        array->cap = cap;
    }

    copy = pk_c_copy_string(json);
    if (copy == NULL) {
        return -1;
    }

    array->items[array->len] = copy;
    array->len++;
    return 0;
}

static void pk_c_json_array_free(pk_c_json_array *array) {
    size_t i;

    if (array == NULL) {
        return;
    }

    for (i = 0; i < array->len; i++) {
        free(array->items[i]);
    }
    free(array->items);
}

static int pk_c_json_array_json_len(const pk_c_json_array *array, size_t *out) {
    size_t len = 2;
    size_t i;

    if (array == NULL || out == NULL) {
        return -1;
    }

    if (array->len != 0 && array->items == NULL) {
        return -1;
    }

    for (i = 0; i < array->len; i++) {
        if (array->items[i] == NULL) {
            return -1;
        }

        if (i != 0) {
            if (pk_c_checked_add_size(len, 1, &len) != 0) {
                return -1;
            }
        }

        if (pk_c_checked_add_size(len, strlen(array->items[i]), &len) != 0) {
            return -1;
        }
    }

    *out = len;
    return 0;
}

static int pk_c_add_text_len(size_t *len, const char *text) {
    return pk_c_checked_add_size(*len, strlen(text), len);
}

static int pk_c_add_array_len(size_t *len, const pk_c_json_array *array) {
    size_t array_len;

    if (pk_c_json_array_json_len(array, &array_len) != 0) {
        return -1;
    }
    return pk_c_checked_add_size(*len, array_len, len);
}

static char *pk_c_append_text(char *dst, const char *text) {
    size_t len = strlen(text);

    memcpy(dst, text, len);
    return dst + len;
}

static char *pk_c_append_array(char *dst, const pk_c_json_array *array) {
    size_t i;

    *dst++ = '[';
    for (i = 0; i < array->len; i++) {
        if (i != 0) {
            *dst++ = ',';
        }
        dst = pk_c_append_text(dst, array->items[i]);
    }
    *dst++ = ']';
    return dst;
}

static int pk_c_json_escaped_len(const char *text, size_t *out) {
    size_t len = 0;
    size_t i;

    if (text == NULL || out == NULL) {
        return -1;
    }

    for (i = 0; text[i] != '\0'; i++) {
        switch (text[i]) {
        case '"':
        case '\\':
        case '\n':
        case '\r':
        case '\t':
            if (pk_c_checked_add_size(len, 2, &len) != 0) {
                return -1;
            }
            break;
        default:
            if (pk_c_checked_add_size(len, 1, &len) != 0) {
                return -1;
            }
            break;
        }
    }

    *out = len;
    return 0;
}

static char *pk_c_json_escape_string(const char *text) {
    size_t len;
    size_t i;
    char *escaped;
    char *dst;

    if (pk_c_json_escaped_len(text, &len) != 0 ||
        pk_c_checked_add_size(len, 1, &len) != 0) {
        return NULL;
    }

    escaped = malloc(len);
    if (escaped == NULL) {
        return NULL;
    }

    dst = escaped;
    for (i = 0; text[i] != '\0'; i++) {
        switch (text[i]) {
        case '"':
            *dst++ = '\\';
            *dst++ = '"';
            break;
        case '\\':
            *dst++ = '\\';
            *dst++ = '\\';
            break;
        case '\n':
            *dst++ = '\\';
            *dst++ = 'n';
            break;
        case '\r':
            *dst++ = '\\';
            *dst++ = 'r';
            break;
        case '\t':
            *dst++ = '\\';
            *dst++ = 't';
            break;
        default:
            *dst++ = text[i];
            break;
        }
    }
    *dst = '\0';
    return escaped;
}

static int pk_c_format_int(char *dst, size_t len, int value) {
    int written = snprintf(dst, len, "%d", value);

    if (written < 0 || (size_t)written >= len) {
        return -1;
    }
    return 0;
}

pk_c_lift_result *pk_c_lift_result_new(void) {
    return calloc(1, sizeof(pk_c_lift_result));
}

void pk_c_lift_result_free(pk_c_lift_result *result) {
    if (result == NULL) {
        return;
    }

    pk_c_json_array_free(&result->declarations);
    pk_c_json_array_free(&result->call_edges);
    pk_c_json_array_free(&result->diagnostics);
    pk_c_json_array_free(&result->opacity_report);
    pk_c_json_array_free(&result->refusals);
    free(result);
}

int pk_c_lift_result_add_declaration(pk_c_lift_result *result, const char *json) {
    if (result == NULL) {
        return -1;
    }
    return pk_c_json_array_add(&result->declarations, json);
}

int pk_c_lift_result_add_call_edge(pk_c_lift_result *result, const char *json) {
    if (result == NULL) {
        return -1;
    }
    return pk_c_json_array_add(&result->call_edges, json);
}

int pk_c_lift_result_add_diagnostic(pk_c_lift_result *result, const char *json) {
    if (result == NULL) {
        return -1;
    }
    return pk_c_json_array_add(&result->diagnostics, json);
}

int pk_c_lift_result_add_opacity(pk_c_lift_result *result, const char *json) {
    if (result == NULL) {
        return -1;
    }
    return pk_c_json_array_add(&result->opacity_report, json);
}

int pk_c_lift_result_add_refusal(pk_c_lift_result *result, const char *json) {
    if (result == NULL) {
        return -1;
    }
    return pk_c_json_array_add(&result->refusals, json);
}

int pk_c_lift_result_add_opacity_entry(
    pk_c_lift_result *result,
    const char *kind,
    const char *path,
    int line,
    int column,
    const char *reason,
    const char *affected_surface) {
    const char *prefix = "{\"affectedSurface\":\"";
    const char *kind_key = "\",\"kind\":\"";
    const char *locus_prefix = "\",\"locus\":{\"column\":";
    const char *line_key = ",\"line\":";
    const char *path_key = ",\"path\":\"";
    const char *reason_key = "\"},\"reason\":\"";
    const char *suffix = "\"}";
    char line_buf[32];
    char column_buf[32];
    char *escaped_affected_surface;
    char *escaped_kind;
    char *escaped_path;
    char *escaped_reason;
    char *json;
    char *dst;
    size_t len = 0;
    int rc;

    if (result == NULL || kind == NULL || path == NULL || reason == NULL ||
        affected_surface == NULL ||
        pk_c_format_int(line_buf, sizeof(line_buf), line) != 0 ||
        pk_c_format_int(column_buf, sizeof(column_buf), column) != 0) {
        return -1;
    }

    escaped_affected_surface = pk_c_json_escape_string(affected_surface);
    escaped_kind = pk_c_json_escape_string(kind);
    escaped_path = pk_c_json_escape_string(path);
    escaped_reason = pk_c_json_escape_string(reason);
    if (escaped_affected_surface == NULL || escaped_kind == NULL ||
        escaped_path == NULL || escaped_reason == NULL) {
        free(escaped_affected_surface);
        free(escaped_kind);
        free(escaped_path);
        free(escaped_reason);
        return -1;
    }

    if (pk_c_add_text_len(&len, prefix) != 0 ||
        pk_c_add_text_len(&len, escaped_affected_surface) != 0 ||
        pk_c_add_text_len(&len, kind_key) != 0 ||
        pk_c_add_text_len(&len, escaped_kind) != 0 ||
        pk_c_add_text_len(&len, locus_prefix) != 0 ||
        pk_c_add_text_len(&len, column_buf) != 0 ||
        pk_c_add_text_len(&len, line_key) != 0 ||
        pk_c_add_text_len(&len, line_buf) != 0 ||
        pk_c_add_text_len(&len, path_key) != 0 ||
        pk_c_add_text_len(&len, escaped_path) != 0 ||
        pk_c_add_text_len(&len, reason_key) != 0 ||
        pk_c_add_text_len(&len, escaped_reason) != 0 ||
        pk_c_add_text_len(&len, suffix) != 0 ||
        pk_c_checked_add_size(len, 1, &len) != 0) {
        free(escaped_affected_surface);
        free(escaped_kind);
        free(escaped_path);
        free(escaped_reason);
        return -1;
    }

    json = malloc(len);
    if (json == NULL) {
        free(escaped_affected_surface);
        free(escaped_kind);
        free(escaped_path);
        free(escaped_reason);
        return -1;
    }

    dst = json;
    dst = pk_c_append_text(dst, prefix);
    dst = pk_c_append_text(dst, escaped_affected_surface);
    dst = pk_c_append_text(dst, kind_key);
    dst = pk_c_append_text(dst, escaped_kind);
    dst = pk_c_append_text(dst, locus_prefix);
    dst = pk_c_append_text(dst, column_buf);
    dst = pk_c_append_text(dst, line_key);
    dst = pk_c_append_text(dst, line_buf);
    dst = pk_c_append_text(dst, path_key);
    dst = pk_c_append_text(dst, escaped_path);
    dst = pk_c_append_text(dst, reason_key);
    dst = pk_c_append_text(dst, escaped_reason);
    dst = pk_c_append_text(dst, suffix);
    *dst = '\0';

    rc = pk_c_lift_result_add_opacity(result, json);
    free(json);
    free(escaped_affected_surface);
    free(escaped_kind);
    free(escaped_path);
    free(escaped_reason);
    return rc;
}

int pk_c_lift_result_add_refusal_entry(
    pk_c_lift_result *result,
    const char *kind,
    const char *path,
    int line,
    int column,
    const char *surface,
    const char *reason) {
    const char *prefix = "{\"kind\":\"";
    const char *locus_prefix = "\",\"locus\":{\"column\":";
    const char *line_key = ",\"line\":";
    const char *path_key = ",\"path\":\"";
    const char *reason_key = "\"},\"reason\":\"";
    const char *surface_key = "\",\"surface\":\"";
    const char *suffix = "\"}";
    char line_buf[32];
    char column_buf[32];
    char *escaped_kind;
    char *escaped_path;
    char *escaped_surface;
    char *escaped_reason;
    char *json;
    char *dst;
    size_t len = 0;
    int rc;

    if (result == NULL || kind == NULL || path == NULL || surface == NULL ||
        reason == NULL ||
        pk_c_format_int(line_buf, sizeof(line_buf), line) != 0 ||
        pk_c_format_int(column_buf, sizeof(column_buf), column) != 0) {
        return -1;
    }

    escaped_kind = pk_c_json_escape_string(kind);
    escaped_path = pk_c_json_escape_string(path);
    escaped_surface = pk_c_json_escape_string(surface);
    escaped_reason = pk_c_json_escape_string(reason);
    if (escaped_kind == NULL || escaped_path == NULL ||
        escaped_surface == NULL || escaped_reason == NULL) {
        free(escaped_kind);
        free(escaped_path);
        free(escaped_surface);
        free(escaped_reason);
        return -1;
    }

    if (pk_c_add_text_len(&len, prefix) != 0 ||
        pk_c_add_text_len(&len, escaped_kind) != 0 ||
        pk_c_add_text_len(&len, locus_prefix) != 0 ||
        pk_c_add_text_len(&len, column_buf) != 0 ||
        pk_c_add_text_len(&len, line_key) != 0 ||
        pk_c_add_text_len(&len, line_buf) != 0 ||
        pk_c_add_text_len(&len, path_key) != 0 ||
        pk_c_add_text_len(&len, escaped_path) != 0 ||
        pk_c_add_text_len(&len, reason_key) != 0 ||
        pk_c_add_text_len(&len, escaped_reason) != 0 ||
        pk_c_add_text_len(&len, surface_key) != 0 ||
        pk_c_add_text_len(&len, escaped_surface) != 0 ||
        pk_c_add_text_len(&len, suffix) != 0 ||
        pk_c_checked_add_size(len, 1, &len) != 0) {
        free(escaped_kind);
        free(escaped_path);
        free(escaped_surface);
        free(escaped_reason);
        return -1;
    }

    json = malloc(len);
    if (json == NULL) {
        free(escaped_kind);
        free(escaped_path);
        free(escaped_surface);
        free(escaped_reason);
        return -1;
    }

    dst = json;
    dst = pk_c_append_text(dst, prefix);
    dst = pk_c_append_text(dst, escaped_kind);
    dst = pk_c_append_text(dst, locus_prefix);
    dst = pk_c_append_text(dst, column_buf);
    dst = pk_c_append_text(dst, line_key);
    dst = pk_c_append_text(dst, line_buf);
    dst = pk_c_append_text(dst, path_key);
    dst = pk_c_append_text(dst, escaped_path);
    dst = pk_c_append_text(dst, reason_key);
    dst = pk_c_append_text(dst, escaped_reason);
    dst = pk_c_append_text(dst, surface_key);
    dst = pk_c_append_text(dst, escaped_surface);
    dst = pk_c_append_text(dst, suffix);
    *dst = '\0';

    rc = pk_c_lift_result_add_refusal(result, json);
    free(json);
    free(escaped_kind);
    free(escaped_path);
    free(escaped_surface);
    free(escaped_reason);
    return rc;
}

char *pk_c_lift_result_to_json(const pk_c_lift_result *result) {
    const char *prefix = "{\"declarations\":";
    const char *call_edges_key = ",\"callEdges\":";
    const char *diagnostics_key = ",\"diagnostics\":";
    const char *opacity_key = ",\"opacityReport\":";
    const char *refusals_key = ",\"refusals\":";
    size_t len = 0;
    char *json;
    char *dst;

    if (result == NULL) {
        return NULL;
    }

    if (pk_c_add_text_len(&len, prefix) != 0 ||
        pk_c_add_array_len(&len, &result->declarations) != 0 ||
        pk_c_add_text_len(&len, call_edges_key) != 0 ||
        pk_c_add_array_len(&len, &result->call_edges) != 0 ||
        pk_c_add_text_len(&len, diagnostics_key) != 0 ||
        pk_c_add_array_len(&len, &result->diagnostics) != 0 ||
        pk_c_add_text_len(&len, opacity_key) != 0 ||
        pk_c_add_array_len(&len, &result->opacity_report) != 0 ||
        pk_c_add_text_len(&len, refusals_key) != 0 ||
        pk_c_add_array_len(&len, &result->refusals) != 0 ||
        pk_c_checked_add_size(len, 2, &len) != 0) {
        return NULL;
    }

    json = malloc(len);
    if (json == NULL) {
        return NULL;
    }

    dst = json;
    dst = pk_c_append_text(dst, prefix);
    dst = pk_c_append_array(dst, &result->declarations);
    dst = pk_c_append_text(dst, call_edges_key);
    dst = pk_c_append_array(dst, &result->call_edges);
    dst = pk_c_append_text(dst, diagnostics_key);
    dst = pk_c_append_array(dst, &result->diagnostics);
    dst = pk_c_append_text(dst, opacity_key);
    dst = pk_c_append_array(dst, &result->opacity_report);
    dst = pk_c_append_text(dst, refusals_key);
    dst = pk_c_append_array(dst, &result->refusals);
    *dst++ = '}';
    *dst = '\0';

    return json;
}
