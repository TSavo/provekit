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

static void lifts_provekit_contract_marker(void) {
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
    require(result != NULL, "marker source should lift");
    require(result->proof_ir_bundle != NULL, "lift should return proof IR JSON");
    require(strstr(result->proof_ir_bundle, "\"kind\":\"ir-document\"") != NULL,
            "lift should return an ir-document");
    require(strstr(result->proof_ir_bundle, "\"kind\":\"contract\"") != NULL,
            "lift should emit a contract declaration");
    require(strstr(result->proof_ir_bundle, "\"name\":\"checked_add_u8.postcondition\"") != NULL,
            "lift should preserve the marker contract name");
    require(strstr(result->proof_ir_bundle, "\"outBinding\":\"out\"") != NULL,
            "lift should emit the default out binding");
    pk_lift_result_free(result);
}

static void unmarked_source_keeps_libclang_gap(void) {
    pk_lift_result *result = pk_lift_source("int add(int a, int b) { return a + b; }\n");
    require(result == NULL, "unmarked source should still use the unimplemented libclang path");
    require(pk_last_error() != NULL, "unmarked source should set last error");
    require(strstr(pk_last_error(), "libclang integration TODO") != NULL,
            "unmarked source should report the libclang gap");
}

static void checked_add_marker_rejects_missing_overflow_guard(void) {
    const char *source =
        "#include <stdbool.h>\n"
        "#include <stdint.h>\n"
        "typedef struct { bool overflow; uint8_t value; } checked_add_u8_result;\n"
        "/* provekit:contract checked_add_u8.postcondition */\n"
        "checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {\n"
        "    uint16_t wide = (uint16_t)a + (uint16_t)b;\n"
        "    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };\n"
        "}\n";

    pk_lift_result *result = pk_lift_source(source);
    require(result == NULL, "weakened checked_add_u8 contract should fail closed");
    require(pk_last_error() != NULL, "weakened checked_add_u8 should set last error");
    require(strstr(pk_last_error(), "checked_add_u8.postcondition") != NULL,
            "weakened checked_add_u8 error should name the contract");
    require(strstr(pk_last_error(), "overflow guard") != NULL,
            "weakened checked_add_u8 error should name the missing guard");
}

static void wrong_contract_marker_lifts_as_wrong_contract(void) {
    const char *source =
        "#include <stdbool.h>\n"
        "#include <stdint.h>\n"
        "typedef struct { bool overflow; uint8_t value; } overflow_add_u8_result;\n"
        "/* provekit:contract overflow_add_u8.postcondition */\n"
        "overflow_add_u8_result overflow_add_u8(uint8_t a, uint8_t b) {\n"
        "    uint16_t wide = (uint16_t)a + (uint16_t)b;\n"
        "    return (overflow_add_u8_result){ .overflow = false, .value = (uint8_t)wide };\n"
        "}\n";

    pk_lift_result *result = pk_lift_source(source);
    require(result != NULL, "wrong contract marker should still lift as its own claim");
    require(strstr(result->proof_ir_bundle, "\"name\":\"overflow_add_u8.postcondition\"") != NULL,
            "wrong contract marker should preserve the wrong contract name");
    require(strstr(result->proof_ir_bundle, "\"name\":\"checked_add_u8.postcondition\"") == NULL,
            "wrong contract marker must not emit the checked-add boundary contract");
    pk_lift_result_free(result);
}

int main(void) {
    lifts_provekit_contract_marker();
    unmarked_source_keeps_libclang_gap();
    checked_add_marker_rejects_missing_overflow_guard();
    wrong_contract_marker_lifts_as_wrong_contract();
    return 0;
}
