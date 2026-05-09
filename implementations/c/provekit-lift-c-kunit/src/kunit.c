/* SPDX-License-Identifier: Apache-2.0 */

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/c_lift_core.h"

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} KBuf;

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} KStringSet;

typedef struct {
    char *name;
    size_t next_index;
} KCounter;

typedef struct {
    KCounter *items;
    size_t len;
    size_t cap;
} KCounterSet;

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} KArgList;

typedef enum {
    KUNIT_OP_EQ,
    KUNIT_OP_NE,
    KUNIT_OP_TRUE,
    KUNIT_OP_FALSE,
    KUNIT_OP_NULL,
    KUNIT_OP_NOT_NULL,
    KUNIT_OP_LT,
    KUNIT_OP_LE,
    KUNIT_OP_GT,
    KUNIT_OP_GE
} KunitOp;

typedef struct {
    KunitOp op;
    int required_args;
} KunitMacroInfo;

static char *k_copy_n(const char *src, size_t len) {
    char *out = malloc(len + 1);

    if (out == NULL) {
        return NULL;
    }
    memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

static char *k_copy(const char *src) {
    return k_copy_n(src == NULL ? "" : src, strlen(src == NULL ? "" : src));
}

static int kbuf_init(KBuf *b) {
    b->len = 0;
    b->cap = 256;
    b->data = malloc(b->cap);
    if (b->data == NULL) {
        b->cap = 0;
        return -1;
    }
    b->data[0] = '\0';
    return 0;
}

static void kbuf_free(KBuf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int kbuf_grow(KBuf *b, size_t need) {
    size_t next = b->cap ? b->cap : 256;
    char *data;

    while (next < b->len + need + 1) {
        if (next > ((size_t)-1) / 2) {
            return -1;
        }
        next *= 2;
    }
    data = realloc(b->data, next);
    if (data == NULL) {
        return -1;
    }
    b->data = data;
    b->cap = next;
    return 0;
}

static int kbuf_append_n(KBuf *b, const char *s, size_t n) {
    if (kbuf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int kbuf_append(KBuf *b, const char *s) {
    return kbuf_append_n(b, s, strlen(s));
}

static int kbuf_append_char(KBuf *b, char c) {
    return kbuf_append_n(b, &c, 1);
}

static int kbuf_append_quoted(KBuf *b, const char *s) {
    char *escaped = pk_c_lift_json_escape(s == NULL ? "" : s);
    int rc;

    if (escaped == NULL) {
        return -1;
    }
    rc = kbuf_append_char(b, '"') == 0 &&
        kbuf_append(b, escaped) == 0 &&
        kbuf_append_char(b, '"') == 0 ? 0 : -1;
    free(escaped);
    return rc;
}

static void k_set_error(char *err, size_t err_len, const char *msg) {
    if (err != NULL && err_len > 0) {
        (void)snprintf(err, err_len, "%s", msg);
    }
}

static void k_string_set_free(KStringSet *set) {
    if (set == NULL) {
        return;
    }
    for (size_t i = 0; i < set->len; i++) {
        free(set->items[i]);
    }
    free(set->items);
    memset(set, 0, sizeof(*set));
}

static int k_string_set_contains(const KStringSet *set, const char *s) {
    if (set == NULL || s == NULL) {
        return 0;
    }
    for (size_t i = 0; i < set->len; i++) {
        if (strcmp(set->items[i], s) == 0) {
            return 1;
        }
    }
    return 0;
}

static int k_string_set_add(KStringSet *set, const char *s) {
    char **items;

    if (s == NULL || s[0] == '\0' || k_string_set_contains(set, s)) {
        return 0;
    }
    if (set->len >= set->cap) {
        size_t cap = set->cap == 0 ? 8 : set->cap * 2;
        if (cap < set->cap) {
            return -1;
        }
        items = realloc(set->items, cap * sizeof(*set->items));
        if (items == NULL) {
            return -1;
        }
        set->items = items;
        set->cap = cap;
    }
    set->items[set->len] = k_copy(s);
    if (set->items[set->len] == NULL) {
        return -1;
    }
    set->len++;
    return 0;
}

static void k_counter_set_free(KCounterSet *set) {
    if (set == NULL) {
        return;
    }
    for (size_t i = 0; i < set->len; i++) {
        free(set->items[i].name);
    }
    free(set->items);
    memset(set, 0, sizeof(*set));
}

static int k_counter_next(KCounterSet *set, const char *name, size_t *idx) {
    KCounter *items;

    if (set == NULL || name == NULL || idx == NULL) {
        return -1;
    }
    for (size_t i = 0; i < set->len; i++) {
        if (strcmp(set->items[i].name, name) == 0) {
            *idx = set->items[i].next_index++;
            return 0;
        }
    }
    if (set->len >= set->cap) {
        size_t cap = set->cap == 0 ? 8 : set->cap * 2;
        if (cap < set->cap) {
            return -1;
        }
        items = realloc(set->items, cap * sizeof(*set->items));
        if (items == NULL) {
            return -1;
        }
        set->items = items;
        set->cap = cap;
    }
    set->items[set->len].name = k_copy(name);
    if (set->items[set->len].name == NULL) {
        return -1;
    }
    set->items[set->len].next_index = 1;
    *idx = 0;
    set->len++;
    return 0;
}

static void k_arg_list_free(KArgList *args) {
    if (args == NULL) {
        return;
    }
    for (size_t i = 0; i < args->len; i++) {
        free(args->items[i]);
    }
    free(args->items);
    memset(args, 0, sizeof(*args));
}

static int k_arg_list_push(KArgList *args, const char *start, size_t len) {
    char **items;
    char *copy;
    size_t first = 0;
    size_t last = len;

    while (first < len && isspace((unsigned char)start[first])) {
        first++;
    }
    while (last > first && isspace((unsigned char)start[last - 1])) {
        last--;
    }
    copy = k_copy_n(start + first, last - first);
    if (copy == NULL) {
        return -1;
    }
    if (args->len >= args->cap) {
        size_t cap = args->cap == 0 ? 4 : args->cap * 2;
        if (cap < args->cap) {
            free(copy);
            return -1;
        }
        items = realloc(args->items, cap * sizeof(*args->items));
        if (items == NULL) {
            free(copy);
            return -1;
        }
        args->items = items;
        args->cap = cap;
    }
    args->items[args->len++] = copy;
    return 0;
}

static int k_split_args(const char *text, KArgList *args) {
    const char *start = text == NULL ? "" : text;
    const char *p = start;
    int paren = 0;
    int bracket = 0;
    int brace = 0;
    char quote = '\0';

    memset(args, 0, sizeof(*args));
    for (; *p != '\0'; p++) {
        if (quote != '\0') {
            if (*p == '\\' && p[1] != '\0') {
                p++;
            } else if (*p == quote) {
                quote = '\0';
            }
            continue;
        }
        if (*p == '"' || *p == '\'') {
            quote = *p;
            continue;
        }
        if (*p == '(') {
            paren++;
        } else if (*p == ')' && paren > 0) {
            paren--;
        } else if (*p == '[') {
            bracket++;
        } else if (*p == ']' && bracket > 0) {
            bracket--;
        } else if (*p == '{') {
            brace++;
        } else if (*p == '}' && brace > 0) {
            brace--;
        } else if (*p == ',' && paren == 0 && bracket == 0 && brace == 0) {
            if (k_arg_list_push(args, start, (size_t)(p - start)) != 0) {
                k_arg_list_free(args);
                return -1;
            }
            start = p + 1;
        }
    }
    if (start != p || args->len > 0) {
        if (k_arg_list_push(args, start, (size_t)(p - start)) != 0) {
            k_arg_list_free(args);
            return -1;
        }
    }
    return 0;
}

static int k_is_ident_start(char c) {
    return isalpha((unsigned char)c) || c == '_';
}

static int k_is_ident_char(char c) {
    return isalnum((unsigned char)c) || c == '_';
}

static int k_is_identifier(const char *s) {
    if (s == NULL || !k_is_ident_start(*s)) {
        return 0;
    }
    for (s++; *s != '\0'; s++) {
        if (!k_is_ident_char(*s)) {
            return 0;
        }
    }
    return 1;
}

static char *k_code_view(const char *source) {
    char *copy = k_copy(source == NULL ? "" : source);
    size_t i = 0;
    int in_block = 0;
    char quote = '\0';

    if (copy == NULL) {
        return NULL;
    }
    while (copy[i] != '\0') {
        if (in_block) {
            if (copy[i] == '*' && copy[i + 1] == '/') {
                copy[i++] = ' ';
                copy[i++] = ' ';
                in_block = 0;
            } else if (copy[i] != '\n') {
                copy[i++] = ' ';
            } else {
                i++;
            }
            continue;
        }
        if (quote != '\0') {
            if (copy[i] == '\\' && copy[i + 1] != '\0') {
                copy[i++] = ' ';
                if (copy[i] != '\n') {
                    copy[i] = ' ';
                }
                i++;
                continue;
            }
            if (copy[i] == quote) {
                quote = '\0';
            }
            if (copy[i] != '\n') {
                copy[i] = ' ';
            }
            i++;
            continue;
        }
        if (copy[i] == '/' && copy[i + 1] == '/') {
            while (copy[i] != '\0' && copy[i] != '\n') {
                copy[i++] = ' ';
            }
            continue;
        }
        if (copy[i] == '/' && copy[i + 1] == '*') {
            copy[i++] = ' ';
            copy[i++] = ' ';
            in_block = 1;
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

static void k_skip_ws(const char **p) {
    while (**p != '\0' && isspace((unsigned char)**p)) {
        (*p)++;
    }
}

static int k_collect_kunit_cases(const char *source, KStringSet *registered) {
    char *code = k_code_view(source);
    const char *p;

    if (code == NULL) {
        return -1;
    }
    p = code;
    while (*p != '\0') {
        if (!k_is_ident_start(*p)) {
            p++;
            continue;
        }
        if (strncmp(p, "KUNIT_CASE", 10) == 0 && !k_is_ident_char(p[10])) {
            char name[256];
            size_t len = 0;

            p += 10;
            k_skip_ws(&p);
            if (*p != '(') {
                continue;
            }
            p++;
            k_skip_ws(&p);
            if (!k_is_ident_start(*p)) {
                continue;
            }
            while (k_is_ident_char(*p) && len + 1 < sizeof(name)) {
                name[len++] = *p++;
            }
            name[len] = '\0';
            if (k_string_set_add(registered, name) != 0) {
                free(code);
                return -1;
            }
        } else {
            p++;
            while (k_is_ident_char(*p)) {
                p++;
            }
        }
    }
    free(code);
    return 0;
}

static int k_segment_has_word(const char *start, const char *end, const char *word) {
    size_t n = strlen(word);

    for (const char *p = start; p < end; p++) {
        if ((p == start || !k_is_ident_char(p[-1])) &&
            (size_t)(end - p) >= n &&
            strncmp(p, word, n) == 0 &&
            (p + n == end || !k_is_ident_char(p[n]))) {
            return 1;
        }
    }
    return 0;
}

static int k_source_has_kunit_signature(const char *source, const char *name) {
    char *code = k_code_view(source);
    const char *p;
    size_t name_len;

    if (code == NULL || name == NULL) {
        free(code);
        return 0;
    }
    name_len = strlen(name);
    p = code;
    while ((p = strstr(p, name)) != NULL) {
        const char *before;
        const char *after;
        const char *params_start;
        const char *params_end;
        int depth = 1;

        if ((p > code && k_is_ident_char(p[-1])) || k_is_ident_char(p[name_len])) {
            p += name_len;
            continue;
        }
        before = p;
        while (before > code && before[-1] != ';' && before[-1] != '{' && before[-1] != '}') {
            before--;
        }
        after = p + name_len;
        while (isspace((unsigned char)*after)) {
            after++;
        }
        if (*after != '(' || !k_segment_has_word(before, p, "void")) {
            p += name_len;
            continue;
        }
        params_start = after + 1;
        params_end = params_start;
        while (*params_end != '\0' && depth > 0) {
            if (*params_end == '(') {
                depth++;
            } else if (*params_end == ')') {
                depth--;
                if (depth == 0) {
                    break;
                }
            }
            params_end++;
        }
        if (depth == 0 &&
            k_segment_has_word(params_start, params_end, "struct") &&
            k_segment_has_word(params_start, params_end, "kunit") &&
            k_segment_has_word(params_start, params_end, "test") &&
            memchr(params_start, '*', (size_t)(params_end - params_start)) != NULL) {
            free(code);
            return 1;
        }
        p += name_len;
    }
    free(code);
    return 0;
}

static int k_collect_test_functions(
    const char *source,
    const pk_c_source_facts *facts,
    const KStringSet *registered,
    KStringSet *tests
) {
    if (facts == NULL) {
        return 0;
    }
    for (size_t i = 0; i < facts->n_functions; i++) {
        const char *name = facts->functions[i].name;

        if (name == NULL || name[0] == '\0') {
            continue;
        }
        if (k_string_set_contains(registered, name) ||
            k_source_has_kunit_signature(source, name)) {
            if (k_string_set_add(tests, name) != 0) {
                return -1;
            }
        }
    }
    return 0;
}

static int k_macro_info(const char *name, KunitMacroInfo *info) {
    const char *tail;

    if (strncmp(name, "KUNIT_EXPECT_", 13) == 0) {
        tail = name + 13;
    } else if (strncmp(name, "KUNIT_ASSERT_", 13) == 0) {
        tail = name + 13;
    } else {
        return 0;
    }
    if (strcmp(tail, "EQ") == 0) {
        info->op = KUNIT_OP_EQ;
        info->required_args = 3;
    } else if (strcmp(tail, "NE") == 0) {
        info->op = KUNIT_OP_NE;
        info->required_args = 3;
    } else if (strcmp(tail, "TRUE") == 0) {
        info->op = KUNIT_OP_TRUE;
        info->required_args = 2;
    } else if (strcmp(tail, "FALSE") == 0) {
        info->op = KUNIT_OP_FALSE;
        info->required_args = 2;
    } else if (strcmp(tail, "NULL") == 0) {
        info->op = KUNIT_OP_NULL;
        info->required_args = 2;
    } else if (strcmp(tail, "NOT_NULL") == 0) {
        info->op = KUNIT_OP_NOT_NULL;
        info->required_args = 2;
    } else if (strcmp(tail, "LT") == 0) {
        info->op = KUNIT_OP_LT;
        info->required_args = 3;
    } else if (strcmp(tail, "LE") == 0) {
        info->op = KUNIT_OP_LE;
        info->required_args = 3;
    } else if (strcmp(tail, "GT") == 0) {
        info->op = KUNIT_OP_GT;
        info->required_args = 3;
    } else if (strcmp(tail, "GE") == 0) {
        info->op = KUNIT_OP_GE;
        info->required_args = 3;
    } else {
        return 0;
    }
    return 1;
}

static char *k_trim_copy(const char *s) {
    const char *start = s == NULL ? "" : s;
    const char *end;

    while (isspace((unsigned char)*start)) {
        start++;
    }
    end = start + strlen(start);
    while (end > start && isspace((unsigned char)end[-1])) {
        end--;
    }
    return k_copy_n(start, (size_t)(end - start));
}

static int k_matching_outer_parens(const char *s) {
    size_t len = strlen(s);
    int depth = 0;
    char quote = '\0';

    if (len < 2 || s[0] != '(' || s[len - 1] != ')') {
        return 0;
    }
    for (size_t i = 0; i < len; i++) {
        if (quote != '\0') {
            if (s[i] == '\\' && s[i + 1] != '\0') {
                i++;
            } else if (s[i] == quote) {
                quote = '\0';
            }
            continue;
        }
        if (s[i] == '"' || s[i] == '\'') {
            quote = s[i];
            continue;
        }
        if (s[i] == '(') {
            depth++;
        } else if (s[i] == ')') {
            depth--;
            if (depth == 0 && i + 1 < len) {
                return 0;
            }
        }
    }
    return depth == 0;
}

static void k_strip_outer_parens(char *s) {
    while (k_matching_outer_parens(s)) {
        size_t len = strlen(s);
        memmove(s, s + 1, len - 2);
        s[len - 2] = '\0';
        char *trimmed = k_trim_copy(s);
        if (trimmed == NULL) {
            return;
        }
        strcpy(s, trimmed);
        free(trimmed);
    }
}

static int k_parse_int_literal(const char *s, long *out) {
    char *end = NULL;
    long value;

    if (s == NULL || s[0] == '\0') {
        return 0;
    }
    value = strtol(s, &end, 10);
    if (end == s) {
        return 0;
    }
    while (*end != '\0') {
        if (!isspace((unsigned char)*end)) {
            return 0;
        }
        end++;
    }
    *out = value;
    return 1;
}

static int k_append_const_int(KBuf *out, long value) {
    char buf[64];

    (void)snprintf(buf, sizeof(buf), "%ld", value);
    return kbuf_append(out, "{\"kind\":\"const\",\"value\":") == 0 &&
        kbuf_append(out, buf) == 0 &&
        kbuf_append(out, ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}") == 0 ? 0 : -1;
}

static int k_append_const_bool(KBuf *out, int value) {
    return kbuf_append(out, "{\"kind\":\"const\",\"value\":") == 0 &&
        kbuf_append(out, value ? "true" : "false") == 0 &&
        kbuf_append(out, ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}") == 0 ? 0 : -1;
}

static int k_append_const_string(KBuf *out, const char *quoted) {
    size_t len = strlen(quoted);
    char *inner;
    int rc;

    if (len < 2 || quoted[0] != '"' || quoted[len - 1] != '"') {
        return -1;
    }
    inner = k_copy_n(quoted + 1, len - 2);
    if (inner == NULL) {
        return -1;
    }
    rc = kbuf_append(out, "{\"kind\":\"const\",\"value\":") == 0 &&
        kbuf_append_quoted(out, inner) == 0 &&
        kbuf_append(out, ",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}") == 0 ? 0 : -1;
    free(inner);
    return rc;
}

static int k_append_ctor_null(KBuf *out) {
    return kbuf_append(out, "{\"kind\":\"ctor\",\"name\":\"NULL\",\"args\":[]}");
}

static int k_find_top_op(const char *s, const char *const *ops, size_t n_ops, size_t *pos, size_t *op_len) {
    int paren = 0;
    int bracket = 0;
    int brace = 0;
    char quote = '\0';
    size_t len = strlen(s);

    for (size_t i = len; i > 0; i--) {
        size_t at = i - 1;

        if (quote != '\0') {
            if (s[at] == quote && (at == 0 || s[at - 1] != '\\')) {
                quote = '\0';
            }
            continue;
        }
        if (s[at] == '"' || s[at] == '\'') {
            quote = s[at];
            continue;
        }
        if (s[at] == ')') {
            paren++;
            continue;
        }
        if (s[at] == '(' && paren > 0) {
            paren--;
            continue;
        }
        if (s[at] == ']') {
            bracket++;
            continue;
        }
        if (s[at] == '[' && bracket > 0) {
            bracket--;
            continue;
        }
        if (s[at] == '}') {
            brace++;
            continue;
        }
        if (s[at] == '{' && brace > 0) {
            brace--;
            continue;
        }
        if (paren != 0 || bracket != 0 || brace != 0) {
            continue;
        }
        for (size_t j = 0; j < n_ops; j++) {
            size_t n = strlen(ops[j]);

            if (at + n <= len && strncmp(s + at, ops[j], n) == 0) {
                if ((ops[j][0] == '-' || ops[j][0] == '+') && at == 0) {
                    continue;
                }
                *pos = at;
                *op_len = n;
                return 1;
            }
        }
    }
    return 0;
}

static const char *k_term_op_name(const char *op, size_t len) {
    if (len == 1) {
        return op;
    }
    if (len == 2 && strncmp(op, "<<", 2) == 0) {
        return "<<";
    }
    if (len == 2 && strncmp(op, ">>", 2) == 0) {
        return ">>";
    }
    return NULL;
}

static int k_append_term_json(KBuf *out, const char *expr, char *err, size_t err_len);

static int k_append_call_term(KBuf *out, const char *expr, char *err, size_t err_len) {
    const char *open = strchr(expr, '(');
    const char *end;
    char *callee;
    char *inside;
    KArgList args;
    int rc = -1;

    if (open == NULL || expr[strlen(expr) - 1] != ')') {
        return 0;
    }
    for (const char *p = expr; p < open; p++) {
        if (!k_is_ident_char(*p)) {
            return 0;
        }
    }
    if (open == expr) {
        return 0;
    }
    end = open + 1;
    {
        int depth = 1;
        char quote = '\0';
        while (*end != '\0' && depth > 0) {
            if (quote != '\0') {
                if (*end == '\\' && end[1] != '\0') {
                    end++;
                } else if (*end == quote) {
                    quote = '\0';
                }
            } else if (*end == '"' || *end == '\'') {
                quote = *end;
            } else if (*end == '(') {
                depth++;
            } else if (*end == ')') {
                depth--;
                if (depth == 0) {
                    break;
                }
            }
            end++;
        }
        if (depth != 0 || end[1] != '\0') {
            return 0;
        }
    }
    callee = k_copy_n(expr, (size_t)(open - expr));
    inside = k_copy_n(open + 1, (size_t)(end - open - 1));
    if (callee == NULL || inside == NULL) {
        free(callee);
        free(inside);
        return -1;
    }
    if (k_split_args(inside, &args) != 0) {
        free(callee);
        free(inside);
        return -1;
    }
    if (kbuf_append(out, "{\"kind\":\"ctor\",\"name\":") != 0 ||
        kbuf_append_quoted(out, callee) != 0 ||
        kbuf_append(out, ",\"args\":[") != 0) {
        goto cleanup;
    }
    for (size_t i = 0; i < args.len; i++) {
        if (i > 0 && kbuf_append_char(out, ',') != 0) {
            goto cleanup;
        }
        if (k_append_term_json(out, args.items[i], err, err_len) != 0) {
            goto cleanup;
        }
    }
    if (kbuf_append(out, "]}") != 0) {
        goto cleanup;
    }
    rc = 1;

cleanup:
    k_arg_list_free(&args);
    free(callee);
    free(inside);
    return rc;
}

static int k_append_term_json(KBuf *out, const char *expr, char *err, size_t err_len) {
    static const char *const ops[] = {"|", "^", "&", "<<", ">>", "+", "-", "*", "/", "%"};
    char *s = k_trim_copy(expr);
    long value;
    size_t pos;
    size_t op_len;
    int call_rc;

    if (s == NULL) {
        return -1;
    }
    k_strip_outer_parens(s);
    if (s[0] == '\0') {
        k_set_error(err, err_len, "empty expression");
        free(s);
        return -1;
    }
    if (s[0] == '&' && s[1] != '&') {
        int rc = k_append_term_json(out, s + 1, err, err_len);
        free(s);
        return rc;
    }
    if (k_parse_int_literal(s, &value)) {
        int rc = k_append_const_int(out, value);
        free(s);
        return rc;
    }
    if (s[0] == '"') {
        int rc = k_append_const_string(out, s);
        free(s);
        return rc;
    }
    if (strcmp(s, "true") == 0) {
        free(s);
        return k_append_const_bool(out, 1);
    }
    if (strcmp(s, "false") == 0) {
        free(s);
        return k_append_const_bool(out, 0);
    }
    if (strcmp(s, "NULL") == 0) {
        free(s);
        return k_append_ctor_null(out);
    }
    call_rc = k_append_call_term(out, s, err, err_len);
    if (call_rc != 0) {
        free(s);
        return call_rc == 1 ? 0 : -1;
    }
    if (k_find_top_op(s, ops, sizeof(ops) / sizeof(ops[0]), &pos, &op_len)) {
        char *lhs = k_copy_n(s, pos);
        char *rhs = k_copy(s + pos + op_len);
        const char *name = k_term_op_name(s + pos, op_len);
        int rc = -1;

        if (lhs == NULL || rhs == NULL || name == NULL) {
            free(lhs);
            free(rhs);
            free(s);
            return -1;
        }
        if (kbuf_append(out, "{\"kind\":\"ctor\",\"name\":") == 0 &&
            kbuf_append_quoted(out, name) == 0 &&
            kbuf_append(out, ",\"args\":[") == 0 &&
            k_append_term_json(out, lhs, err, err_len) == 0 &&
            kbuf_append_char(out, ',') == 0 &&
            k_append_term_json(out, rhs, err, err_len) == 0 &&
            kbuf_append(out, "]}") == 0) {
            rc = 0;
        }
        free(lhs);
        free(rhs);
        free(s);
        return rc;
    }
    if (k_is_identifier(s)) {
        int rc = kbuf_append(out, "{\"kind\":\"var\",\"name\":") == 0 &&
            kbuf_append_quoted(out, s) == 0 &&
            kbuf_append_char(out, '}') == 0 ? 0 : -1;
        free(s);
        return rc;
    }
    k_set_error(err, err_len, "expression shape not in C KUnit lift whitelist");
    free(s);
    return -1;
}

static int k_append_atomic2(KBuf *out, const char *name, const char *lhs, const char *rhs, char *err, size_t err_len) {
    return kbuf_append(out, "{\"kind\":\"atomic\",\"name\":") == 0 &&
        kbuf_append_quoted(out, name) == 0 &&
        kbuf_append(out, ",\"args\":[") == 0 &&
        k_append_term_json(out, lhs, err, err_len) == 0 &&
        kbuf_append_char(out, ',') == 0 &&
        k_append_term_json(out, rhs, err, err_len) == 0 &&
        kbuf_append(out, "]}") == 0 ? 0 : -1;
}

static int k_append_null_atomic(KBuf *out, const char *name, const char *expr, char *err, size_t err_len) {
    return kbuf_append(out, "{\"kind\":\"atomic\",\"name\":") == 0 &&
        kbuf_append_quoted(out, name) == 0 &&
        kbuf_append(out, ",\"args\":[") == 0 &&
        k_append_term_json(out, expr, err, err_len) == 0 &&
        kbuf_append(out, ",{\"kind\":\"ctor\",\"name\":\"NULL\",\"args\":[]}]}") == 0 ? 0 : -1;
}

static const char *k_cmp_name(const char *op, size_t len) {
    if (len == 2 && strncmp(op, "==", 2) == 0) return "eq";
    if (len == 2 && strncmp(op, "!=", 2) == 0) return "ne";
    if (len == 2 && strncmp(op, "<=", 2) == 0) return "le";
    if (len == 2 && strncmp(op, ">=", 2) == 0) return "ge";
    if (len == 1 && op[0] == '<') return "lt";
    if (len == 1 && op[0] == '>') return "gt";
    return NULL;
}

static int k_find_top_comparison(const char *s, size_t *pos, size_t *op_len, const char **name) {
    static const char *const ops[] = {"==", "!=", "<=", ">=", "<", ">"};
    int paren = 0;
    int bracket = 0;
    int brace = 0;
    char quote = '\0';
    size_t len = strlen(s);

    for (size_t i = 0; i < len; i++) {
        if (quote != '\0') {
            if (s[i] == '\\' && s[i + 1] != '\0') {
                i++;
            } else if (s[i] == quote) {
                quote = '\0';
            }
            continue;
        }
        if (s[i] == '"' || s[i] == '\'') {
            quote = s[i];
            continue;
        }
        if (s[i] == '(') paren++;
        else if (s[i] == ')' && paren > 0) paren--;
        else if (s[i] == '[') bracket++;
        else if (s[i] == ']' && bracket > 0) bracket--;
        else if (s[i] == '{') brace++;
        else if (s[i] == '}' && brace > 0) brace--;
        if (paren != 0 || bracket != 0 || brace != 0) {
            continue;
        }
        for (size_t j = 0; j < sizeof(ops) / sizeof(ops[0]); j++) {
            size_t n = strlen(ops[j]);

            if (i + n <= len && strncmp(s + i, ops[j], n) == 0) {
                if ((ops[j][0] == '<' && s[i + 1] == '<') ||
                    (ops[j][0] == '>' && s[i + 1] == '>')) {
                    continue;
                }
                *pos = i;
                *op_len = n;
                *name = k_cmp_name(s + i, n);
                return *name != NULL;
            }
        }
    }
    return 0;
}

static int k_append_formula_json(KBuf *out, const char *expr, char *err, size_t err_len);

static int k_append_connective(KBuf *out, const char *kind, const char *lhs, const char *rhs, char *err, size_t err_len) {
    return kbuf_append(out, "{\"kind\":") == 0 &&
        kbuf_append_quoted(out, kind) == 0 &&
        kbuf_append(out, ",\"operands\":[") == 0 &&
        k_append_formula_json(out, lhs, err, err_len) == 0 &&
        kbuf_append_char(out, ',') == 0 &&
        k_append_formula_json(out, rhs, err, err_len) == 0 &&
        kbuf_append(out, "]}") == 0 ? 0 : -1;
}

static int k_find_top_logic(const char *s, const char *op, size_t *pos) {
    int paren = 0;
    char quote = '\0';
    size_t len = strlen(s);
    size_t op_len = strlen(op);

    for (size_t i = 0; i + op_len <= len; i++) {
        if (quote != '\0') {
            if (s[i] == '\\' && s[i + 1] != '\0') {
                i++;
            } else if (s[i] == quote) {
                quote = '\0';
            }
            continue;
        }
        if (s[i] == '"' || s[i] == '\'') {
            quote = s[i];
            continue;
        }
        if (s[i] == '(') {
            paren++;
            continue;
        }
        if (s[i] == ')' && paren > 0) {
            paren--;
            continue;
        }
        if (paren == 0 && strncmp(s + i, op, op_len) == 0) {
            *pos = i;
            return 1;
        }
    }
    return 0;
}

static int k_append_formula_json(KBuf *out, const char *expr, char *err, size_t err_len) {
    char *s = k_trim_copy(expr);
    size_t pos;
    size_t op_len;
    const char *name;

    if (s == NULL) {
        return -1;
    }
    k_strip_outer_parens(s);
    if (s[0] == '!' && s[1] != '=') {
        int rc = kbuf_append(out, "{\"kind\":\"not\",\"operands\":[") == 0 &&
            k_append_formula_json(out, s + 1, err, err_len) == 0 &&
            kbuf_append(out, "]}") == 0 ? 0 : -1;
        free(s);
        return rc;
    }
    if (k_find_top_logic(s, "||", &pos)) {
        char *lhs = k_copy_n(s, pos);
        char *rhs = k_copy(s + pos + 2);
        int rc = lhs != NULL && rhs != NULL
            ? k_append_connective(out, "or", lhs, rhs, err, err_len)
            : -1;
        free(lhs);
        free(rhs);
        free(s);
        return rc;
    }
    if (k_find_top_logic(s, "&&", &pos)) {
        char *lhs = k_copy_n(s, pos);
        char *rhs = k_copy(s + pos + 2);
        int rc = lhs != NULL && rhs != NULL
            ? k_append_connective(out, "and", lhs, rhs, err, err_len)
            : -1;
        free(lhs);
        free(rhs);
        free(s);
        return rc;
    }
    if (k_find_top_comparison(s, &pos, &op_len, &name)) {
        char *lhs = k_copy_n(s, pos);
        char *rhs = k_copy(s + pos + op_len);
        int rc = lhs != NULL && rhs != NULL
            ? k_append_atomic2(out, name, lhs, rhs, err, err_len)
            : -1;
        free(lhs);
        free(rhs);
        free(s);
        return rc;
    }
    {
        int rc = kbuf_append(out, "{\"kind\":\"atomic\",\"name\":\"truthy\",\"args\":[") == 0 &&
            k_append_term_json(out, s, err, err_len) == 0 &&
            kbuf_append(out, "]}") == 0 ? 0 : -1;
        free(s);
        return rc;
    }
}

static int k_build_formula(KBuf *formula, KunitOp op, const KArgList *args, char *err, size_t err_len) {
    const char *a = args->items[1];
    const char *b = args->len > 2 ? args->items[2] : NULL;

    if (kbuf_init(formula) != 0) {
        return -1;
    }
    switch (op) {
    case KUNIT_OP_EQ:
        return k_append_atomic2(formula, "eq", a, b, err, err_len);
    case KUNIT_OP_NE:
        return k_append_atomic2(formula, "ne", a, b, err, err_len);
    case KUNIT_OP_TRUE:
        return k_append_formula_json(formula, a, err, err_len);
    case KUNIT_OP_FALSE:
        return kbuf_append(formula, "{\"kind\":\"not\",\"operands\":[") == 0 &&
            k_append_formula_json(formula, a, err, err_len) == 0 &&
            kbuf_append(formula, "]}") == 0 ? 0 : -1;
    case KUNIT_OP_NULL:
        return k_append_null_atomic(formula, "eq", a, err, err_len);
    case KUNIT_OP_NOT_NULL:
        return k_append_null_atomic(formula, "ne", a, err, err_len);
    case KUNIT_OP_LT:
        return k_append_atomic2(formula, "lt", a, b, err, err_len);
    case KUNIT_OP_LE:
        return k_append_atomic2(formula, "le", a, b, err, err_len);
    case KUNIT_OP_GT:
        return k_append_atomic2(formula, "gt", a, b, err, err_len);
    case KUNIT_OP_GE:
        return k_append_atomic2(formula, "ge", a, b, err, err_len);
    }
    k_set_error(err, err_len, "unknown KUnit operation");
    return -1;
}

static int k_build_contract(
    KBuf *out,
    const char *test_name,
    size_t index,
    const char *post,
    const pk_c_locus *locus,
    const char *fallback_path
) {
    char index_buf[64];

    (void)snprintf(index_buf, sizeof(index_buf), "%zu", index);
    if (kbuf_init(out) != 0) {
        return -1;
    }
    if (kbuf_append(out, "{\"fn_name\":") != 0 ||
        kbuf_append_char(out, '"') != 0 ||
        kbuf_append(out, test_name) != 0 ||
        kbuf_append(out, "::") != 0 ||
        kbuf_append(out, index_buf) != 0 ||
        kbuf_append(out, "\",\"kind\":\"function-contract\",") != 0 ||
        kbuf_append(out, "\"formals\":[],\"formal_sorts\":[],") != 0 ||
        kbuf_append(out, "\"return_sort\":{\"kind\":\"primitive\",\"name\":\"i32\"},") != 0 ||
        kbuf_append(out, "\"pre\":{\"kind\":\"atomic\",\"name\":\"true\",\"args\":[]},") != 0 ||
        kbuf_append(out, "\"post\":") != 0 ||
        kbuf_append(out, post) != 0 ||
        kbuf_append(out, ",\"effects\":{\"effects\":[]},") != 0 ||
        kbuf_append(out, "\"locus\":{\"col\":") != 0) {
        return -1;
    }
    (void)snprintf(index_buf, sizeof(index_buf), "%d", locus == NULL ? 0 : locus->column);
    if (kbuf_append(out, index_buf) != 0 ||
        kbuf_append(out, ",\"file\":") != 0 ||
        kbuf_append_quoted(out, locus != NULL && locus->path != NULL ? locus->path : fallback_path) != 0 ||
        kbuf_append(out, ",\"line\":") != 0) {
        return -1;
    }
    (void)snprintf(index_buf, sizeof(index_buf), "%d", locus == NULL ? 0 : locus->line);
    return kbuf_append(out, index_buf) == 0 &&
        kbuf_append(out, "},\"auto_minted_mementos\":[],\"body_cid\":null}") == 0 ? 0 : -1;
}

static int k_append_core_result(pk_c_lift_result *result, const pk_c_source_facts *facts) {
    if (facts == NULL || facts->extraction_result == NULL) {
        return 0;
    }
    return pk_c_lift_result_extend(result, facts->extraction_result);
}

static int k_add_skip_opacity(
    pk_c_lift_result *result,
    const pk_c_macro_call_fact *call,
    const char *reason
) {
    return pk_c_lift_result_add_opacity_entry(
        result,
        "c-kunit.unsupported-assertion",
        call->locus.path,
        call->locus.line,
        call->locus.column,
        reason,
        "c-kunit");
}

static int k_lift_kunit_macros(
    pk_c_lift_result *result,
    const char *path,
    const pk_c_source_facts *facts,
    const KStringSet *tests
) {
    KCounterSet counters = {0};
    int rc = 0;

    if (facts == NULL) {
        return 0;
    }
    for (size_t i = 0; i < facts->n_macro_calls; i++) {
        const pk_c_macro_call_fact *call = &facts->macro_calls[i];
        KunitMacroInfo info;
        KArgList args;
        KBuf formula;
        KBuf contract;
        size_t idx;
        char err[256];

        if (call->name == NULL || !k_macro_info(call->name, &info)) {
            continue;
        }
        if (!k_string_set_contains(tests, call->enclosing_function)) {
            continue;
        }
        if (k_counter_next(&counters, call->enclosing_function, &idx) != 0) {
            rc = -1;
            break;
        }
        if (k_split_args(call->argument_text, &args) != 0) {
            rc = -1;
            break;
        }
        if ((int)args.len < info.required_args) {
            (void)k_add_skip_opacity(result, call, "KUnit assertion had fewer arguments than expected");
            k_arg_list_free(&args);
            continue;
        }
        err[0] = '\0';
        if (k_build_formula(&formula, info.op, &args, err, sizeof(err)) != 0) {
            (void)k_add_skip_opacity(
                result,
                call,
                err[0] != '\0' ? err : "KUnit assertion expression could not be lifted");
            kbuf_free(&formula);
            k_arg_list_free(&args);
            continue;
        }
        if (k_build_contract(&contract, call->enclosing_function, idx, formula.data,
                &call->locus, path) != 0) {
            kbuf_free(&formula);
            k_arg_list_free(&args);
            rc = -1;
            break;
        }
        if (pk_c_lift_result_add_declaration(result, contract.data) != 0) {
            kbuf_free(&contract);
            kbuf_free(&formula);
            k_arg_list_free(&args);
            rc = -1;
            break;
        }
        kbuf_free(&contract);
        kbuf_free(&formula);
        k_arg_list_free(&args);
    }
    k_counter_set_free(&counters);
    return rc;
}

pk_c_lift_result *pk_c_kunit_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    pk_c_source_facts *core_facts;
    pk_c_source_facts *scan_facts;
    KStringSet registered = {0};
    KStringSet tests = {0};

    if (result == NULL) {
        return NULL;
    }
    if (source == NULL) {
        return result;
    }
    core_facts = pk_c_parse_source_with_options(path, source, options);
    if (core_facts == NULL) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"severity\":\"error\",\"message\":\"parse failed\"}");
        return result;
    }
    if (k_append_core_result(result, core_facts) != 0) {
        pk_c_source_facts_free(core_facts);
        pk_c_lift_result_free(result);
        return NULL;
    }
    if (options != NULL && options->backend == PK_C_PARSE_BACKEND_CLANG_AST) {
        scan_facts = pk_c_parse_source(path, source);
        if (scan_facts == NULL) {
            pk_c_source_facts_free(core_facts);
            pk_c_lift_result_free(result);
            return NULL;
        }
    } else {
        scan_facts = core_facts;
    }
    if (k_collect_kunit_cases(source, &registered) != 0 ||
        k_collect_test_functions(source, scan_facts, &registered, &tests) != 0 ||
        k_lift_kunit_macros(result, path, scan_facts, &tests) != 0) {
        if (scan_facts != core_facts) {
            pk_c_source_facts_free(scan_facts);
        }
        pk_c_source_facts_free(core_facts);
        k_string_set_free(&registered);
        k_string_set_free(&tests);
        pk_c_lift_result_free(result);
        return NULL;
    }
    if (scan_facts != core_facts) {
        pk_c_source_facts_free(scan_facts);
    }
    pk_c_source_facts_free(core_facts);
    k_string_set_free(&registered);
    k_string_set_free(&tests);
    return result;
}

pk_c_lift_result *pk_c_kunit_lift_source(const char *path, const char *source) {
    return pk_c_kunit_lift_source_with_options(path, source, NULL);
}
