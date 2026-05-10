/* SPDX-License-Identifier: Apache-2.0 */
#ifndef PROVEKIT_C_WALKER_TERM_H
#define PROVEKIT_C_WALKER_TERM_H

#include "provekit/c_lift_core.h"

int pk_c_walker_emit_c11_terms(
    pk_c_lift_result *result,
    const char *path,
    const char *source,
    const pk_c_parse_options *options);

#endif
