#include "provekit/c_lift_core.h"

#include <stdint.h>
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
