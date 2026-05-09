#include <stdio.h>
#include <string.h>

#include "provekit/c_lift_core.h"

static int add_contract(pk_c_lift_result *result, const char *name, const char *var_name) {
    char json[512];
    int written = snprintf(json,
        sizeof(json),
        "{\"kind\":\"contract\",\"name\":\"%s\",\"outBinding\":\"out\","
        "\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"%s\"}],"
        "\"kind\":\"atomic\",\"name\":\"%s\"}}",
        name,
        var_name,
        name);

    if (written < 0 || (size_t)written >= sizeof(json)) {
        return -1;
    }

    return pk_c_lift_result_add_declaration(result, json);
}

pk_c_lift_result *pk_c_sparse_lift_source(const char *path, const char *source) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    pk_c_source_facts *facts;
    int saw_user = 0;
    int saw_rcu = 0;
    int saw_must_hold = 0;
    int saw_acquires = 0;
    int saw_releases = 0;

    if (!result) {
        return NULL;
    }

    if (!source) {
        return result;
    }

    facts = pk_c_parse_source(path, source);
    if (!facts) {
        return result;
    }

    for (size_t i = 0; i < facts->n_sparse_annotations; i++) {
        pk_c_sparse_annotation_fact *annotation = &facts->sparse_annotations[i];

        if (!saw_user && strcmp(annotation->name, "__user") == 0) {
            (void)add_contract(result, "c-sparse.user-pointer", "ptr");
            saw_user = 1;
        } else if (!saw_rcu && strcmp(annotation->name, "__rcu") == 0) {
            (void)add_contract(result, "c-sparse.rcu-pointer", "ptr");
            saw_rcu = 1;
        } else if (!saw_must_hold && strcmp(annotation->name, "__must_hold") == 0) {
            (void)add_contract(result, "c-sparse.must-hold",
                annotation->argument_text[0] ? annotation->argument_text : "lock");
            saw_must_hold = 1;
        } else if (!saw_acquires && strcmp(annotation->name, "__acquires") == 0) {
            (void)add_contract(result, "c-sparse.acquires",
                annotation->argument_text[0] ? annotation->argument_text : "lock");
            saw_acquires = 1;
        } else if (!saw_releases && strcmp(annotation->name, "__releases") == 0) {
            (void)add_contract(result, "c-sparse.releases",
                annotation->argument_text[0] ? annotation->argument_text : "lock");
            saw_releases = 1;
        }
    }

    pk_c_source_facts_free(facts);
    return result;
}
