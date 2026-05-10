/* SPDX-License-Identifier: Apache-2.0 */

#include "contract.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char *copy_n(const char *src, size_t len) {
    char *out = malloc(len + 1);

    if (out == NULL) {
        return NULL;
    }
    memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

static char *trim_copy_n(const char *start, size_t len) {
    const char *end = start + len;

    while (start < end && isspace((unsigned char)*start)) {
        start++;
    }
    while (end > start && isspace((unsigned char)end[-1])) {
        end--;
    }
    return copy_n(start, (size_t)(end - start));
}

static int is_ident_char(int ch) {
    return isalnum((unsigned char)ch) || ch == '_';
}

static int token_boundary_before(const char *base, const char *p) {
    return p == base || !is_ident_char((unsigned char)p[-1]);
}

static int token_boundary_after(const char *p, const char *end) {
    return p >= end || !is_ident_char((unsigned char)*p);
}

static int contains_token(const char *start, const char *end, const char *token) {
    size_t len = strlen(token);
    const char *p = start;

    while (p < end && (p = strstr(p, token)) != NULL && p < end) {
        if (p + len <= end &&
            token_boundary_before(start, p) &&
            token_boundary_after(p + len, end)) {
            return 1;
        }
        p += len;
    }
    return 0;
}

static int is_void_param(const char *param) {
    const char *p = param;

    while (isspace((unsigned char)*p)) {
        p++;
    }
    if (strncmp(p, "void", 4) != 0 || is_ident_char((unsigned char)p[4])) {
        return 0;
    }
    p += 4;
    while (isspace((unsigned char)*p)) {
        p++;
    }
    return *p == '\0';
}

static char *fallback_formal(size_t index) {
    char name[32];

    (void)snprintf(name, sizeof(name), "x%zu", index);
    return copy_n(name, strlen(name));
}

static int reserved_param_word(const char *name) {
    static const char *const reserved[] = {
        "const", "volatile", "restrict", "struct", "union", "enum",
        "__user", "__rcu", "size_t", "gfp_t", "void", "char", "int",
        "long", "short", "signed", "unsigned"
    };

    for (size_t i = 0; i < sizeof(reserved) / sizeof(reserved[0]); i++) {
        if (strcmp(name, reserved[i]) == 0) {
            return 1;
        }
    }
    return 0;
}

static char *extract_param_name(const char *param, size_t index) {
    size_t len = strlen(param);
    size_t end = len;
    size_t start;
    char *name;

    while (end > 0 && isspace((unsigned char)param[end - 1])) {
        end--;
    }
    while (end > 0 && param[end - 1] == ']') {
        while (end > 0 && param[end - 1] != '[') {
            end--;
        }
        if (end > 0) {
            end--;
        }
        while (end > 0 && isspace((unsigned char)param[end - 1])) {
            end--;
        }
    }
    start = end;
    while (start > 0 && is_ident_char((unsigned char)param[start - 1])) {
        start--;
    }
    if (start == end) {
        return fallback_formal(index);
    }
    name = copy_n(param + start, end - start);
    if (name == NULL) {
        return NULL;
    }
    if (reserved_param_word(name)) {
        free(name);
        return fallback_formal(index);
    }
    return name;
}

static int add_param_predicates(pk_c_walker_contract *contract, const char *param, const char *name) {
    const char *start = param;
    const char *end = param + strlen(param);

    if (contains_token(start, end, "__user") &&
        pk_c_walker_contract_add_pre_take(
            contract,
            pk_c_walker_formula_atomic1_var("is_user_ptr", name)) != 0) {
        return -1;
    }
    if (contains_token(start, end, "__rcu") &&
        pk_c_walker_contract_add_pre_take(
            contract,
            pk_c_walker_formula_atomic1_var("is_rcu_protected", name)) != 0) {
        return -1;
    }
    if (contains_token(start, end, "size_t") &&
        pk_c_walker_contract_add_pre_take(
            contract,
            pk_c_walker_formula_atomic2_var_const(">=", name, 0)) != 0) {
        return -1;
    }
    if (contains_token(start, end, "gfp_t") &&
        pk_c_walker_contract_add_pre_take(
            contract,
            pk_c_walker_formula_atomic1_var("valid_gfp_flags", name)) != 0) {
        return -1;
    }
    return 0;
}

static int split_params(
    const pk_c_walker_function_span *span,
    pk_c_walker_contract *contract
) {
    const char *part = span->params_start;
    const char *p = span->params_start;
    int depth = 0;
    size_t index = 0;

    while (p <= span->params_end) {
        if (p == span->params_end || (*p == ',' && depth == 0)) {
            char *param = trim_copy_n(part, (size_t)(p - part));
            char *name;

            if (param == NULL) {
                return -1;
            }
            if (param[0] != '\0' && !is_void_param(param)) {
                name = extract_param_name(param, index);
                if (name == NULL) {
                    free(param);
                    return -1;
                }
                if (pk_c_walker_contract_add_formal(contract, name) != 0 ||
                    add_param_predicates(contract, param, name) != 0) {
                    free(name);
                    free(param);
                    return -1;
                }
                free(name);
                index++;
            }
            free(param);
            part = p + 1;
        } else if (*p == '(' || *p == '[') {
            depth++;
        } else if ((*p == ')' || *p == ']') && depth > 0) {
            depth--;
        }
        p++;
    }
    return 0;
}

static char *clean_lock_arg(const char *start, const char *end) {
    char *arg;
    char *p;

    while (start < end && isspace((unsigned char)*start)) {
        start++;
    }
    while (end > start && isspace((unsigned char)end[-1])) {
        end--;
    }
    if (start < end && *start == '&') {
        start++;
        while (start < end && isspace((unsigned char)*start)) {
            start++;
        }
    }
    arg = copy_n(start, (size_t)(end - start));
    if (arg == NULL) {
        return NULL;
    }
    p = arg;
    while (*p != '\0') {
        if (isspace((unsigned char)*p)) {
            *p = '\0';
            break;
        }
        p++;
    }
    if (arg[0] == '\0') {
        free(arg);
        return copy_n("lock", strlen("lock"));
    }
    return arg;
}

static int json_escape_append(char **out, size_t *len, size_t *cap, const char *s) {
    const unsigned char *p = (const unsigned char *)(s == NULL ? "" : s);

    for (; *p != '\0'; p++) {
        const char *piece = NULL;
        char small[7];
        size_t n;

        switch (*p) {
        case '"':
            piece = "\\\"";
            break;
        case '\\':
            piece = "\\\\";
            break;
        case '\n':
            piece = "\\n";
            break;
        case '\r':
            piece = "\\r";
            break;
        case '\t':
            piece = "\\t";
            break;
        default:
            if (*p < 0x20) {
                (void)snprintf(small, sizeof(small), "\\u00%02x", *p);
                piece = small;
            } else {
                small[0] = (char)*p;
                small[1] = '\0';
                piece = small;
            }
            break;
        }
        n = strlen(piece);
        if (*len + n + 1 > *cap) {
            size_t next = *cap ? *cap : 64;
            char *data;

            while (next < *len + n + 1) {
                if (next > ((size_t)-1) / 2) {
                    return -1;
                }
                next *= 2;
            }
            data = realloc(*out, next);
            if (data == NULL) {
                return -1;
            }
            *out = data;
            *cap = next;
        }
        memcpy(*out + *len, piece, n);
        *len += n;
        (*out)[*len] = '\0';
    }
    return 0;
}

static char *effect_json(const char *kind, const char *target) {
    char *escaped = NULL;
    size_t len = 0;
    size_t cap = 0;
    int written;
    char *json;

    if (json_escape_append(&escaped, &len, &cap, target) != 0) {
        free(escaped);
        return NULL;
    }
    if (escaped == NULL) {
        escaped = copy_n("", 0);
        if (escaped == NULL) {
            return NULL;
        }
    }
    written = snprintf(NULL, 0, "{\"kind\":\"%s\",\"target\":\"%s\"}", kind, escaped);
    if (written < 0) {
        free(escaped);
        return NULL;
    }
    json = malloc((size_t)written + 1);
    if (json == NULL) {
        free(escaped);
        return NULL;
    }
    (void)snprintf(json, (size_t)written + 1,
        "{\"kind\":\"%s\",\"target\":\"%s\"}", kind, escaped);
    free(escaped);
    return json;
}

static int scan_attribute(
    pk_c_walker_contract *contract,
    const char *start,
    const char *end,
    const char *attr,
    const char *pre_name,
    const char *effect_kind
) {
    size_t attr_len = strlen(attr);
    const char *p = start;

    while (p < end && (p = strstr(p, attr)) != NULL && p < end) {
        const char *open;
        const char *close;
        char *arg;

        if (p + attr_len > end ||
            !token_boundary_before(start, p) ||
            !token_boundary_after(p + attr_len, end)) {
            p += attr_len;
            continue;
        }
        open = p + attr_len;
        while (open < end && isspace((unsigned char)*open)) {
            open++;
        }
        if (open >= end || *open != '(') {
            p += attr_len;
            continue;
        }
        close = open + 1;
        while (close < end && *close != ')') {
            close++;
        }
        if (close >= end) {
            return -1;
        }
        arg = clean_lock_arg(open + 1, close);
        if (arg == NULL) {
            return -1;
        }
        if (pre_name != NULL &&
            pk_c_walker_contract_add_pre_take(
                contract,
                pk_c_walker_formula_atomic1_var(pre_name, arg)) != 0) {
            free(arg);
            return -1;
        }
        if (effect_kind != NULL &&
            pk_c_walker_contract_add_effect_take(
                contract,
                effect_json(effect_kind, arg)) != 0) {
            free(arg);
            return -1;
        }
        free(arg);
        p = close + 1;
    }
    return 0;
}

static int add_signature_attributes(
    const pk_c_walker_function_span *span,
    pk_c_walker_contract *contract
) {
    const char *start = span->signature_start;
    const char *end = span->body_start - 1;

    if (scan_attribute(contract, start, end, "__must_hold", "lock_held", NULL) != 0) {
        return -1;
    }
    if (scan_attribute(contract, start, end, "__acquires", NULL, "lock_acquire") != 0) {
        return -1;
    }
    return scan_attribute(contract, start, end, "__releases", NULL, "lock_release");
}

int pk_c_walker_extract_type_predicates(
    const char *source,
    const char *fn_name,
    pk_c_walker_contract *contract
) {
    pk_c_walker_function_span span;

    if (!pk_c_walker_find_function_source(source, fn_name, &span)) {
        return 0;
    }
    if (split_params(&span, contract) != 0) {
        return -1;
    }
    return add_signature_attributes(&span, contract);
}
