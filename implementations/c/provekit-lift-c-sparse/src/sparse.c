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

    if (!result) {
        return NULL;
    }

    facts = pk_c_parse_source(path, source);
    if (facts) {
        pk_c_source_facts_free(facts);
    }

    if (!source) {
        return result;
    }

    if (strstr(source, "__user")) {
        (void)add_contract(result, "c-sparse.user-pointer", "ptr");
    }
    if (strstr(source, "__rcu")) {
        (void)add_contract(result, "c-sparse.rcu-pointer", "ptr");
    }
    if (strstr(source, "__must_hold")) {
        (void)add_contract(result, "c-sparse.must-hold", "lock");
    }
    if (strstr(source, "__acquires")) {
        (void)add_contract(result, "c-sparse.acquires", "lock");
    }
    if (strstr(source, "__releases")) {
        (void)add_contract(result, "c-sparse.releases", "lock");
    }

    return result;
}
