/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Private kernel-doc index, shared between kernel_doc.c (builder) and
 * composition.c (consumer). Not part of any public surface.
 *
 * The composition pass needs per-function kernel-doc preconditions
 * (param.nonnull, param.positive, context.must-hold, return.negative-errno)
 * to construct real FunctionContractMemento envelopes for the libprovekit
 * compose FFI, per CCP v1.0.0 sections 2 + 9 + Appendix A. The kernel_doc
 * scanner already extracts these; this index lets us hand the same data
 * to the composition pass without re-parsing the source.
 *
 * The scanner runs once per source over the same line buffer
 * scan_kernel_doc walks, recording (function, kind, binding) triples.
 * Construction order in kernel_doc.c does not depend on entry insertion
 * order in this index because the consumer (composition.c) sorts entries
 * lex by kind before encoding them, so byte-stable composed CIDs are
 * preserved across runs.
 */
#ifndef PROVEKIT_C_KDOC_INDEX_H
#define PROVEKIT_C_KDOC_INDEX_H

#include <stddef.h>

typedef struct {
    char *kind;     /* e.g. "c-kernel-doc.param.nonnull" (heap-owned) */
    char *binding;  /* parameter name or lock name (heap-owned) */
} pk_c_kdoc_entry;

typedef struct {
    char *function; /* function name (heap-owned) */
    pk_c_kdoc_entry *entries;
    size_t n_entries;
    size_t cap_entries;
} pk_c_kdoc_function_entries;

typedef struct {
    pk_c_kdoc_function_entries *functions;
    size_t n_functions;
    size_t cap_functions;
} pk_c_kdoc_index;

/* Build a kernel-doc index from `source`. Returns a heap-allocated
 * index (caller frees via pk_c_kdoc_index_free). On allocation failure
 * the index is freed and NULL is returned. A NULL `source` yields an
 * empty (non-NULL) index. */
pk_c_kdoc_index *pk_c_kdoc_index_build(const char *source);

/* Look up the entries for `function_name`. Returns NULL if absent. */
const pk_c_kdoc_function_entries *pk_c_kdoc_index_lookup(
    const pk_c_kdoc_index *index,
    const char *function_name);

void pk_c_kdoc_index_free(pk_c_kdoc_index *index);

#endif
