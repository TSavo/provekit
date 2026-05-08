#include <stdio.h>
#include <string.h>

#include "provekit/c_lift_core.h"

static int add_contract(pk_c_lift_result *result, const char *name) {
    char json[512];
    int written = snprintf(json,
        sizeof(json),
        "{\"kind\":\"contract\",\"name\":\"%s\",\"outBinding\":\"out\","
        "\"post\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"%s\"}}",
        name,
        name);

    if (written < 0 || (size_t)written >= sizeof(json)) {
        return -1;
    }

    return pk_c_lift_result_add_declaration(result, json);
}

static int contains_bug_on_call(const char *source) {
    const char *p = source;

    while ((p = strstr(p, "BUG_ON(")) != NULL) {
        if (p < source + strlen("BUILD_") || strncmp(p - strlen("BUILD_"), "BUILD_", strlen("BUILD_")) != 0) {
            return 1;
        }
        p += strlen("BUG_ON(");
    }

    return 0;
}

pk_c_lift_result *pk_c_assertions_lift_source(const char *path, const char *source) {
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

    if (strstr(source, "WARN_ON(") || strstr(source, "WARN_ON_ONCE(")) {
        (void)add_contract(result, "c-assertions.warn-on");
    }
    if (strstr(source, "BUILD_BUG_ON(")) {
        (void)add_contract(result, "c-assertions.build-bug-on");
    }
    if (contains_bug_on_call(source)) {
        (void)add_contract(result, "c-assertions.bug-on");
    }
    if (strstr(source, "assert(")) {
        (void)add_contract(result, "c-assertions.assert");
    }

    return result;
}
