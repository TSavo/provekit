#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/c_lift_core.h"

static char *json_escape_fragment(const char *src) {
    size_t len = 0;
    char *out;
    char *p;

    if (src == NULL) {
        src = "";
    }
    for (const unsigned char *s = (const unsigned char *)src; *s != '\0'; s++) {
        size_t add;

        if (*s == '"' || *s == '\\') {
            add = 2;
        } else if (*s < 0x20) {
            add = 6;
        } else {
            add = 1;
        }
        if (add > ((size_t)-1) - len) {
            return NULL;
        }
        len += add;
    }
    out = malloc(len + 1);
    if (out == NULL) {
        return NULL;
    }
    p = out;
    for (const unsigned char *s = (const unsigned char *)src; *s != '\0'; s++) {
        switch (*s) {
        case '"':
            *p++ = '\\';
            *p++ = '"';
            break;
        case '\\':
            *p++ = '\\';
            *p++ = '\\';
            break;
        case '\n':
            *p++ = '\\';
            *p++ = 'n';
            break;
        case '\r':
            *p++ = '\\';
            *p++ = 'r';
            break;
        case '\t':
            *p++ = '\\';
            *p++ = 't';
            break;
        default:
            if (*s < 0x20) {
                (void)snprintf(p, 7, "\\u%04x", *s);
                p += 6;
            } else {
                *p++ = (char)*s;
            }
            break;
        }
    }
    *p = '\0';
    return out;
}

static int add_contract(pk_c_lift_result *result, const char *name, const char *var_name) {
    char *escaped_name = json_escape_fragment(name);
    char *escaped_var_name = json_escape_fragment(var_name);
    char *json;
    int written;
    int rc;

    if (escaped_name == NULL || escaped_var_name == NULL) {
        free(escaped_name);
        free(escaped_var_name);
        return -1;
    }
    written = snprintf(NULL,
        0,
        "{\"kind\":\"contract\",\"name\":\"%s\",\"outBinding\":\"out\","
        "\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"%s\"}],"
        "\"kind\":\"atomic\",\"name\":\"%s\"}}",
        escaped_name,
        escaped_var_name,
        escaped_name);
    if (written < 0) {
        free(escaped_name);
        free(escaped_var_name);
        return -1;
    }
    json = malloc((size_t)written + 1);
    if (json == NULL) {
        free(escaped_name);
        free(escaped_var_name);
        return -1;
    }
    (void)snprintf(json,
        (size_t)written + 1,
        "{\"kind\":\"contract\",\"name\":\"%s\",\"outBinding\":\"out\","
        "\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"%s\"}],"
        "\"kind\":\"atomic\",\"name\":\"%s\"}}",
        escaped_name,
        escaped_var_name,
        escaped_name);

    rc = pk_c_lift_result_add_declaration(result, json);
    free(json);
    free(escaped_name);
    free(escaped_var_name);
    return rc;
}

pk_c_lift_result *pk_c_sparse_lift_source(const char *path, const char *source) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    pk_c_source_facts *facts;

    if (!result) {
        return NULL;
    }

    if (!source) {
        return result;
    }

    facts = pk_c_parse_source(path, source);
    if (!facts) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"severity\":\"error\",\"message\":\"parse failed\"}");
        return result;
    }

    for (size_t i = 0; i < facts->n_sparse_annotations; i++) {
        pk_c_sparse_annotation_fact *annotation = &facts->sparse_annotations[i];

        if (strcmp(annotation->name, "__user") == 0) {
            if (add_contract(result, "c-sparse.user-pointer", "ptr") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
        } else if (strcmp(annotation->name, "__rcu") == 0) {
            if (add_contract(result, "c-sparse.rcu-pointer", "ptr") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
        } else if (strcmp(annotation->name, "__must_hold") == 0) {
            if (add_contract(result, "c-sparse.must-hold",
                annotation->argument_text[0] ? annotation->argument_text : "lock") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
        } else if (strcmp(annotation->name, "__acquires") == 0) {
            if (add_contract(result, "c-sparse.acquires",
                annotation->argument_text[0] ? annotation->argument_text : "lock") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
        } else if (strcmp(annotation->name, "__releases") == 0) {
            if (add_contract(result, "c-sparse.releases",
                annotation->argument_text[0] ? annotation->argument_text : "lock") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
        }
    }

    pk_c_source_facts_free(facts);
    return result;
}
