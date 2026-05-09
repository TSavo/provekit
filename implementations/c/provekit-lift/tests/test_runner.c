/* SPDX-License-Identifier: Apache-2.0 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/lift.h"

static void require(int ok, const char *message) {
    if (!ok) {
        fprintf(stderr, "FAIL: %s\n", message);
        exit(1);
    }
}

static void marker_source_is_not_lifted_by_compat_facade(void) {
    const char *source =
        "#include <stdbool.h>\n"
        "#include <stdint.h>\n"
        "typedef struct { bool overflow; uint8_t value; } checked_add_u8_result;\n"
        "/* provekit:contract checked_add_u8.postcondition */\n"
        "checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {\n"
        "    uint16_t wide = (uint16_t)a + (uint16_t)b;\n"
        "    if (wide >= 256) {\n"
        "        return (checked_add_u8_result){ .overflow = true, .value = 0 };\n"
        "    }\n"
        "    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };\n"
        "}\n";

    pk_lift_result *result = pk_lift_source(source);
    require(result == NULL, "legacy marker source should not lift through the compat facade");
    require(pk_last_error() != NULL, "compat facade should set last error");
    require(strstr(pk_last_error(), "generic C surface is a compatibility facade") != NULL,
            "compat facade error should explain the route");
    require(strstr(pk_last_error(), "c-sparse") != NULL,
            "compat facade error should name c-sparse");
    require(strstr(pk_last_error(), "c-kernel-doc") != NULL,
            "compat facade error should name c-kernel-doc");
    require(strstr(pk_last_error(), "c-assertions") != NULL,
            "compat facade error should name c-assertions");
}

static void unmarked_source_uses_same_compat_facade_error(void) {
    pk_lift_result *result = pk_lift_source("int add(int a, int b) { return a + b; }\n");
    require(result == NULL, "unmarked source should not lift through the compat facade");
    require(pk_last_error() != NULL, "compat facade should set last error");
    require(strstr(pk_last_error(), "generic C surface is a compatibility facade") != NULL,
            "compat facade error should explain the route");
}

int main(void) {
    marker_source_is_not_lifted_by_compat_facade();
    unmarked_source_uses_same_compat_facade_error();
    return 0;
}
