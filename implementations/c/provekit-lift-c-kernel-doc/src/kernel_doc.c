#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/c_lift_core.h"
#include "kdoc_index.h"

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} LineArray;

static char *copy_n(const char *src, size_t len) {
    char *out = malloc(len + 1);

    if (!out) {
        return NULL;
    }
    memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

static char *copy_str(const char *src) {
    return copy_n(src == NULL ? "" : src, strlen(src == NULL ? "" : src));
}

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

static char *trim_copy(const char *src) {
    const char *start = src == NULL ? "" : src;
    const char *end;

    while (*start != '\0' && isspace((unsigned char)*start)) {
        start++;
    }
    end = start + strlen(start);
    while (end > start && isspace((unsigned char)end[-1])) {
        end--;
    }
    return copy_n(start, (size_t)(end - start));
}

static int line_array_push(LineArray *lines, char *line) {
    char **items;
    size_t cap;

    if (lines->len >= lines->cap) {
        cap = lines->cap == 0 ? 16 : lines->cap * 2;
        if (cap < lines->cap) {
            free(line);
            return -1;
        }
        items = realloc(lines->items, cap * sizeof(*lines->items));
        if (items == NULL) {
            free(line);
            return -1;
        }
        lines->items = items;
        lines->cap = cap;
    }
    lines->items[lines->len++] = line;
    return 0;
}

static void line_array_free(LineArray *lines) {
    if (lines == NULL) {
        return;
    }
    for (size_t i = 0; i < lines->len; i++) {
        free(lines->items[i]);
    }
    free(lines->items);
    lines->items = NULL;
    lines->len = 0;
    lines->cap = 0;
}

static int split_lines(const char *source, LineArray *lines) {
    const char *p = source == NULL ? "" : source;

    while (*p != '\0') {
        const char *start = p;

        while (*p != '\0' && *p != '\n') {
            p++;
        }
        if (line_array_push(lines, copy_n(start, (size_t)(p - start))) != 0) {
            return -1;
        }
        if (*p == '\n') {
            p++;
        }
    }
    return 0;
}

static const char *first_nonblank(const char *line) {
    const char *p = line == NULL ? "" : line;

    while (*p != '\0' && isspace((unsigned char)*p)) {
        p++;
    }
    return p;
}

static int is_blank(const char *line) {
    return *first_nonblank(line) == '\0';
}

static int has_prefix(const char *s, const char *prefix) {
    return strncmp(s, prefix, strlen(prefix)) == 0;
}

static char lower_ascii(char c) {
    return (char)tolower((unsigned char)c);
}

static int contains_ci(const char *haystack, const char *needle) {
    size_t needle_len = strlen(needle);

    if (needle_len == 0) {
        return 1;
    }
    for (const char *h = haystack == NULL ? "" : haystack; *h != '\0'; h++) {
        size_t i;

        for (i = 0; i < needle_len; i++) {
            if (h[i] == '\0' || lower_ascii(h[i]) != lower_ascii(needle[i])) {
                break;
            }
        }
        if (i == needle_len) {
            return 1;
        }
    }
    return 0;
}

static int append_core_result(pk_c_lift_result *result, const pk_c_source_facts *facts) {
    if (facts == NULL || facts->extraction_result == NULL) {
        return 0;
    }
    return pk_c_lift_result_extend(result, facts->extraction_result);
}

#ifdef PK_C_ENABLE_FFI_COMPOSE
/* Forward declaration of the composition pass. Lives in composition.c
 * and links in only when the libclang AST backend is enabled (the
 * regex-only stub build has no per-function effects, so composition
 * cannot soundly classify pure subtrees). The optional kernel-doc
 * index lets composition build real FunctionContractMemento envelopes
 * carrying the function's accumulated kernel-doc preconditions; if the
 * index is NULL or a function has no entries, composition falls back
 * to the identity-shape body so the chain still composes. */
int pk_c_composition_emit(
    pk_c_lift_result *result,
    const pk_c_source_facts *facts,
    const pk_c_kdoc_index *kdoc);
#endif

static int add_contract(
    pk_c_lift_result *result,
    const char *name,
    const char *function_name,
    const char *binding_name
) {
    char *escaped_name = json_escape_fragment(name);
    char *escaped_function = json_escape_fragment(function_name);
    char *escaped_binding = json_escape_fragment(binding_name);
    char *json;
    int written;
    int rc;

    if (escaped_name == NULL || escaped_function == NULL || escaped_binding == NULL) {
        free(escaped_name);
        free(escaped_function);
        free(escaped_binding);
        return -1;
    }

    written = snprintf(NULL,
        0,
        "{\"function\":\"%s\",\"kind\":\"contract\",\"name\":\"%s\","
        "\"outBinding\":\"out\",\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"%s\"}],"
        "\"kind\":\"atomic\",\"name\":\"%s\"}}",
        escaped_function,
        escaped_name,
        escaped_binding,
        escaped_name);
    if (written < 0) {
        free(escaped_name);
        free(escaped_function);
        free(escaped_binding);
        return -1;
    }
    json = malloc((size_t)written + 1);
    if (json == NULL) {
        free(escaped_name);
        free(escaped_function);
        free(escaped_binding);
        return -1;
    }
    (void)snprintf(json,
        (size_t)written + 1,
        "{\"function\":\"%s\",\"kind\":\"contract\",\"name\":\"%s\","
        "\"outBinding\":\"out\",\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"%s\"}],"
        "\"kind\":\"atomic\",\"name\":\"%s\"}}",
        escaped_function,
        escaped_name,
        escaped_binding,
        escaped_name);

    rc = pk_c_lift_result_add_declaration(result, json);
    free(json);
    free(escaped_name);
    free(escaped_function);
    free(escaped_binding);
    return rc;
}

static int add_diagnostic(
    pk_c_lift_result *result,
    const char *kind,
    const char *path,
    int line,
    const char *message
) {
    char *escaped_kind = json_escape_fragment(kind);
    char *escaped_path = json_escape_fragment(path);
    char *escaped_message = json_escape_fragment(message);
    char *json;
    int written;
    int rc;

    if (escaped_kind == NULL || escaped_path == NULL || escaped_message == NULL) {
        free(escaped_kind);
        free(escaped_path);
        free(escaped_message);
        return -1;
    }
    written = snprintf(NULL,
        0,
        "{\"kind\":\"%s\",\"locus\":{\"column\":1,\"line\":%d,\"path\":\"%s\"},"
        "\"message\":\"%s\",\"severity\":\"warning\"}",
        escaped_kind,
        line,
        escaped_path,
        escaped_message);
    if (written < 0) {
        free(escaped_kind);
        free(escaped_path);
        free(escaped_message);
        return -1;
    }
    json = malloc((size_t)written + 1);
    if (json == NULL) {
        free(escaped_kind);
        free(escaped_path);
        free(escaped_message);
        return -1;
    }
    (void)snprintf(json,
        (size_t)written + 1,
        "{\"kind\":\"%s\",\"locus\":{\"column\":1,\"line\":%d,\"path\":\"%s\"},"
        "\"message\":\"%s\",\"severity\":\"warning\"}",
        escaped_kind,
        line,
        escaped_path,
        escaped_message);
    rc = pk_c_lift_result_add_diagnostic(result, json);
    free(json);
    free(escaped_kind);
    free(escaped_path);
    free(escaped_message);
    return rc;
}

static char *clean_doc_line(const char *line) {
    char *work = copy_str(line);
    char *start;
    char *end_marker;
    char *trimmed;

    if (work == NULL) {
        return NULL;
    }
    start = strstr(work, "/**");
    if (start != NULL) {
        memmove(work, start + 3, strlen(start + 3) + 1);
    }
    end_marker = strstr(work, "*/");
    if (end_marker != NULL) {
        *end_marker = '\0';
    }
    start = (char *)first_nonblank(work);
    if (*start == '*') {
        start++;
        if (*start == ' ') {
            start++;
        }
    }
    trimmed = trim_copy(start);
    free(work);
    return trimmed;
}

static char *extract_function_name(const char *line) {
    const char *open = strchr(line == NULL ? "" : line, '(');
    const char *end;
    const char *start;

    if (open == NULL) {
        return NULL;
    }
    end = open;
    while (end > line && isspace((unsigned char)end[-1])) {
        end--;
    }
    start = end;
    while (start > line &&
        (isalnum((unsigned char)start[-1]) || start[-1] == '_')) {
        start--;
    }
    if (start == end) {
        return NULL;
    }
    return copy_n(start, (size_t)(end - start));
}

static char *extract_doc_param(const char *doc_line) {
    const char *p = doc_line;
    const char *start;

    if (p == NULL || *p != '@') {
        return NULL;
    }
    p++;
    start = p;
    while (*p != '\0' && *p != ':' && !isspace((unsigned char)*p)) {
        p++;
    }
    if (p == start) {
        return NULL;
    }
    return copy_n(start, (size_t)(p - start));
}

static char *extract_must_hold_lock(const char *doc_line) {
    const char *held;
    const char *end;
    const char *start;

    held = strstr(doc_line == NULL ? "" : doc_line, " held");
    if (held == NULL) {
        return copy_str("lock");
    }
    end = held;
    while (end > doc_line && isspace((unsigned char)end[-1])) {
        end--;
    }
    start = end;
    while (start > doc_line &&
        (isalnum((unsigned char)start[-1]) || start[-1] == '_')) {
        start--;
    }
    if (start == end) {
        return copy_str("lock");
    }
    return copy_n(start, (size_t)(end - start));
}

static int process_doc_comment(
    pk_c_lift_result *result,
    const LineArray *lines,
    size_t start,
    size_t end,
    const char *path
) {
    size_t attach = end + 1;
    char *function_name = NULL;

    while (attach < lines->len && is_blank(lines->items[attach])) {
        attach++;
    }
    if (attach >= lines->len) {
        return add_diagnostic(result,
            "c-kernel-doc.unattached-comment",
            path,
            (int)start + 1,
            "kernel-doc comment is not followed by a function declaration");
    }
    if (*first_nonblank(lines->items[attach]) == '#') {
        return pk_c_lift_result_add_opacity_entry(result,
            "c-kernel-doc.conditional-attachment",
            path,
            (int)start + 1,
            1,
            "kernel-doc comment is separated from the declaration by a preprocessor directive",
            "c-kernel-doc");
    }

    function_name = extract_function_name(lines->items[attach]);
    if (function_name == NULL) {
        return add_diagnostic(result,
            "c-kernel-doc.unattached-comment",
            path,
            (int)start + 1,
            "kernel-doc comment is not attached to a recognizable function declaration");
    }

    for (size_t i = start; i <= end; i++) {
        char *doc = clean_doc_line(lines->items[i]);
        int rc = 0;

        if (doc == NULL) {
            free(function_name);
            return -1;
        }

        if (doc[0] == '@') {
            char *param = extract_doc_param(doc);

            if (param != NULL && contains_ci(doc, "must not be null")) {
                rc = add_contract(result,
                    "c-kernel-doc.param.nonnull",
                    function_name,
                    param);
            } else if (param != NULL && contains_ci(doc, "must be positive")) {
                rc = add_contract(result,
                    "c-kernel-doc.param.positive",
                    function_name,
                    param);
            }
            free(param);
        } else if (has_prefix(doc, "Context:") && contains_ci(doc, "held")) {
            char *lock = extract_must_hold_lock(doc);

            if (lock == NULL) {
                rc = -1;
            } else {
                rc = add_contract(result,
                    "c-kernel-doc.context.must-hold",
                    function_name,
                    lock);
                free(lock);
            }
        } else if (has_prefix(doc, "Return:")) {
            if (contains_ci(doc, "negative errno")) {
                rc = add_contract(result,
                    "c-kernel-doc.return.negative-errno",
                    function_name,
                    function_name);
            } else if (contains_ci(doc, "owns") && contains_ci(doc, "release")) {
                rc = pk_c_lift_result_add_refusal_entry(result,
                    "c-kernel-doc.unsupported-return-ownership",
                    path,
                    (int)i + 1,
                    1,
                    "c-kernel-doc",
                    "kernel-doc return ownership language is recognized but not modeled by this lifter");
            }
        }
        free(doc);
        if (rc != 0) {
            free(function_name);
            return -1;
        }
    }

    free(function_name);
    return 0;
}

static int scan_kernel_doc(pk_c_lift_result *result, const char *path, const char *source) {
    LineArray lines = {0};
    int rc = 0;

    if (split_lines(source, &lines) != 0) {
        line_array_free(&lines);
        return -1;
    }

    for (size_t i = 0; i < lines.len; i++) {
        const char *first = first_nonblank(lines.items[i]);

        if (!has_prefix(first, "/**")) {
            continue;
        }

        size_t end = i;
        while (end < lines.len && strstr(lines.items[end], "*/") == NULL) {
            end++;
        }
        if (end >= lines.len) {
            rc = add_diagnostic(result,
                "c-kernel-doc.unclosed-comment",
                path,
                (int)i + 1,
                "kernel-doc comment is not closed");
            break;
        }
        rc = process_doc_comment(result, &lines, i, end, path);
        if (rc != 0) {
            break;
        }
        i = end;
    }

    line_array_free(&lines);
    return rc;
}

/* -------------------------------------------------------------------- */
/* Kernel-doc index builder. Mirrors scan_kernel_doc's traversal, but
 * appends to a (function, kind, binding) triple table instead of
 * emitting JSON declarations. The composition pass consumes the index
 * to construct real FunctionContractMemento envelopes for the
 * libprovekit compose FFI per CCP v1.0.0 sections 2 + 9. Functions
 * with zero kernel-doc entries are absent from the index; the
 * composition pass falls back to the identity-shape body for them. */

static pk_c_kdoc_function_entries *kdoc_index_get_or_create(
    pk_c_kdoc_index *index,
    const char *function_name
) {
    for (size_t i = 0; i < index->n_functions; i++) {
        if (strcmp(index->functions[i].function, function_name) == 0) {
            return &index->functions[i];
        }
    }
    if (index->n_functions >= index->cap_functions) {
        size_t next = index->cap_functions == 0 ? 4 : index->cap_functions * 2;
        pk_c_kdoc_function_entries *resized = realloc(
            index->functions, next * sizeof(*resized));
        if (resized == NULL) {
            return NULL;
        }
        index->functions = resized;
        index->cap_functions = next;
    }
    pk_c_kdoc_function_entries *fe = &index->functions[index->n_functions];
    fe->function = copy_str(function_name);
    if (fe->function == NULL) {
        return NULL;
    }
    fe->entries = NULL;
    fe->n_entries = 0;
    fe->cap_entries = 0;
    index->n_functions++;
    return fe;
}

static int kdoc_function_entries_append(
    pk_c_kdoc_function_entries *fe,
    const char *kind,
    const char *binding
) {
    /* De-duplicate exact (kind, binding) pairs: kernel-doc commonly
     * names the same parameter once; defensive against accidental
     * duplicates that would inflate the pre-condition list and skew
     * composed CIDs. */
    for (size_t i = 0; i < fe->n_entries; i++) {
        if (strcmp(fe->entries[i].kind, kind) == 0 &&
            strcmp(fe->entries[i].binding, binding) == 0) {
            return 0;
        }
    }
    if (fe->n_entries >= fe->cap_entries) {
        size_t next = fe->cap_entries == 0 ? 4 : fe->cap_entries * 2;
        pk_c_kdoc_entry *resized = realloc(
            fe->entries, next * sizeof(*resized));
        if (resized == NULL) {
            return -1;
        }
        fe->entries = resized;
        fe->cap_entries = next;
    }
    fe->entries[fe->n_entries].kind = copy_str(kind);
    fe->entries[fe->n_entries].binding = copy_str(binding);
    if (fe->entries[fe->n_entries].kind == NULL ||
        fe->entries[fe->n_entries].binding == NULL) {
        free(fe->entries[fe->n_entries].kind);
        free(fe->entries[fe->n_entries].binding);
        return -1;
    }
    fe->n_entries++;
    return 0;
}

static int index_doc_comment(
    pk_c_kdoc_index *index,
    const LineArray *lines,
    size_t start,
    size_t end
) {
    size_t attach = end + 1;
    char *function_name = NULL;
    pk_c_kdoc_function_entries *fe = NULL;

    while (attach < lines->len && is_blank(lines->items[attach])) {
        attach++;
    }
    if (attach >= lines->len) {
        return 0;
    }
    if (*first_nonblank(lines->items[attach]) == '#') {
        return 0;
    }
    function_name = extract_function_name(lines->items[attach]);
    if (function_name == NULL) {
        return 0;
    }

    for (size_t i = start; i <= end; i++) {
        char *doc = clean_doc_line(lines->items[i]);
        const char *kind = NULL;
        char *binding = NULL;
        int rc = 0;

        if (doc == NULL) {
            free(function_name);
            return -1;
        }

        if (doc[0] == '@') {
            char *param = extract_doc_param(doc);
            if (param != NULL && contains_ci(doc, "must not be null")) {
                kind = "c-kernel-doc.param.nonnull";
                binding = param;
            } else if (param != NULL && contains_ci(doc, "must be positive")) {
                kind = "c-kernel-doc.param.positive";
                binding = param;
            } else {
                free(param);
            }
        } else if (has_prefix(doc, "Context:") && contains_ci(doc, "held")) {
            char *lock = extract_must_hold_lock(doc);
            if (lock != NULL) {
                kind = "c-kernel-doc.context.must-hold";
                binding = lock;
            }
        } else if (has_prefix(doc, "Return:") &&
                   contains_ci(doc, "negative errno")) {
            kind = "c-kernel-doc.return.negative-errno";
            binding = copy_str(function_name);
            if (binding == NULL) {
                free(doc);
                free(function_name);
                return -1;
            }
        }

        if (kind != NULL && binding != NULL) {
            if (fe == NULL) {
                fe = kdoc_index_get_or_create(index, function_name);
                if (fe == NULL) {
                    free(binding);
                    free(doc);
                    free(function_name);
                    return -1;
                }
            }
            rc = kdoc_function_entries_append(fe, kind, binding);
        }
        free(binding);
        free(doc);
        if (rc != 0) {
            free(function_name);
            return -1;
        }
    }

    free(function_name);
    return 0;
}

pk_c_kdoc_index *pk_c_kdoc_index_build(const char *source) {
    pk_c_kdoc_index *index = calloc(1, sizeof(*index));
    LineArray lines = {0};
    int rc = 0;

    if (index == NULL) {
        return NULL;
    }
    if (source == NULL) {
        return index;
    }
    if (split_lines(source, &lines) != 0) {
        line_array_free(&lines);
        pk_c_kdoc_index_free(index);
        return NULL;
    }
    for (size_t i = 0; i < lines.len; i++) {
        const char *first = first_nonblank(lines.items[i]);
        if (!has_prefix(first, "/**")) {
            continue;
        }
        size_t end = i;
        while (end < lines.len && strstr(lines.items[end], "*/") == NULL) {
            end++;
        }
        if (end >= lines.len) {
            break;
        }
        rc = index_doc_comment(index, &lines, i, end);
        if (rc != 0) {
            break;
        }
        i = end;
    }
    line_array_free(&lines);
    if (rc != 0) {
        pk_c_kdoc_index_free(index);
        return NULL;
    }
    return index;
}

const pk_c_kdoc_function_entries *pk_c_kdoc_index_lookup(
    const pk_c_kdoc_index *index,
    const char *function_name
) {
    if (index == NULL || function_name == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < index->n_functions; i++) {
        if (strcmp(index->functions[i].function, function_name) == 0) {
            return &index->functions[i];
        }
    }
    return NULL;
}

void pk_c_kdoc_index_free(pk_c_kdoc_index *index) {
    if (index == NULL) {
        return;
    }
    for (size_t i = 0; i < index->n_functions; i++) {
        pk_c_kdoc_function_entries *fe = &index->functions[i];
        for (size_t j = 0; j < fe->n_entries; j++) {
            free(fe->entries[j].kind);
            free(fe->entries[j].binding);
        }
        free(fe->entries);
        free(fe->function);
    }
    free(index->functions);
    free(index);
}

pk_c_lift_result *pk_c_kernel_doc_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    pk_c_source_facts *facts;

    if (!result) {
        return NULL;
    }
    if (!source) {
        return result;
    }

    facts = pk_c_parse_source_with_options(path, source, options);
    if (facts == NULL) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"message\":\"parse failed\",\"severity\":\"error\"}");
        return result;
    }
    if (append_core_result(result, facts) != 0) {
        pk_c_source_facts_free(facts);
        pk_c_lift_result_free(result);
        return NULL;
    }

#ifdef PK_C_ENABLE_FFI_COMPOSE
    /* Composition pass (CCP §4 eager materialization). Walks the
     * call-site graph the parser already populated, finds pure
     * subtrees, and emits ComposedFunctionContract mementos via the
     * canonical libprovekit FFI. Soft failure: chain-by-chain refusals
     * are silently skipped; only an OOM bubbles up.
     *
     * Build the kernel-doc index first so composition can derive real
     * FunctionContractMemento envelopes from each function's
     * accumulated preconditions per CCP §2 + §9. The index lifecycle
     * is bounded to this call: composition reads it, then it is freed
     * before scan_kernel_doc emits the per-precondition declarations
     * (which remain unchanged so the existing IR output is stable). */
    pk_c_kdoc_index *kdoc = pk_c_kdoc_index_build(source);
    if (kdoc == NULL) {
        pk_c_source_facts_free(facts);
        pk_c_lift_result_free(result);
        return NULL;
    }
    int compose_rc = pk_c_composition_emit(result, facts, kdoc);
    pk_c_kdoc_index_free(kdoc);
    if (compose_rc != 0) {
        pk_c_source_facts_free(facts);
        pk_c_lift_result_free(result);
        return NULL;
    }
#endif

    pk_c_source_facts_free(facts);

    if (scan_kernel_doc(result, path == NULL ? "" : path, source) != 0) {
        pk_c_lift_result_free(result);
        return NULL;
    }
    return result;
}

pk_c_lift_result *pk_c_kernel_doc_lift_source(const char *path, const char *source) {
    return pk_c_kernel_doc_lift_source_with_options(path, source, NULL);
}
