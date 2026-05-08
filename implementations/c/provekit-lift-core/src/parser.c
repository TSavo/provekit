#include "provekit/c_lift_core.h"

#include <ctype.h>
#include <regex.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define PK_C_NO_FUNCTION SIZE_MAX

static const char *pk_c_parser_first_nonblank(const char *line);

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

static int pk_c_parser_contract_annotation(const char *line) {
    return strstr(line, "//provekit:contract") != NULL;
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

static char *pk_c_parser_argument_text(const char *open_paren) {
    const char *start = open_paren + 1;
    const char *end = start;

    while (*end != '\0' && *end != ')') {
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
    for (i = 0; i < facts->n_call_sites; i++) {
        free(facts->call_sites[i].caller);
        free(facts->call_sites[i].callee);
        free(facts->call_sites[i].locus.path);
    }
    free(facts->functions);
    free(facts->macro_calls);
    free(facts->call_sites);
    pk_c_lift_result_free(facts->extraction_result);
    free(facts);
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

    if (facts == NULL) {
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
        int is_blank;

        next = strchr(line, '\n');
        if (next != NULL) {
            *next = '\0';
            next++;
        }

        is_blank = pk_c_parser_blank_line(line);
        if (pending_body != PK_C_NO_FUNCTION && !is_blank) {
            const char *first = pk_c_parser_first_nonblank(line);

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
                continue;
            }
        }

        if (pk_c_parser_contract_annotation(line)) {
            pending_contract = 1;
            continue;
        }

        if (!pk_c_parser_definition_disallowed_prefix(line) &&
            regexec(&function_re, line, 2, function_match, 0) == 0 &&
            function_match[1].rm_so >= 0) {
            size_t function_index;

            if (pk_c_parser_append_function(facts, path, line + function_match[1].rm_so,
                (size_t)(function_match[1].rm_eo - function_match[1].rm_so), line_no,
                (int)function_match[1].rm_so + 1, pending_contract) != 0) {
                pk_c_source_facts_free(facts);
                facts = NULL;
                goto done;
            }
            pending_contract = 0;
            function_index = facts->n_functions - 1;
            body_open = strchr(line + function_match[0].rm_eo, '{');
            if (body_open != NULL) {
                facts->functions[function_index].has_body = 1;
                if (pk_c_parser_scan_calls(facts, &call_re, path, body_open + 1, line_no,
                    (int)(body_open + 1 - line),
                    facts->functions[function_index].name) != 0) {
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
            continue;
        }

        if (!is_blank) {
            pending_contract = 0;
        }

        if (current_function != PK_C_NO_FUNCTION) {
            if (pk_c_parser_scan_calls(facts, &call_re, path, line, line_no, 0,
                facts->functions[current_function].name) != 0) {
                pk_c_source_facts_free(facts);
                facts = NULL;
                goto done;
            }
            brace_depth += pk_c_parser_brace_delta_from(line);
            if (brace_depth <= 0) {
                current_function = PK_C_NO_FUNCTION;
                brace_depth = 0;
            }
        }
    }

done:
    if (call_re_compiled) {
        regfree(&call_re);
    }
    if (function_re_compiled) {
        regfree(&function_re);
    }
    free(owned_source);
    return facts;
}
