#include "provekit/c_lift_core.h"

pk_c_source_facts *pk_c_parse_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_source_facts *facts = pk_c_parse_source(path, source);

    if (facts != NULL && options != NULL &&
        options->backend == PK_C_PARSE_BACKEND_CLANG_AST) {
        if (facts->extraction_result == NULL) {
            facts->extraction_result = pk_c_lift_result_new();
        }
        if (facts->extraction_result != NULL) {
            (void)pk_c_lift_result_add_opacity_entry(
                facts->extraction_result,
                "ast-backend-unavailable",
                path == NULL ? "" : path,
                1,
                1,
                "libclang AST backend was not enabled at build time",
                "c-core");
        }
    }

    return facts;
}
