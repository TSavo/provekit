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

pk_c_lift_result *pk_c_assertions_lift_source(const char *path, const char *source) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    pk_c_source_facts *facts;
    int saw_warn_on = 0;
    int saw_build_bug_on = 0;
    int saw_assert = 0;

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

    for (size_t i = 0; i < facts->n_macro_calls; i++) {
        pk_c_macro_call_fact *call = &facts->macro_calls[i];

        if (!saw_warn_on &&
            (strcmp(call->name, "WARN_ON") == 0 || strcmp(call->name, "WARN_ON_ONCE") == 0)) {
            if (add_contract(result, "c-assertions.warn-on") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
            saw_warn_on = 1;
        } else if (!saw_build_bug_on && strcmp(call->name, "BUILD_BUG_ON") == 0) {
            if (add_contract(result, "c-assertions.build-bug-on") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
            saw_build_bug_on = 1;
        } else if (strcmp(call->name, "BUG_ON") == 0) {
            if (pk_c_lift_result_add_refusal_entry(
                result,
                "c-assertions.bug-on",
                call->locus.path,
                call->locus.line,
                call->locus.column,
                "c-assertions",
                "BUG_ON is a recognized assertion-like control-flow stop; no positive proof contract emitted") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
        }
    }

    for (size_t i = 0; i < facts->n_call_sites; i++) {
        pk_c_call_site_fact *call = &facts->call_sites[i];

        if (!saw_assert && strcmp(call->callee, "assert") == 0) {
            if (add_contract(result, "c-assertions.assert") != 0) {
                pk_c_source_facts_free(facts);
                pk_c_lift_result_free(result);
                return NULL;
            }
            saw_assert = 1;
        }
    }

    pk_c_source_facts_free(facts);
    return result;
}
