#include "provekit/c_lift_core.h"

#include <stdlib.h>
#include <string.h>

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

    if (array == NULL || json == NULL) {
        return -1;
    }

    if (array->len == array->cap) {
        cap = array->cap == 0 ? 4 : array->cap * 2;
        items = realloc(array->items, cap * sizeof(*array->items));
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

static size_t pk_c_json_array_json_len(const pk_c_json_array *array) {
    size_t len = 2;
    size_t i;

    for (i = 0; i < array->len; i++) {
        if (i != 0) {
            len++;
        }
        len += strlen(array->items[i]);
    }

    return len;
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
    size_t len;
    char *json;
    char *dst;

    if (result == NULL) {
        return NULL;
    }

    len = strlen(prefix) +
        pk_c_json_array_json_len(&result->declarations) +
        strlen(call_edges_key) +
        pk_c_json_array_json_len(&result->call_edges) +
        strlen(diagnostics_key) +
        pk_c_json_array_json_len(&result->diagnostics) +
        strlen(opacity_key) +
        pk_c_json_array_json_len(&result->opacity_report) +
        strlen(refusals_key) +
        pk_c_json_array_json_len(&result->refusals) +
        2;

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
