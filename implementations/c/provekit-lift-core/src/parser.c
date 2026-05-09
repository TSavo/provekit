#include "provekit/c_lift_core.h"

#include <ctype.h>
#include <regex.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PK_C_NO_FUNCTION SIZE_MAX

static const char *pk_c_parser_first_nonblank(const char *line);
static char *pk_c_parser_argument_text(const char *open_paren);

static char *pk_c_parser_copy_n(const char *src, size_t len) {
    char *copy = malloc(len + 1);

    if (copy == NULL) {
        return NULL;
    }
    memcpy(copy, src, len);
    copy[len] = '\0';
    return copy;
}

static char *pk_c_parser_copy(const char *src) {
    if (src == NULL) {
        return NULL;
    }
    return pk_c_parser_copy_n(src, strlen(src));
}

static int pk_c_parser_checked_mul(size_t lhs, size_t rhs, size_t *out) {
    if (lhs != 0 && rhs > SIZE_MAX / lhs) {
        return -1;
    }
    *out = lhs * rhs;
    return 0;
}

static int pk_c_parser_blank_line(const char *line) {
    const unsigned char *p = (const unsigned char *)line;

    while (*p != '\0') {
        if (!isspace(*p)) {
            return 0;
        }
        p++;
    }
    return 1;
}

static int pk_c_parser_contract_annotation(const char *line, int in_block_comment) {
    const char *p = pk_c_parser_first_nonblank(line);

    if (in_block_comment) {
        return 0;
    }
    return strncmp(p, "//provekit:contract", strlen("//provekit:contract")) == 0;
}

static int pk_c_parser_keyword(const char *name) {
    static const char *const keywords[] = {
        "auto", "break", "case", "char", "const", "continue", "default", "do",
        "double", "else", "enum", "extern", "float", "for", "goto", "if",
        "inline", "int", "long", "register", "restrict", "return", "short",
        "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
        "unsigned", "void", "volatile", "while", "_Alignas", "_Alignof",
        "_Atomic", "_Bool", "_Complex", "_Generic", "_Imaginary", "_Noreturn",
        "_Static_assert", "_Thread_local"
    };
    size_t i;

    for (i = 0; i < sizeof(keywords) / sizeof(keywords[0]); i++) {
        if (strcmp(name, keywords[i]) == 0) {
            return 1;
        }
    }
    return 0;
}

static int pk_c_parser_macro_name(const char *name) {
    unsigned char first;

    if (strncmp(name, "KUNIT_", 6) == 0) {
        return 1;
    }
    first = (unsigned char)name[0];
    return isupper(first) != 0;
}

static int pk_c_parser_definition_disallowed_prefix(const char *line) {
    char token[16];
    size_t len = 0;
    const unsigned char *p = (const unsigned char *)pk_c_parser_first_nonblank(line);

    while ((isalnum(*p) || *p == '_') && len + 1 < sizeof(token)) {
        token[len++] = (char)*p;
        p++;
    }
    token[len] = '\0';
    return strcmp(token, "return") == 0 || strcmp(token, "if") == 0 ||
        strcmp(token, "for") == 0 || strcmp(token, "while") == 0 ||
        strcmp(token, "switch") == 0 || strcmp(token, "sizeof") == 0;
}

static void pk_c_parser_set_locus(pk_c_locus *locus, const char *path, int line, int column) {
    locus->path = pk_c_parser_copy(path == NULL ? "" : path);
    locus->line = line;
    locus->column = column;
}

static int pk_c_parser_grow_functions(pk_c_source_facts *facts) {
    pk_c_function_fact *items;
    size_t cap;
    size_t bytes;

    if (facts->n_functions < facts->cap_functions) {
        return 0;
    }
    cap = facts->cap_functions == 0 ? 4 : facts->cap_functions * 2;
    if (cap < facts->cap_functions ||
        pk_c_parser_checked_mul(cap, sizeof(*facts->functions), &bytes) != 0) {
        return -1;
    }
    items = realloc(facts->functions, bytes);
    if (items == NULL) {
        return -1;
    }
    facts->functions = items;
    facts->cap_functions = cap;
    return 0;
}

static int pk_c_parser_grow_macros(pk_c_source_facts *facts) {
    pk_c_macro_call_fact *items;
    size_t cap;
    size_t bytes;

    if (facts->n_macro_calls < facts->cap_macro_calls) {
        return 0;
    }
    cap = facts->cap_macro_calls == 0 ? 4 : facts->cap_macro_calls * 2;
    if (cap < facts->cap_macro_calls ||
        pk_c_parser_checked_mul(cap, sizeof(*facts->macro_calls), &bytes) != 0) {
        return -1;
    }
    items = realloc(facts->macro_calls, bytes);
    if (items == NULL) {
        return -1;
    }
    facts->macro_calls = items;
    facts->cap_macro_calls = cap;
    return 0;
}

static int pk_c_parser_grow_sparse_annotations(pk_c_source_facts *facts) {
    pk_c_sparse_annotation_fact *items;
    size_t cap;
    size_t bytes;

    if (facts->n_sparse_annotations < facts->cap_sparse_annotations) {
        return 0;
    }
    cap = facts->cap_sparse_annotations == 0 ? 4 : facts->cap_sparse_annotations * 2;
    if (cap < facts->cap_sparse_annotations ||
        pk_c_parser_checked_mul(cap, sizeof(*facts->sparse_annotations), &bytes) != 0) {
        return -1;
    }
    items = realloc(facts->sparse_annotations, bytes);
    if (items == NULL) {
        return -1;
    }
    facts->sparse_annotations = items;
    facts->cap_sparse_annotations = cap;
    return 0;
}

static int pk_c_parser_grow_calls(pk_c_source_facts *facts) {
    pk_c_call_site_fact *items;
    size_t cap;
    size_t bytes;

    if (facts->n_call_sites < facts->cap_call_sites) {
        return 0;
    }
    cap = facts->cap_call_sites == 0 ? 4 : facts->cap_call_sites * 2;
    if (cap < facts->cap_call_sites ||
        pk_c_parser_checked_mul(cap, sizeof(*facts->call_sites), &bytes) != 0) {
        return -1;
    }
    items = realloc(facts->call_sites, bytes);
    if (items == NULL) {
        return -1;
    }
    facts->call_sites = items;
    facts->cap_call_sites = cap;
    return 0;
}

static char *pk_c_parser_argument_text_or_empty(const char *open_paren) {
    if (open_paren == NULL) {
        return pk_c_parser_copy("");
    }
    return pk_c_parser_argument_text(open_paren);
}

static int pk_c_parser_append_sparse_annotation(
    pk_c_source_facts *facts,
    const char *path,
    const char *name,
    size_t name_len,
    const char *caller,
    const char *open_paren,
    int line,
    int column
) {
    pk_c_sparse_annotation_fact *fact;

    if (pk_c_parser_grow_sparse_annotations(facts) != 0) {
        return -1;
    }
    fact = &facts->sparse_annotations[facts->n_sparse_annotations];
    memset(fact, 0, sizeof(*fact));
    fact->name = pk_c_parser_copy_n(name, name_len);
    fact->enclosing_function = pk_c_parser_copy(caller);
    fact->argument_text = pk_c_parser_argument_text_or_empty(open_paren);
    pk_c_parser_set_locus(&fact->locus, path, line, column);
    if (fact->name == NULL || fact->enclosing_function == NULL ||
        fact->argument_text == NULL || fact->locus.path == NULL) {
        free(fact->name);
        free(fact->enclosing_function);
        free(fact->argument_text);
        free(fact->locus.path);
        memset(fact, 0, sizeof(*fact));
        return -1;
    }
    facts->n_sparse_annotations++;
    return 0;
}

static int pk_c_parser_append_function(
    pk_c_source_facts *facts,
    const char *path,
    const char *name,
    size_t name_len,
    int line,
    int column,
    int has_contract_annotation
) {
    pk_c_function_fact *fact;

    if (pk_c_parser_grow_functions(facts) != 0) {
        return -1;
    }
    fact = &facts->functions[facts->n_functions];
    memset(fact, 0, sizeof(*fact));
    fact->name = pk_c_parser_copy_n(name, name_len);
    pk_c_parser_set_locus(&fact->locus, path, line, column);
    if (fact->name == NULL || fact->locus.path == NULL) {
        free(fact->name);
        free(fact->locus.path);
        memset(fact, 0, sizeof(*fact));
        return -1;
    }
    fact->has_contract_annotation = has_contract_annotation;
    facts->n_functions++;
    return 0;
}

static int pk_c_parser_sparse_annotation_name(const char *name, size_t len) {
    static const char *const names[] = {
        "__user", "__rcu", "__must_hold", "__acquires", "__releases"
    };
    size_t i;

    for (i = 0; i < sizeof(names) / sizeof(names[0]); i++) {
        if (strlen(names[i]) == len && strncmp(name, names[i], len) == 0) {
            return 1;
        }
    }
    return 0;
}

static int pk_c_parser_scan_sparse_annotations(
    pk_c_source_facts *facts,
    const char *path,
    const char *line,
    int line_no,
    int column_offset,
    const char *caller
) {
    const char *p = line;

    while (*p != '\0') {
        const char *start;
        const char *end;
        const char *arg = NULL;

        if (!isalpha((unsigned char)*p) && *p != '_') {
            p++;
            continue;
        }
        start = p;
        p++;
        while (isalnum((unsigned char)*p) || *p == '_') {
            p++;
        }
        end = p;
        if (!pk_c_parser_sparse_annotation_name(start, (size_t)(end - start))) {
            continue;
        }
        while (isspace((unsigned char)*p)) {
            p++;
        }
        if (*p == '(') {
            arg = p;
        }
        if (pk_c_parser_append_sparse_annotation(facts, path, start, (size_t)(end - start),
            caller, arg, line_no, (int)(start - line) + column_offset + 1) != 0) {
            return -1;
        }
    }
    return 0;
}

static char *pk_c_parser_argument_text(const char *open_paren) {
    const char *start = open_paren + 1;
    const char *end = start;
    int depth = 1;

    while (*end != '\0' && depth > 0) {
        if (*end == '(') {
            depth++;
        } else if (*end == ')') {
            depth--;
            if (depth == 0) {
                break;
            }
        }
        end++;
    }
    return pk_c_parser_copy_n(start, (size_t)(end - start));
}

static int pk_c_parser_append_macro(
    pk_c_source_facts *facts,
    const char *path,
    const char *name,
    const char *caller,
    const char *open_paren,
    int line,
    int column
) {
    pk_c_macro_call_fact *fact;

    if (pk_c_parser_grow_macros(facts) != 0) {
        return -1;
    }
    fact = &facts->macro_calls[facts->n_macro_calls];
    memset(fact, 0, sizeof(*fact));
    fact->name = pk_c_parser_copy(name);
    fact->enclosing_function = pk_c_parser_copy(caller);
    fact->argument_text = pk_c_parser_argument_text(open_paren);
    pk_c_parser_set_locus(&fact->locus, path, line, column);
    if (fact->name == NULL || fact->enclosing_function == NULL ||
        fact->argument_text == NULL || fact->locus.path == NULL) {
        free(fact->name);
        free(fact->enclosing_function);
        free(fact->argument_text);
        free(fact->locus.path);
        memset(fact, 0, sizeof(*fact));
        return -1;
    }
    facts->n_macro_calls++;
    return 0;
}

static int pk_c_parser_append_call(
    pk_c_source_facts *facts,
    const char *path,
    const char *caller,
    const char *callee,
    int line,
    int column
) {
    pk_c_call_site_fact *fact;

    if (pk_c_parser_grow_calls(facts) != 0) {
        return -1;
    }
    fact = &facts->call_sites[facts->n_call_sites];
    memset(fact, 0, sizeof(*fact));
    fact->caller = pk_c_parser_copy(caller);
    fact->callee = pk_c_parser_copy(callee);
    pk_c_parser_set_locus(&fact->locus, path, line, column);
    if (fact->caller == NULL || fact->callee == NULL || fact->locus.path == NULL) {
        free(fact->caller);
        free(fact->callee);
        free(fact->locus.path);
        memset(fact, 0, sizeof(*fact));
        return -1;
    }
    facts->n_call_sites++;
    return 0;
}

static const char *pk_c_parser_first_nonblank(const char *line) {
    const unsigned char *p = (const unsigned char *)line;

    while (*p != '\0' && isspace(*p)) {
        p++;
    }
    return (const char *)p;
}

static int pk_c_parser_brace_delta_from(const char *line) {
    int delta = 0;

    while (*line != '\0') {
        if (*line == '{') {
            delta++;
        } else if (*line == '}') {
            delta--;
        }
        line++;
    }
    return delta;
}

static int pk_c_parser_regex_error(pk_c_source_facts *facts, const char *which) {
    facts->extraction_result = pk_c_lift_result_new();
    if (facts->extraction_result != NULL) {
        (void)pk_c_lift_result_add_diagnostic(
            facts->extraction_result,
            which
        );
    }
    return -1;
}

static int pk_c_parser_scan_calls(
    pk_c_source_facts *facts,
    regex_t *call_re,
    const char *path,
    const char *line,
    int line_no,
    int column_offset,
    const char *caller
) {
    const char *cursor = line;
    regmatch_t match[2];

    while (regexec(call_re, cursor, 2, match, 0) == 0) {
        char *name;
        int column;

        if (match[1].rm_so < 0) {
            break;
        }
        name = pk_c_parser_copy_n(cursor + match[1].rm_so,
            (size_t)(match[1].rm_eo - match[1].rm_so));
        if (name == NULL) {
            return -1;
        }
        column = (int)((cursor - line) + match[1].rm_so + column_offset + 1);
        if (!pk_c_parser_keyword(name)) {
            if (pk_c_parser_macro_name(name)) {
                if (pk_c_parser_append_macro(facts, path, name, caller,
                    cursor + match[0].rm_eo - 1, line_no, column) != 0) {
                    free(name);
                    return -1;
                }
            } else if (pk_c_parser_append_call(facts, path, caller, name, line_no, column) != 0) {
                free(name);
                return -1;
            }
        }
        free(name);
        if (match[0].rm_eo <= 0) {
            break;
        }
        cursor += match[0].rm_eo;
    }
    return 0;
}

static char *pk_c_parser_code_view(const char *line, int *in_block_comment) {
    char *copy = pk_c_parser_copy(line);
    size_t i = 0;
    char quote = '\0';

    if (copy == NULL) {
        return NULL;
    }

    while (copy[i] != '\0') {
        if (*in_block_comment) {
            if (copy[i] == '*' && copy[i + 1] == '/') {
                copy[i] = ' ';
                copy[i + 1] = ' ';
                i += 2;
                *in_block_comment = 0;
            } else {
                copy[i++] = ' ';
            }
            continue;
        }

        if (quote != '\0') {
            if (copy[i] == '\\' && copy[i + 1] != '\0') {
                copy[i] = ' ';
                copy[i + 1] = ' ';
                i += 2;
                continue;
            }
            if (copy[i] == quote) {
                quote = '\0';
            }
            copy[i++] = ' ';
            continue;
        }

        if (copy[i] == '/' && copy[i + 1] == '/') {
            while (copy[i] != '\0') {
                copy[i++] = ' ';
            }
            break;
        }
        if (copy[i] == '/' && copy[i + 1] == '*') {
            copy[i] = ' ';
            copy[i + 1] = ' ';
            i += 2;
            *in_block_comment = 1;
            continue;
        }
        if (copy[i] == '"' || copy[i] == '\'') {
            quote = copy[i];
            copy[i++] = ' ';
            continue;
        }
        i++;
    }

    return copy;
}

void pk_c_source_facts_free(pk_c_source_facts *facts) {
    size_t i;

    if (facts == NULL) {
        return;
    }
    for (i = 0; i < facts->n_functions; i++) {
        free(facts->functions[i].name);
        free(facts->functions[i].locus.path);
    }
    for (i = 0; i < facts->n_macro_calls; i++) {
        free(facts->macro_calls[i].name);
        free(facts->macro_calls[i].enclosing_function);
        free(facts->macro_calls[i].argument_text);
        free(facts->macro_calls[i].locus.path);
    }
    for (i = 0; i < facts->n_sparse_annotations; i++) {
        free(facts->sparse_annotations[i].name);
        free(facts->sparse_annotations[i].enclosing_function);
        free(facts->sparse_annotations[i].argument_text);
        free(facts->sparse_annotations[i].locus.path);
    }
    for (i = 0; i < facts->n_call_sites; i++) {
        free(facts->call_sites[i].caller);
        free(facts->call_sites[i].callee);
        free(facts->call_sites[i].args_json);
        free(facts->call_sites[i].locus.path);
    }
    free(facts->functions);
    free(facts->macro_calls);
    free(facts->sparse_annotations);
    free(facts->call_sites);
    pk_c_lift_result_free(facts->extraction_result);
    free(facts->parser_backend);
    free(facts->parser_compile_command);
    free(facts->parser_target_triple);
    free(facts);
}

char *pk_c_lift_json_escape(const char *src) {
    size_t total = 0;
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
        if (add > ((size_t)-1) - total) {
            return NULL;
        }
        total += add;
    }
    out = malloc(total + 1);
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

static int pk_c_emit_one_call_edge(
    pk_c_lift_result *result,
    const pk_c_call_site_fact *fact
) {
    char *caller = pk_c_lift_json_escape(fact->caller);
    char *callee = pk_c_lift_json_escape(fact->callee);
    char *path = pk_c_lift_json_escape(fact->locus.path);
    const char *args = (fact->args_json != NULL && fact->args_json[0] != '\0')
        ? fact->args_json
        : "[]";
    char *json = NULL;
    int written;
    int rc;

    if (caller == NULL || callee == NULL || path == NULL) {
        free(caller);
        free(callee);
        free(path);
        return -1;
    }
    written = snprintf(NULL,
        0,
        "{\"caller_function\":\"%s\",\"callee_name\":\"%s\","
        "\"args\":%s,\"callsite_path\":\"%s\","
        "\"callsite_line\":%d,\"callsite_column\":%d}",
        caller,
        callee,
        args,
        path,
        fact->locus.line,
        fact->locus.column);
    if (written < 0) {
        free(caller);
        free(callee);
        free(path);
        return -1;
    }
    json = malloc((size_t)written + 1);
    if (json == NULL) {
        free(caller);
        free(callee);
        free(path);
        return -1;
    }
    (void)snprintf(json,
        (size_t)written + 1,
        "{\"caller_function\":\"%s\",\"callee_name\":\"%s\","
        "\"args\":%s,\"callsite_path\":\"%s\","
        "\"callsite_line\":%d,\"callsite_column\":%d}",
        caller,
        callee,
        args,
        path,
        fact->locus.line,
        fact->locus.column);
    rc = pk_c_lift_result_add_call_edge(result, json);
    free(json);
    free(caller);
    free(callee);
    free(path);
    return rc;
}

int pk_c_emit_call_edges(pk_c_source_facts *facts) {
    if (facts == NULL || facts->n_call_sites == 0) {
        return 0;
    }
    if (facts->extraction_result == NULL) {
        facts->extraction_result = pk_c_lift_result_new();
        if (facts->extraction_result == NULL) {
            return -1;
        }
    }
    for (size_t i = 0; i < facts->n_call_sites; i++) {
        if (pk_c_emit_one_call_edge(facts->extraction_result,
                &facts->call_sites[i]) != 0) {
            return -1;
        }
    }
    return 0;
}

pk_c_source_facts *pk_c_parse_source(const char *path, const char *source) {
    static const char *const function_pattern =
        "^[[:space:]]*[A-Za-z_][A-Za-z0-9_ *]*[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\\(";
    static const char *const call_pattern =
        "([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\\(";
    pk_c_source_facts *facts = calloc(1, sizeof(*facts));
    regex_t function_re;
    regex_t call_re;
    int function_re_compiled = 0;
    int call_re_compiled = 0;
    char *owned_source;
    char *line;
    char *next;
    int line_no = 1;
    int pending_contract = 0;
    size_t pending_body = PK_C_NO_FUNCTION;
    size_t current_function = PK_C_NO_FUNCTION;
    int brace_depth = 0;
    int in_block_comment = 0;

    if (facts == NULL) {
        return NULL;
    }
    facts->parser_backend = pk_c_parser_copy("regex");
    if (facts->parser_backend == NULL) {
        pk_c_source_facts_free(facts);
        return NULL;
    }
    owned_source = pk_c_parser_copy(source == NULL ? "" : source);
    if (owned_source == NULL) {
        pk_c_source_facts_free(facts);
        return NULL;
    }
    if (regcomp(&function_re, function_pattern, REG_EXTENDED) != 0) {
        (void)pk_c_parser_regex_error(facts, "{\"severity\":\"error\",\"message\":\"function regex compile failed\"}");
        free(owned_source);
        return facts;
    }
    function_re_compiled = 1;
    if (regcomp(&call_re, call_pattern, REG_EXTENDED) != 0) {
        (void)pk_c_parser_regex_error(facts, "{\"severity\":\"error\",\"message\":\"call regex compile failed\"}");
        regfree(&function_re);
        free(owned_source);
        return facts;
    }
    call_re_compiled = 1;

    for (line = owned_source; line != NULL; line = next, line_no++) {
        regmatch_t function_match[2];
        const char *body_open;
        char *code_line;
        int is_blank;
        int line_started_in_block_comment;

        next = strchr(line, '\n');
        if (next != NULL) {
            *next = '\0';
            next++;
        }

        line_started_in_block_comment = in_block_comment;
        code_line = pk_c_parser_code_view(line, &in_block_comment);
        if (code_line == NULL) {
            pk_c_source_facts_free(facts);
            facts = NULL;
            goto done;
        }

        is_blank = pk_c_parser_blank_line(code_line);
        if (pending_body != PK_C_NO_FUNCTION && !is_blank) {
            const char *first = pk_c_parser_first_nonblank(code_line);

            if (*first == '{') {
                facts->functions[pending_body].has_body = 1;
                current_function = pending_body;
                brace_depth = pk_c_parser_brace_delta_from(first);
            }
            pending_body = PK_C_NO_FUNCTION;
            if (current_function != PK_C_NO_FUNCTION && brace_depth <= 0) {
                current_function = PK_C_NO_FUNCTION;
                brace_depth = 0;
            }
            if (*first == '{') {
                free(code_line);
                continue;
            }
        }

        if (pk_c_parser_contract_annotation(line, line_started_in_block_comment)) {
            pending_contract = 1;
            free(code_line);
            continue;
        }

        if (!pk_c_parser_definition_disallowed_prefix(code_line) &&
            regexec(&function_re, code_line, 2, function_match, 0) == 0 &&
            function_match[1].rm_so >= 0) {
            size_t function_index;

            if (pk_c_parser_append_function(facts, path, code_line + function_match[1].rm_so,
                (size_t)(function_match[1].rm_eo - function_match[1].rm_so), line_no,
                (int)function_match[1].rm_so + 1, pending_contract) != 0) {
                free(code_line);
                pk_c_source_facts_free(facts);
                facts = NULL;
                goto done;
            }
            pending_contract = 0;
            function_index = facts->n_functions - 1;
            if (pk_c_parser_scan_sparse_annotations(facts, path, code_line, line_no, 0,
                facts->functions[function_index].name) != 0) {
                free(code_line);
                pk_c_source_facts_free(facts);
                facts = NULL;
                goto done;
            }
            body_open = strchr(code_line + function_match[0].rm_eo, '{');
            if (body_open != NULL) {
                facts->functions[function_index].has_body = 1;
                if (pk_c_parser_scan_calls(facts, &call_re, path, body_open + 1, line_no,
                    (int)(body_open + 1 - code_line),
                    facts->functions[function_index].name) != 0) {
                    free(code_line);
                    pk_c_source_facts_free(facts);
                    facts = NULL;
                    goto done;
                }
                brace_depth = pk_c_parser_brace_delta_from(body_open);
                if (brace_depth > 0) {
                    current_function = function_index;
                } else {
                    current_function = PK_C_NO_FUNCTION;
                    brace_depth = 0;
                }
            } else {
                pending_body = function_index;
            }
            free(code_line);
            continue;
        }

        if (!is_blank) {
            pending_contract = 0;
        }

        if (current_function != PK_C_NO_FUNCTION) {
            if (pk_c_parser_scan_calls(facts, &call_re, path, code_line, line_no, 0,
                facts->functions[current_function].name) != 0) {
                free(code_line);
                pk_c_source_facts_free(facts);
                facts = NULL;
                goto done;
            }
            brace_depth += pk_c_parser_brace_delta_from(code_line);
            if (brace_depth <= 0) {
                current_function = PK_C_NO_FUNCTION;
                brace_depth = 0;
            }
        }
        free(code_line);
    }

done:
    if (call_re_compiled) {
        regfree(&call_re);
    }
    if (function_re_compiled) {
        regfree(&function_re);
    }
    free(owned_source);
    if (facts != NULL) {
        (void)pk_c_emit_call_edges(facts);
    }
    return facts;
}
